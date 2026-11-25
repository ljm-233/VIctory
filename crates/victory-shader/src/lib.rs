//! Xbox Series shaders to host shaders.
//!
//! An Xbox Series shader is a standard DXBC container ([`dxbc`]). Its `DXIL`
//! part ([`dxil`]) may carry Xbox additions around the LLVM bitcode: an
//! `XDAT` table before it indexing blobs after it, among them `xbop`
//! (compiler options) and the zlib-compressed GFX10 code. The bitcode may use
//! console-only intrinsics, so the GFX10 code is translated in general.

pub mod bvh;
pub mod dxbc;
pub mod dxil;
pub mod emit;
pub mod gfx10;
pub mod io;
pub mod ir;
pub mod native;
pub mod resource;
pub mod spill;
pub mod structure;

pub use dxbc::{Container, FourCC, Part};
pub use dxil::{DxilProgram, ShaderKind, XboxExtras};
pub use native::{
    HwStage, NativeProgram, ResourceType, RootParam, RootType, UsedResource, native_programs,
};

/// What identifies the translations this build produces: a hash of this
/// crate's own sources, its manifest and the features it is built with
/// (`build.rs`, which re-runs when any of them changes).
///
/// A rebuild of `victory-gpu`, the application or the HLE layer leaves it
/// alone, so `victory-gpu`'s shader cache can key its entries on it
/// (`VICTORY_SHADER_CACHE_KEY=translator`) and keep the translations of the
/// previous build across such rebuilds.
///
/// It does not cover the run-time switches that change a translation
/// (`VICTORY_SHADER_LOOP_LIMIT`, `VICTORY_SHADER_OPT`, `VICTORY_GROUP_SYNC`,
/// the wave switches, the host's view heap): the cache keys on those
/// separately, because two runs of the same build can differ in them.
pub const TRANSLATOR_FINGERPRINT: &str = env!("VICTORY_SHADER_FINGERPRINT");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShaderError {
    #[error("not a DXBC container")]
    NotDxbc,
    #[error("malformed shader: {0}")]
    Malformed(&'static str),
    #[error("GFX10 code at {offset:#x}: {reason}")]
    Isa { offset: usize, reason: &'static str },
    /// Valid code the translator cannot handle yet: an opcode name or a construct.
    #[error("GFX10 code at {offset:#x}: {what} is not supported")]
    Unsupported { offset: usize, what: &'static str },
}
