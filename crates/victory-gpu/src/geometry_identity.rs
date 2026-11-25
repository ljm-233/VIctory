//! Capture-time identification of the native vertex stream bound to a mesh draw.
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

type Digest = [u8; 32];
type Index = HashMap<Digest, Vec<(usize, Digest)>>;

fn parse_index(bytes: &[u8]) -> Option<Index> {
    let records = bytes.strip_prefix(b"VIGEO001")?.chunks_exact(68);
    if !records.remainder().is_empty() {
        return None;
    }
    let mut index = Index::new();
    for record in records {
        let key: Digest = record[..32].try_into().ok()?;
        let size = u32::from_le_bytes(record[32..36].try_into().ok()?) as usize;
        if size < 128 {
            return None;
        }
        let digest = record[36..].try_into().ok()?;
        index.entry(key).or_default().push((size, digest));
    }
    Some(index)
}

/// Tags the recorded draw with hashes of the bytes read from its guest root SRV.
/// Always read afresh since two draws may reuse one address with different contents.
pub(crate) fn identify(address: u64, available: u64) -> Option<String> {
    static INDEX: OnceLock<Option<Index>> = OnceLock::new();
    let index = INDEX
        .get_or_init(|| {
            let path = std::env::var_os("VICTORY_GEOMETRY_INDEX")?;
            let bytes = std::fs::read(&path).ok()?;
            let index = parse_index(&bytes);
            if index.is_none() {
                tracing::warn!("invalid geometry fingerprint index");
            }
            index
        })
        .as_ref()?;
    identify_with(index, address, available)
}

fn identify_with(index: &Index, address: u64, available: u64) -> Option<String> {
    if available < 128 {
        return None;
    }
    let mut prefix = [0u8; 128];
    if !crate::host::read_into(address as *const u8, &mut prefix) {
        return None;
    }
    let hash = blake3::hash(&prefix);
    let mut result = format!(" geom-prefix={hash}");
    if let Some(options) = index.get(hash.as_bytes()) {
        let sizes: HashSet<usize> = options
            .iter()
            .map(|&(n, _)| n)
            .filter(|&n| (n as u64) <= available)
            .collect();
        let mut sizes: Vec<_> = sizes.into_iter().collect();
        sizes.sort_unstable();
        if let Some(&max) = sizes.last() {
            // Each size came from a specific decoded vertex stream in the offline index.
            let mut data = vec![0; max];
            if crate::host::read_into(address as *const u8, &mut data) && data[..128] == prefix {
                for size in sizes {
                    let full = blake3::hash(&data[..size]);
                    if options.contains(&(size, *full.as_bytes())) {
                        use std::fmt::Write;
                        let _ = write!(result, " geom-stream={size}:{full}");
                    }
                }
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncation_and_undersized_records() {
        let mut bytes = b"VIGEO001".to_vec();
        bytes.extend([0u8; 68]);
        assert!(parse_index(&bytes).is_none());
        bytes[40..44].copy_from_slice(&128u32.to_le_bytes());
        assert_eq!(parse_index(&bytes).unwrap()[&[0; 32]], vec![(128, [0; 32])]);
        assert!(parse_index(&bytes[..bytes.len() - 1]).is_none());
    }
    #[test]
    fn identifies_complete_buffers_at_the_same_address_after_reuse() {
        let mut a: Vec<u8> = (0..512).map(|i| (i % 197) as u8).collect();
        let mut b = a.clone();
        b[384] ^= 1;
        let mut index = b"VIGEO001".to_vec();
        for data in [&a[..], &b[..]] {
            index.extend(blake3::hash(&data[..128]).as_bytes());
            index.extend((data.len() as u32).to_le_bytes());
            index.extend(blake3::hash(data).as_bytes());
        }
        let index = parse_index(&index).unwrap();
        let addr = a.as_ptr() as u64;
        let tag = identify_with(&index, addr, 512).unwrap();
        assert!(tag.contains(&format!("geom-stream=512:{}", blake3::hash(&a))));
        a[384] ^= 1;
        let tag = identify_with(&index, addr, 512).unwrap();
        assert!(tag.contains(&format!("geom-stream=512:{}", blake3::hash(&b))));
        assert!(
            !identify_with(&index, addr, 511)
                .unwrap()
                .contains("geom-stream=")
        );
        assert!(identify_with(&index, addr, 127).is_none());
    }
}
