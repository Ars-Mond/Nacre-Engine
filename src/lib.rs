//! NacreEngine — a minimal real-time PBR engine/renderer built on top of `wgpu`.
//!
//! NacreEngine ships as a **library** crate. It never owns input/output: it does
//! not create a window, surface, event loop, or wgpu `Device`/`Queue`. The host
//! application supplies those — together with a target texture (its format and
//! size) — and the engine renders into that texture through a stable
//! `update -> prepare -> render` lifecycle.
//!
//! Physically based lighting uses the metallic-roughness workflow with a
//! Cook-Torrance BRDF. Shaders are authored in WGSL and run on every `wgpu`
//! backend (DX12, Vulkan, Metal, GLES).
//!
//! This is an early scaffold; the public rendering API is not defined yet.

/// The crate version, taken from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
