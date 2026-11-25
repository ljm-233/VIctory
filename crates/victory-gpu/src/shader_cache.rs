//! Translated shaders kept on disk (decision 0003): under the configured
//! cache folder, keyed by the container's content and by what translated
//! it, so a rebuilt translator never serves an old translation. Entries go
//! through `victory-pipeline`'s content-addressed cache, which writes them
//! atomically; an entry that does not read back is translated again.
//!
//! What "what translated it" means is `KeyMode`, from
//! `VICTORY_SHADER_CACHE_KEY`:
//!
//! - unset, `exe` (default): the BLAKE3 of our executable. Any rebuild of
//!   any crate makes every entry unreachable, so a run after a one-line
//!   change in the GPU or HLE layer translates all of the title's shaders
//!   again.
//! - `translator`: [`victory_shader::TRANSLATOR_FINGERPRINT`], the hash of
//!   `victory-shader`'s own sources and features (`crates/victory-shader/
//!   build.rs`). A rebuild that leaves the translator alone keeps the
//!   entries; a change inside it, or a reverted one, does not.
//! - `loose`: nothing but the switches below, not even the translator.
//!   For iterating on the translator itself, where the alternative is a full
//!   re-translation per build. It can serve a translation made by different
//!   code: delete the folder (`VICTORY_SHADER_CACHE_CLEAR=1`, or the
//!   `shaders` folder under the cache folder) before trusting a result.
//!
//! In every mode the run-time switches that change what a translation says
//! are part of the key (`settings`). `VICTORY_SHADER_CACHE_CLEAR=1`
//! deletes every entry before the cache opens.
//!
//! The entry is our own encoding of [`HostShader`]: a field added to it
//! must be added to `encode` and `decode` too (entries of older builds
//! are ignored anyway).

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use victory_pipeline::{Cache, Converter};
use victory_shader::emit::{HostShader, StaticSampler, UserData};
use victory_shader::resource::RootIndexedView;
use victory_shader::{HwStage, RootType};

static CACHE: OnceLock<Cache> = OnceLock::new();
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);

/// Keeps translated shaders in `dir` from now on.
pub fn use_shader_cache(dir: PathBuf) {
    if clear_requested() {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => tracing::warn!(
                dir = %dir.display(),
                "VICTORY_SHADER_CACHE_CLEAR: every translated shader deleted"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(
                error = %e,
                dir = %dir.display(),
                "VICTORY_SHADER_CACHE_CLEAR: some translated shaders remain"
            ),
        }
    }
    // What the entries will be keyed on, said once here so a run's log shows
    // which policy it ran under (the strict default included).
    tracing::info!(
        dir = %dir.display(),
        key = key_mode().name(),
        "shader cache"
    );
    match Cache::open(dir) {
        Ok(cache) => {
            let _ = CACHE.set(cache);
        }
        Err(e) => tracing::warn!(error = %e, "no shader cache"),
    }
}

/// Shaders found in the cache and translated since the start.
pub(crate) fn counts() -> (u64, u64) {
    (HITS.load(Ordering::Relaxed), MISSES.load(Ordering::Relaxed))
}

/// What the cache keys its entries on besides the container's bytes;
/// `VICTORY_SHADER_CACHE_KEY` picks one (see the module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyMode {
    /// The BLAKE3 of our executable, as before this switch existed.
    Exe,
    /// `victory-shader`'s own fingerprint: what can change a translation.
    Translator,
    /// Neither: a fixed key, for iterating on the translator itself.
    Loose,
}

impl KeyMode {
    fn name(self) -> &'static str {
        match self {
            KeyMode::Exe => "exe",
            KeyMode::Translator => "translator",
            KeyMode::Loose => "loose",
        }
    }
}

/// `VICTORY_SHADER_CACHE_KEY`'s value; the strict default when it is unset
/// or unreadable. Pure, for testing.
fn parse_key_mode(value: Option<&str>) -> KeyMode {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("translator") | Some("source") => KeyMode::Translator,
        Some("loose") | Some("fixed") => KeyMode::Loose,
        _ => KeyMode::Exe,
    }
}

fn key_mode() -> KeyMode {
    static MODE: OnceLock<KeyMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let raw = std::env::var("VICTORY_SHADER_CACHE_KEY").ok();
        let mode = parse_key_mode(raw.as_deref());
        match (mode, raw.as_deref().map(str::trim)) {
            (KeyMode::Loose, _) => tracing::warn!(
                "VICTORY_SHADER_CACHE_KEY=loose: translations outlive the translator that made \
                 them; delete the shader cache folder (VICTORY_SHADER_CACHE_CLEAR=1) after \
                 changing victory-shader"
            ),
            (KeyMode::Translator, _) => tracing::info!(
                fingerprint = victory_shader::TRANSLATOR_FINGERPRINT,
                "shader cache keyed on the translator, not on this executable"
            ),
            (KeyMode::Exe, Some(other)) if !matches!(other, "exe" | "strict") => {
                tracing::warn!(
                    value = other,
                    "unknown VICTORY_SHADER_CACHE_KEY; keying on the executable"
                );
            }
            _ => {}
        }
        mode
    })
}

/// `VICTORY_SHADER_CACHE_CLEAR`: whether this run starts from an empty
/// cache.
fn clear_requested() -> bool {
    std::env::var("VICTORY_SHADER_CACHE_CLEAR")
        .ok()
        .is_some_and(|v| !matches!(v.trim(), "" | "0"))
}

/// What identifies the translation: the key mode's material and the
/// translation settings this run uses, hashed.
fn build() -> &'static [u8; 32] {
    static BUILD: OnceLock<[u8; 32]> = OnceLock::new();
    BUILD.get_or_init(|| {
        let mode = key_mode();
        // An entry is never served to a run whose policy differs, so a
        // strict run cannot pick up what a loose one left behind.
        let mut h = blake3::Hasher::new();
        h.update(b"victory-shader-key:");
        h.update(mode.name().as_bytes());
        h.update(&[0]);
        match mode {
            // Streamed, not read into memory: the executable of a release
            // build is tens of megabytes, and hashing it was charged to the
            // first shader of every run.
            KeyMode::Exe => {
                let hashed = std::env::current_exe()
                    .and_then(std::fs::File::open)
                    .and_then(|file| h.update_reader(file));
                if let Err(e) = hashed {
                    // Without it no entry can be trusted: a fresh key per run.
                    tracing::warn!(error = %e, "cannot read our executable; shader cache unused");
                    return *blake3::hash(&fresh().to_le_bytes()).as_bytes();
                }
            }
            KeyMode::Translator => {
                h.update(victory_shader::TRANSLATOR_FINGERPRINT.as_bytes());
            }
            KeyMode::Loose => {}
        }
        // The entry is this file's `encode`/`decode`: a change to them
        // changes what an entry means, so the file's own text is part of the
        // key (as upstream does).
        h.update(blake3::hash(ENCODING).as_bytes());
        let mut settings = Vec::with_capacity(64);
        add_settings(&mut settings);
        h.update(&settings);
        *h.finalize().as_bytes()
    })
}

/// This file, whose `encode` and `decode` define the entries.
const ENCODING: &[u8] = include_bytes!("shader_cache.rs");

/// A key that is not shared with any earlier run.
fn fresh() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// Appends the switches this run translates with. Each of them changes what
/// a translation says, so all of them are part of the key in every mode.
fn add_settings(bytes: &mut Vec<u8>) {
    let limit = victory_shader::emit::loop_limit();
    if limit != victory_shader::emit::DEFAULT_LOOP_LIMIT {
        tracing::warn!(
            limit,
            "VICTORY_SHADER_LOOP_LIMIT: translated loops stop sooner"
        );
    }
    bytes.extend_from_slice(&limit.to_le_bytes());
    // The view heap's null slot differs by host GPU: a translation cached
    // without the clamp must not be served. (Probing creates the host device
    // if it is not up yet, as it always did from here.)
    crate::descriptor::host::probe_view_limit();
    bytes.extend_from_slice(&victory_shader::emit::view_heap_limit().to_le_bytes());
    // The host's widest wave picks the form a wave64 compute program is
    // translated into (32-lane hosts get the half-wave form), so an entry
    // made on one card must not be served on another (issue 1844 upstream).
    // Read after the probe: creating the device is what sets it.
    bytes.extend_from_slice(&victory_shader::emit::host_max_wave().to_le_bytes());
    // Which group barriers wait is part of the translation too.
    bytes.push(victory_shader::emit::group_sync());
    // So are the switches that pick the form of a wave64 program: without
    // them here a run with `VICTORY_WAVE_WIDE=0` was served the translations
    // an earlier run of the same executable had cached with the switch unset.
    for name in [
        "VICTORY_WAVE_WIDE",
        "VICTORY_WAVE_DEEP_SKIPS",
        "VICTORY_WAVE_OWN_BITS",
        "VICTORY_F16_NEAREST",
    ] {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'=');
        if let Some(value) = std::env::var_os(name) {
            bytes.extend_from_slice(value.as_encoded_bytes());
        }
        bytes.push(0);
    }
    // And which clean-up passes ran over the translated code.
    bytes.extend_from_slice(&victory_shader::emit::shader_opt().to_le_bytes());
}

/// The translation of a console shader container, from the cache when
/// there is one.
pub(crate) fn host_shader(container: &[u8]) -> Result<HostShader, String> {
    if let Some(shader) = stand_in(container) {
        return Ok(shader);
    }
    let translate = |c: &[u8]| victory_shader::emit::translate(c).map_err(|e| e.to_string());
    let r = match CACHE.get() {
        Some(cache) => cached(cache, build(), container, translate),
        None => translate(container),
    };
    match r {
        Err(e) => exposure_stub(container).ok_or(e),
        ok => ok,
    }
}

/// Optional stand-ins from `victory_shader::emit::stand_in`:
/// - `VICTORY_AMBIENT_STUB=1`: replaces diffuse GI temporal resolve passes
///   with a constant ambient light floor (`VICTORY_AMBIENT_STUB_VALUE`,
///   default 0.1).
/// - `VICTORY_SHADOWMASK_STUB=1`: replaces ray-traced distant shadow passes
///   with 1.0 (lit) (`VICTORY_SHADOWMASK_STUB_VALUE`, default 1.0).
///
/// The passes themselves are named by hash (`*_HASHES` overrides the list the
/// translator carries). A stand-in is made fresh, never cached: it is a value
/// the run asked for, not a translation of the title's program.
#[cfg(windows)]
fn stand_in(container: &[u8]) -> Option<HostShader> {
    use victory_shader::emit::stand_in as s;
    let hash = blake3::hash(container).to_hex()[..16].to_owned();
    let kinds = [
        (
            "VICTORY_AMBIENT_STUB",
            s::AMBIENT_HASHES,
            s::AMBIENT_VALUE,
            s::ambient_slots(&hash),
        ),
        (
            "VICTORY_SHADOWMASK_STUB",
            s::SHADOWMASK_HASHES,
            s::SHADOWMASK_VALUE,
            s::SHADOWMASK_SLOTS,
        ),
    ];
    for (name, hashes, value, defaults) in kinds {
        if std::env::var(name).ok().as_deref().map(str::trim) != Some("1") {
            continue;
        }
        let listed = match std::env::var(format!("{name}_HASHES")) {
            Ok(v) => v
                .split(|c: char| c == ',' || c.is_whitespace())
                .any(|h| h.eq_ignore_ascii_case(&hash)),
            Err(_) => hashes.contains(&hash.as_str()),
        };
        if !listed {
            continue;
        }
        let value = std::env::var(format!("{name}_VALUE"))
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(value);
        let slots = std::env::var(format!("{name}_SLOTS")).unwrap_or_default();
        let plan = s::Plan::new(value)
            .with_overrides(defaults)
            .with_overrides(&slots);
        match s::storing_plan(container, plan) {
            Ok((shader, plan)) => {
                tracing::warn!(
                    %hash,
                    stand_in = name,
                    value,
                    outputs = ?plan.seen,
                    "pass replaced by a stand-in writing each output in its own encoding"
                );
                return Some(shader);
            }
            Err(e) => {
                tracing::warn!(%hash, stand_in = name, error = %e, "stand-in does not translate");
            }
        }
    }
    None
}

/// `VICTORY_EXPOSURE_STUB=1`: replaces the auto-exposure shader, when it does
/// not translate, with a constant mid-grey luminance stand-in (`exposure`).
#[cfg(windows)]
fn exposure_stub(container: &[u8]) -> Option<HostShader> {
    use victory_shader::emit::exposure;
    if std::env::var("VICTORY_EXPOSURE_STUB").ok()?.trim() != "1" {
        return None;
    }
    let hash = blake3::hash(container).to_hex()[..16].to_owned();
    let listed = match std::env::var("VICTORY_EXPOSURE_STUB_HASHES") {
        Ok(v) => v
            .split(|c: char| c == ',' || c.is_whitespace())
            .any(|h| h.eq_ignore_ascii_case(&hash)),
        Err(_) => exposure::HASHES.contains(&hash.as_str()),
    };
    if !listed {
        return None;
    }
    let value = std::env::var("VICTORY_EXPOSURE_STUB_VALUE")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(exposure::VALUE);
    match exposure::stub(container, value) {
        Ok(s) => {
            tracing::warn!(%hash, value, "exposure shader replaced by a constant stand-in");
            Some(s)
        }
        Err(e) => {
            tracing::warn!(%hash, error = %e, "exposure stand-in does not translate either");
            None
        }
    }
}

/// What [`warm`] found for one container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warmed {
    /// The entry was already there (a translation, or a refusal recorded
    /// earlier).
    Cached,
    /// Translated now and written to the cache.
    Translated,
}

/// Translates `container` into the cache and nothing else: no host device,
/// no pipelines. This is what the offline warm-up (`victory precompile`)
/// asks for, so that a run does not translate while it plays.
///
/// The key is the run's: the same executable (or translator, under
/// `VICTORY_SHADER_CACHE_KEY=translator`), the same GPU limits and the same
/// switches (`VICTORY_HOST_WAVE`, `VICTORY_VIEW_LIMIT`, `VICTORY_SHADER_*`).
/// A refusal is recorded too, as a run records it, and comes back as `Err`.
pub fn warm(container: &[u8]) -> Result<Warmed, String> {
    let translate = |c: &[u8]| victory_shader::emit::translate(c).map_err(|e| e.to_string());
    let Some(cache) = CACHE.get() else {
        return translate(container).map(|_| Warmed::Translated);
    };
    let input = [&build()[..], container].concat();
    let converter = Translate {
        translate,
        made: Mutex::new(None),
    };
    cache
        .get_or_convert(&converter, &input)
        .map_err(|e| format!("{e:#}"))?;
    match converter.take() {
        Some(Ok(_)) => Ok(Warmed::Translated),
        Some(Err(reason)) => Err(reason),
        None => Ok(Warmed::Cached),
    }
}

/// The cache entry's converter: the build and the container in, the
/// encoded translation (or its refusal) out. `made` keeps what a miss just
/// translated, so a miss does not read its own entry back (a just-written
/// file is slow to read where something watches new files), and a refusal
/// needs no second attempt.
struct Translate<F> {
    translate: F,
    made: Mutex<Option<Result<HostShader, String>>>,
}

impl<F: Fn(&[u8]) -> Result<HostShader, String> + Send + Sync> Converter for Translate<F> {
    fn id(&self) -> &'static str {
        "shader.dxil"
    }
    fn version(&self) -> u32 {
        // The build fingerprint in the input does the versioning.
        1
    }
    fn convert(&self, input: &[u8]) -> anyhow::Result<Vec<u8>> {
        // A refusal is kept too: the same key gives the same refusal, so it
        // need not be tried again on every run (a big compute program that
        // does not translate costs a second of start-up per run).
        let translation = (self.translate)(&input[32..]);
        let encoded = encode(&translation);
        if let Ok(mut made) = self.made.lock() {
            *made = Some(translation);
        }
        Ok(encoded)
    }
}

impl<F> Translate<F> {
    /// What this converter translated, once.
    fn take(&self) -> Option<Result<HostShader, String>> {
        self.made.lock().ok().and_then(|mut made| made.take())
    }
}

fn cached(
    cache: &Cache,
    build: &[u8; 32],
    container: &[u8],
    translate: impl Fn(&[u8]) -> Result<HostShader, String> + Send + Sync,
) -> Result<HostShader, String> {
    let input = [&build[..], container].concat();
    let converter = Translate {
        translate,
        made: Mutex::new(None),
    };
    for _ in 0..2 {
        let path = match cache.get_or_convert(&converter, &input) {
            Ok(path) => path,
            Err(e) => {
                // The translation in hand is good even when its entry could
                // not be written; return it rather than make it again.
                if let Some(made) = converter.take() {
                    MISSES.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(error = format!("{e:#}"), "shader cache entry not written");
                    return made;
                }
                return Err(format!("{e:#}"));
            }
        };
        // A miss hands back what it just translated: only a hit reads the file.
        if let Some(made) = converter.take() {
            MISSES.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(path = %path.display(), "shader translated and cached");
            return made;
        }
        match std::fs::read(&path).ok().and_then(|b| decode(&b)) {
            Some(made) => {
                HITS.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(path = %path.display(), "shader from the cache");
                return made;
            }
            None => {
                tracing::debug!(path = %path.display(), "unreadable shader cache entry; translating again");
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    MISSES.fetch_add(1, Ordering::Relaxed);
    (converter.translate)(container)
}

const MAGIC: &[u8; 4] = b"VSH1";
/// A container that does not translate: the reason in words follows.
const REFUSED: &[u8; 4] = b"VSHE";
const STAGES: [HwStage; 8] = [
    HwStage::Ls,
    HwStage::Hs,
    HwStage::Es,
    HwStage::Gs,
    HwStage::Vs,
    HwStage::Ps,
    HwStage::Ms,
    HwStage::Cs,
];

fn root_code(r: RootType) -> u32 {
    match r {
        RootType::Constants => 0,
        RootType::Descriptor => 1,
        RootType::Table => 2,
        RootType::ImmediateSampler => 3,
        RootType::ResourceHeap => 4,
        RootType::SamplerHeap => 5,
        RootType::Other(x) => 6 + x,
    }
}

fn root_type(code: u32) -> RootType {
    match code {
        0 => RootType::Constants,
        1 => RootType::Descriptor,
        2 => RootType::Table,
        3 => RootType::ImmediateSampler,
        4 => RootType::ResourceHeap,
        5 => RootType::SamplerHeap,
        x => RootType::Other(x - 6),
    }
}

/// Little-endian words and length-prefixed lists; a refusal is its
/// reason's text after `REFUSED`.
fn encode(translation: &Result<HostShader, String>) -> Vec<u8> {
    let s = match translation {
        Ok(s) => s,
        Err(reason) => return [REFUSED.as_slice(), reason.as_bytes()].concat(),
    };
    let mut out = MAGIC.to_vec();
    let word = |out: &mut Vec<u8>, v: u32| out.extend(v.to_le_bytes());
    let stage = STAGES.iter().position(|&x| x == s.stage).unwrap_or(0) as u32;
    word(&mut out, stage);
    word(&mut out, s.dxil.len() as u32);
    out.extend(&s.dxil);
    word(&mut out, s.user_data.len() as u32);
    for u in &s.user_data {
        for v in [u.index, u32::from(u.slot), root_code(u.root), u.dword] {
            word(&mut out, v);
        }
    }
    word(&mut out, s.samplers.len() as u32);
    for sampler in &s.samplers {
        for &v in sampler {
            word(&mut out, v);
        }
    }
    word(&mut out, s.static_samplers.len() as u32);
    for t in &s.static_samplers {
        let [u, v, w] = t.address;
        let words = [
            t.filter,
            u,
            v,
            w,
            t.mip_lod_bias.to_bits(),
            t.max_anisotropy,
            t.comparison,
            t.border,
            t.min_lod.to_bits(),
            t.max_lod.to_bits(),
        ];
        for v in words {
            word(&mut out, v);
        }
    }
    for slots in [&s.root_cbvs, &s.root_srvs, &s.root_uavs] {
        word(&mut out, slots.len() as u32);
        out.extend(slots);
    }
    word(&mut out, s.root_indexed_views.len() as u32);
    for view in &s.root_indexed_views {
        for value in [
            u32::from(view.table_slot),
            u32::from(view.constant_slot),
            view.constant_dword,
            view.byte_offset,
            u32::from(view.shift),
        ] {
            word(&mut out, value);
        }
    }
    out
}

/// `None`: not an entry this build can read. `Some(Err(reason))`: a
/// refusal the build that wrote it reached.
fn decode(bytes: &[u8]) -> Option<Result<HostShader, String>> {
    if let Some(reason) = bytes.strip_prefix(REFUSED) {
        // An entry with no reason is damaged, not a refusal: a refusal the
        // build that wrote it reached always says why.
        return (!reason.is_empty())
            .then(|| Err(String::from_utf8_lossy(reason).into_owned()));
    }
    struct Reader<'a>(&'a [u8]);
    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let (head, rest) = self.0.split_at_checked(n)?;
            self.0 = rest;
            Some(head)
        }
        fn word(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn list(&mut self) -> Option<Vec<u8>> {
            let n = self.word()? as usize;
            Some(self.take(n)?.to_vec())
        }
    }
    let mut r = Reader(bytes);
    if r.take(4)? != MAGIC {
        return None;
    }
    let stage = *STAGES.get(r.word()? as usize)?;
    let dxil = r.list()?;
    let n = r.word()? as usize;
    let mut user_data = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        user_data.push(UserData {
            index: r.word()?,
            slot: u8::try_from(r.word()?).ok()?,
            root: root_type(r.word()?),
            dword: r.word()?,
        });
    }
    let n = r.word()? as usize;
    let mut samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        samplers.push([r.word()?, r.word()?, r.word()?, r.word()?]);
    }
    let n = r.word()? as usize;
    let mut static_samplers = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        static_samplers.push(StaticSampler {
            filter: r.word()?,
            address: [r.word()?, r.word()?, r.word()?],
            mip_lod_bias: f32::from_bits(r.word()?),
            max_anisotropy: r.word()?,
            comparison: r.word()?,
            border: r.word()?,
            min_lod: f32::from_bits(r.word()?),
            max_lod: f32::from_bits(r.word()?),
        });
    }
    let root_cbvs = r.list()?;
    let root_srvs = r.list()?;
    let root_uavs = r.list()?;
    let n = r.word()? as usize;
    let mut root_indexed_views = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        root_indexed_views.push(RootIndexedView {
            table_slot: u8::try_from(r.word()?).ok()?,
            constant_slot: u8::try_from(r.word()?).ok()?,
            constant_dword: r.word()?,
            byte_offset: r.word()?,
            shift: u8::try_from(r.word()?).ok().filter(|&s| s < 32)?,
        });
    }
    if !r.0.is_empty() {
        return None;
    }
    Some(Ok(HostShader {
        stage,
        dxil,
        user_data,
        samplers,
        static_samplers,
        root_cbvs,
        root_srvs,
        root_uavs,
        root_indexed_views,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shader() -> HostShader {
        HostShader {
            stage: HwStage::Ps,
            dxil: b"DXBC and the rest".to_vec(),
            user_data: vec![UserData {
                index: 8,
                slot: 1,
                root: RootType::Table,
                dword: 0,
            }],
            samplers: vec![[0x400, 0xc0_0000, 0x18f0_0000, 0]],
            static_samplers: vec![StaticSampler::from_words([
                0x400,
                0xc0_0000,
                0x18f0_0000,
                0,
            ])],
            root_cbvs: vec![2],
            root_srvs: vec![0, 3],
            root_uavs: vec![1],
            root_indexed_views: vec![RootIndexedView {
                table_slot: 1,
                constant_slot: 2,
                constant_dword: 3,
                byte_offset: 32,
                shift: 5,
            }],
        }
    }

    #[test]
    fn translations_come_back_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let build = [7u8; 32];
        let container = b"a console container".as_slice();
        let runs = AtomicU64::new(0);
        let translate = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Ok(shader())
        };
        assert_eq!(cached(&cache, &build, container, translate), Ok(shader()));
        assert_eq!(cached(&cache, &build, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 1, "the second came from disk");

        // Another build translates again.
        assert_eq!(cached(&cache, &[8; 32], container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 2);

        // A damaged entry is a miss, translated and stored again.
        for e in walk(dir.path()) {
            std::fs::write(e, b"VSH1 broken").unwrap();
        }
        assert_eq!(cached(&cache, &build, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);
        assert_eq!(cached(&cache, &build, container, translate), Ok(shader()));
        assert_eq!(runs.load(Ordering::Relaxed), 3);

    }

    #[test]
    fn a_refusal_comes_back_from_the_cache_and_is_translated_once() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(dir.path()).unwrap();
        let runs = AtomicU64::new(0);
        let fail = |_: &[u8]| {
            runs.fetch_add(1, Ordering::Relaxed);
            Err::<HostShader, _>("a lane mask exchanged where not every lane runs".to_owned())
        };
        for _ in 0..2 {
            assert_eq!(
                cached(&cache, &[7; 32], b"a container", fail),
                Err("a lane mask exchanged where not every lane runs".to_owned())
            );
        }
        assert_eq!(runs.load(Ordering::Relaxed), 1, "the second came from disk");

        // A refusal of another build is that build's: tried again.
        assert!(cached(&cache, &[8; 32], b"a container", fail).is_err());
        assert_eq!(runs.load(Ordering::Relaxed), 2);

        // A refusal entry with no reason is damaged: a miss, tried again.
        for e in walk(dir.path()) {
            std::fs::write(e, REFUSED).unwrap();
        }
        assert_eq!(
            cached(&cache, &[7; 32], b"a container", fail),
            Err("a lane mask exchanged where not every lane runs".to_owned())
        );
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                files.extend(walk(&e.path()));
            } else {
                files.push(e.path());
            }
        }
        files
    }

    #[test]
    fn the_key_switch_parses_or_keeps_the_strict_default() {
        assert_eq!(parse_key_mode(None), KeyMode::Exe);
        assert_eq!(parse_key_mode(Some("")), KeyMode::Exe);
        assert_eq!(parse_key_mode(Some("exe")), KeyMode::Exe);
        assert_eq!(parse_key_mode(Some(" strict ")), KeyMode::Exe);
        assert_eq!(parse_key_mode(Some("translator")), KeyMode::Translator);
        assert_eq!(parse_key_mode(Some("source")), KeyMode::Translator);
        assert_eq!(parse_key_mode(Some("LOOSE")), KeyMode::Exe, "values are exact");
        assert_eq!(parse_key_mode(Some("loose")), KeyMode::Loose);
        assert_eq!(parse_key_mode(Some("yes")), KeyMode::Exe);
        // Every mode has its own name, and thus its own key material.
        assert_eq!(
            [KeyMode::Exe, KeyMode::Translator, KeyMode::Loose].map(KeyMode::name),
            ["exe", "translator", "loose"]
        );
    }

    #[test]
    fn entries_round_trip_and_reject_damage() {
        let bytes = encode(&Ok(shader()));
        assert_eq!(decode(&bytes), Some(Ok(shader())));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode(&[bytes.as_slice(), &[0]].concat()), None);
        assert_eq!(decode(b"VSH0"), None);

        let refused = encode(&Err("a lane mask exchanged where not every lane runs".to_owned()));
        assert_eq!(
            decode(&refused),
            Some(Err("a lane mask exchanged where not every lane runs".to_owned()))
        );
        // A refusal entry with no reason is damaged, not a refusal.
        assert_eq!(decode(REFUSED), None);
    }
}
