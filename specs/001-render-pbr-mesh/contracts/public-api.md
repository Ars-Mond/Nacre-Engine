# Contract: Public API

**Feature**: `001-render-pbr-mesh` | **Date**: 2026-06-10

This is the library's primary contract: the public Rust API and the `update → prepare →
render` lifecycle (constitution Principle V). Signatures are indicative of the intended
surface; SemVer governs changes. All `wgpu` types are from the pinned `wgpu = 27`.

## Types (re-exported from the crate root)

```rust
pub const VERSION: &str;            // already present
pub const MAX_LIGHTS: usize = 8;

pub struct Engine { /* opaque */ }

pub struct EngineConfig {
    pub target_format: wgpu::TextureFormat, // sRGB color format recommended (FR-017)
    pub sample_count: u32,                  // 1 or 4; must match host color attachment
}

pub struct Viewport {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub min_depth: f32, pub max_depth: f32, // default 0.0 / 1.0
}

pub struct Camera {
    pub view: glam::Mat4,
    pub projection: glam::Mat4,
    pub position: glam::Vec3,
}

pub enum Primitive { Cube, Sphere }

pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals:   Vec<[f32; 3]>,
    pub uvs:       Vec<[f32; 2]>,
    pub tangents:  Vec<[f32; 4]>,
    pub indices:   Vec<u32>,
}

#[derive(Clone, Copy)]
pub struct MeshHandle(/* opaque */);

pub struct Material<'a> {
    pub base_color: glam::Vec4,                      // linear
    pub metallic: f32,
    pub roughness: f32,
    pub normal_map: Option<&'a wgpu::TextureView>,   // host-created (FR-018)
    pub occlusion_map: Option<&'a wgpu::TextureView>,
}

pub enum Light {
    Directional { direction: glam::Vec3, color: glam::Vec3, intensity: f32 },
    Point       { position: glam::Vec3,  color: glam::Vec3, intensity: f32, range: f32 },
}

pub struct Scene<'a> {
    pub camera: Camera,
    pub mesh: MeshHandle,
    pub transform: glam::Mat4,        // default identity
    pub material: Material<'a>,
    pub lights: &'a [Light],          // 0..=MAX_LIGHTS effective
}

pub enum EngineError {
    EmptyGeometry,
    AttributeLengthMismatch { attribute: &'static str },
    IndexOutOfRange { index: u32, vertex_count: u32 },
    IndicesNotTriangles { len: usize },
}
```

## Lifecycle methods

```rust
impl Engine {
    /// Create the engine from host-owned GPU handles. Creates NO window, surface,
    /// event loop, or device (Principle III). Builds the pipeline and bind group layouts.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, config: &EngineConfig) -> Self;

    /// Upload custom geometry once; returns a handle to reference from `Scene`.
    pub fn create_mesh(&mut self, device: &wgpu::Device, data: &MeshData)
        -> Result<MeshHandle, EngineError>;

    /// Upload a built-in primitive (cube/sphere) with generated normals and tangents.
    pub fn builtin_mesh(&mut self, device: &wgpu::Device, primitive: Primitive) -> MeshHandle;

    // ---- update → prepare → render (the stable lifecycle) ----

    /// Phase 1 — record the scene to draw this frame (CPU-side; marks state dirty).
    pub fn update(&mut self, scene: &Scene);

    /// Phase 2 — upload uniforms, (re)create the depth buffer to match the color
    /// target, and refresh the material bind group. `target_size` is the host color
    /// attachment size in pixels; the engine's depth attachment MUST match it (a wgpu
    /// requirement), so depth is sized to the target, not the draw viewport.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target_size: (u32, u32));

    /// Phase 3 — encode the draw into a render pass the caller has begun, clipped to
    /// `viewport` via set_viewport + scissor. Does NOT clear or touch pixels outside the
    /// viewport. The pass MUST have been begun with `depth_view()` as its depth attachment.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>, viewport: Viewport);

    // ---- depth exposure (FR-016) ----

    /// The depth format the engine uses; pass this when building the pass's depth attachment.
    pub fn depth_format(&self) -> wgpu::TextureFormat;     // Depth32Float

    /// The engine-owned depth view to attach when beginning the pass. Valid after `prepare()`.
    pub fn depth_view(&self) -> &wgpu::TextureView;
}
```

## Contract guarantees

1. **No I/O ownership**: no method creates or destroys a window, surface, event loop, or
   device/queue; the engine only references the host's `Device`/`Queue` (Principle III).
2. **Viewport containment**: `render` modifies only pixels inside `viewport` (SC-002); it
   never issues a clear.
3. **Depth attachment protocol**: the caller begins the pass with `depth_view()` (format
   `depth_format()`); `render` assumes that attachment is present. Color attachment format and
   sample count MUST match `EngineConfig`.
4. **Lifecycle order**: `update` → `prepare` → `render` per frame. `prepare` must run after any
   `update` and after any change to the color-target size; `render` must run inside an active pass.
5. **Stability**: this surface is the SemVer contract; breaking changes require a major bump.

## Lifecycle ordering (informative)

```text
new(device, queue, config)
create_mesh(...) / builtin_mesh(...)        // once per mesh
loop each frame:
    update(&scene)
    prepare(device, queue, target_size)   // (width, height) of the color attachment
    // caller begins a render pass with color = host target, depth = engine.depth_view()
    render(&mut pass, viewport)            // draw sub-rectangle (set_viewport + scissor)
    // caller ends the pass, submits, presents
```
