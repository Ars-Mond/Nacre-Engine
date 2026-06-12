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
//! # Example
//!
//! The host owns the `Device`, `Queue`, and the render pass — which it begins with
//! the engine's depth view ([`Engine::depth_view`]) as the depth attachment.
//!
//! ```no_run
//! use nacre_engine::glam::{Mat4, Vec3};
//! use nacre_engine::wgpu;
//! use nacre_engine::{Camera, Engine, EngineConfig, Light, Material, Primitive, Scene, Viewport};
//!
//! fn draw(device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>) {
//!     let mut engine = Engine::new(
//!         device,
//!         queue,
//!         &EngineConfig { target_format: wgpu::TextureFormat::Rgba8UnormSrgb, sample_count: 1 },
//!     );
//!     let mesh = engine.builtin_mesh(device, Primitive::Cube);
//!
//!     let eye = Vec3::new(0.0, 1.0, 3.0);
//!     let lights = [Light::Directional {
//!         direction: Vec3::new(0.0, -0.5, -1.0),
//!         color: Vec3::ONE,
//!         intensity: 3.0,
//!     }];
//!     let scene = Scene {
//!         camera: Camera {
//!             view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
//!             projection: Mat4::perspective_rh(60_f32.to_radians(), 1.0, 0.1, 100.0),
//!             position: eye,
//!         },
//!         mesh,
//!         transform: Mat4::IDENTITY,
//!         material: Material::default(),
//!         lights: &lights,
//!     };
//!
//!     // update -> prepare -> render. `prepare` takes the color-target size.
//!     engine.update(&scene);
//!     engine.prepare(device, queue, (800, 600));
//!     engine.render(pass, Viewport::new(0.0, 0.0, 800.0, 600.0));
//! }
//! ```

// Constitution Principle VI/VIII enforcement for library code, plus documented
// public API (Principle V).
#![deny(
    missing_docs,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::dbg_macro,
    clippy::undocumented_unsafe_blocks
)]

mod depth;
mod engine;
mod error;
mod material;
mod mesh;
mod pipeline;
mod scene;
mod tonemap;
mod uniforms;

pub use engine::{Engine, EngineConfig};
pub use error::EngineError;
pub use material::Material;
pub use mesh::{MeshData, MeshHandle, Primitive};
pub use scene::{Camera, Light, Scene, Viewport};
pub use tonemap::{ToneMapOperator, ToneMapping};
pub use uniforms::MAX_LIGHTS;

// Re-export the exact dependency versions so hosts share one crate instance.
pub use glam;
pub use wgpu;

/// The crate version, taken from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
