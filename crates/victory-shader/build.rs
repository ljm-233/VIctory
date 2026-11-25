//! Fingerprints the translator: the hash of this crate's own sources, its
//! manifest and the features it is built with, exposed to the crate as
//! `VICTORY_SHADER_FINGERPRINT` (see `TRANSLATOR_FINGERPRINT` in `lib.rs`).
//!
//! `victory-gpu`'s shader cache keys its entries on it
//! (`VICTORY_SHADER_CACHE_KEY=translator`), so a rebuild that only touches
//! the GPU layer, the application or the HLE layer keeps the translations
//! of the previous build: what this fingerprint covers is what can change
//! a translated program.
//!
//! Hashed: every file under `src/` but the test-only modules (`*_tests.rs`
//! and `tests.rs`: editing a test does not change a translation), this
//! script, `Cargo.toml`, and the enabled features. Not hashed: the target
//! and the profile (they change code generation, not what the translator
//! says), and the run-time switches
//! (`VICTORY_SHADER_LOOP_LIMIT`, `VICTORY_SHADER_OPT`, `VICTORY_GROUP_SYNC`,
//! the wave switches) which the cache keys on separately, because they can
//! differ between two runs of the same build.
//!
//! The hash changes when a file's path, length or bytes change; a comment
//! edit invalidates too, which costs one re-translation run and never a
//! stale entry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // A file added anywhere under `src/` changes the directory's timestamp,
    // so watching it covers new files too.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");

    let mut files = BTreeMap::new();
    collect(&root.join("src"), &root, &mut files);
    for name in ["Cargo.toml", "build.rs"] {
        files.insert(name.to_owned(), std::fs::read(root.join(name)).unwrap_or_default());
    }

    let mut hash = Hash::new();
    for (path, bytes) in &files {
        hash.update(path.as_bytes());
        hash.update(&[0]);
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    // `cfg!(feature = "...")` in the crate changes what a translation says
    // (the `diagnostics` build lets the loop limit move), and the enabled
    // features are in no file the hash above reads.
    let mut features: Vec<String> = std::env::vars()
        .filter(|(name, _)| name.starts_with("CARGO_FEATURE_"))
        .map(|(name, _)| name)
        .collect();
    features.sort();
    hash.update(b"features:");
    for name in &features {
        hash.update(name.as_bytes());
        hash.update(&[0]);
    }

    println!("cargo:rustc-env=VICTORY_SHADER_FINGERPRINT={}", hash.hex());
}

/// Adds every file under `dir` to `files`, keyed by its path relative to
/// `root` with `/` separators (the same on every host).
fn collect(dir: &Path, root: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, root, files);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let key = rel.to_string_lossy().replace('\\', "/");
            // A test-only module: it is compiled out of the translator, so
            // editing one must not throw the translations away (upstream
            // does the same by marking them `#![cfg(test)]`).
            let test_only = key.ends_with("_tests.rs") || key.ends_with("tests.rs");
            if !test_only {
                files.insert(key, std::fs::read(&path).unwrap_or_default());
            }
        }
    }
}

/// Two independent 64-bit FNV-1a lanes: enough to notice any edit (this
/// guards a cache, it is not a security boundary).
struct Hash(u64, u64);

impl Hash {
    fn new() -> Self {
        Hash(0xcbf2_9ce4_8422_2325, 0x9e37_79b9_7f4a_7c15)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            self.1 = (self.1 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(7);
        }
    }

    fn hex(&self) -> String {
        format!("{:016x}{:016x}", self.0, self.1)
    }
}
