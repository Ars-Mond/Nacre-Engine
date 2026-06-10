# Phase 1 Data Model: Render a Single PBR Mesh

**Feature**: `001-render-pbr-mesh` | **Date**: 2026-06-10

This describes the host-facing input types and the GPU-side data they produce. Math types
are `glam`; GPU structs are `#[repr(C)]` + `bytemuck::{Pod, Zeroable}`. Field names are
indicative; exact signatures live in [contracts/public-api.md](./contracts/public-api.md).

## Host-facing input types

### EngineConfig

Declared once at `Engine::new`.

| Field | Type | Notes / Validation |
|-------|------|--------------------|
| `target_format` | `wgpu::TextureFormat` | The host target's color format; should be an sRGB format (FR-017). Pipeline color state matches it. |
| `sample_count` | `u32` | MSAA samples; 1 or 4 (research §4). Must equal the host color attachment's sample count. |

The engine derives and exposes `depth_format()` = `Depth32Float` and a `depth_view()` once
`prepare()` has run (research §3).

### Viewport

The draw sub-rectangle passed to `render()` (drives `set_viewport` + scissor).

| Field | Type | Notes / Validation |
|-------|------|--------------------|
| `x`, `y` | `f32` | Top-left origin within the target. |
| `width`, `height` | `f32` | Render area for `set_viewport`/scissor. Zero width/height ⇒ frame is a safe no-op (edge case). |
| `min_depth`, `max_depth` | `f32` | Default 0.0 / 1.0. |

The depth buffer is sized to the **color target**, not this viewport: `prepare(device,
queue, target_size)` takes the color-attachment `(width, height)` and (re)creates the depth
texture when that size changes (a wgpu requirement that depth and color attachments share
dimensions).

### Camera

| Field | Type | Notes |
|-------|------|-------|
| `view` | `glam::Mat4` | World→view (right-handed, Y-up). |
| `projection` | `glam::Mat4` | View→clip (wgpu depth 0..1). |
| `position` | `glam::Vec3` | Camera world position, needed for the specular view vector. May be derived from `view.inverse()` if not provided. |

### Primitive (built-in geometry selector)

`enum Primitive { Cube, Sphere }` — sphere uses a fixed default tessellation. Built-ins are
generated with positions, normals, UVs, and tangents.

### MeshData (custom geometry)

| Field | Type | Validation |
|-------|------|------------|
| `positions` | `Vec<[f32; 3]>` | Required, non-empty. |
| `normals` | `Vec<[f32; 3]>` | Required, same length as positions. |
| `uvs` | `Vec<[f32; 2]>` | Required, same length. |
| `tangents` | `Vec<[f32; 4]>` | Required (xyz + handedness w); same length. No auto-generation in this feature. |
| `indices` | `Vec<u32>` | Required, non-empty, length % 3 == 0, each < vertex count. |

Validation failures (length mismatch, empty/degenerate, out-of-range index) are reported as
errors at `create_mesh`, not at render time.

### MeshHandle

Opaque handle returned by `Engine::create_mesh` / `Engine::builtin_mesh`; references uploaded
GPU vertex/index buffers and the index count. Referenced by `Scene`.

### Material

| Field | Type | Notes |
|-------|------|-------|
| `base_color` | `glam::Vec4` | Linear RGBA factor. |
| `metallic` | `f32` | 0..1. |
| `roughness` | `f32` | 0..1 (clamped to a small minimum to avoid a zero-area highlight). |
| `normal_map` | `Option<&wgpu::TextureView>` | Host-created (FR-018). Absent ⇒ geometric normal. |
| `occlusion_map` | `Option<&wgpu::TextureView>` | Host-created. Absent ⇒ AO = 1.0. |

Normal mapping requires usable tangents on the mesh; a normal map with degenerate tangents
falls back to the geometric normal (edge case).

### Light

`enum Light { Directional { direction, color, intensity }, Point { position, color, intensity, range } }`

| Variant field | Type | Notes |
|---------------|------|-------|
| `direction` | `glam::Vec3` | Directional: direction the light travels (normalized). |
| `position` | `glam::Vec3` | Point: world position. |
| `color` | `glam::Vec3` | Linear RGB. |
| `intensity` | `f32` | Scalar multiplier. |
| `range` | `f32` | Point only; soft cutoff distance. |

At most `MAX_LIGHTS = 8` are used (research §12); extras are ignored with a warning.

### Scene

The per-frame input handed to `update()`.

| Field | Type | Notes |
|-------|------|-------|
| `camera` | `Camera` | — |
| `mesh` | `MeshHandle` | The single mesh to draw. |
| `transform` | `glam::Mat4` | Model→world; default identity (single object, no hierarchy). |
| `material` | `Material` | — |
| `lights` | `&[Light]` | 0..=8 effective. |

## GPU-side structures (bytemuck `Pod`)

Std140-friendly layouts (vec3 padded to 16 bytes).

### Vertex (vertex buffer)

| Field | Type | Offset |
|-------|------|--------|
| `position` | `[f32; 3]` | 0 |
| `normal` | `[f32; 3]` | 12 |
| `uv` | `[f32; 2]` | 24 |
| `tangent` | `[f32; 4]` | 32 |

Stride 48 bytes. Index buffer is `u32`.

### CameraUniform (group 0)

`view_proj: [[f32;4];4]`, `camera_pos: [f32;4]` (xyz + pad).

### ModelUniform (group 0)

`model: [[f32;4];4]`, `normal_matrix: [[f32;4];4]` (mat3 stored in a mat4 for alignment;
inverse-transpose of the model upper-left 3×3).

### LightStd + LightsUniform (group 0)

`LightStd { position_or_direction: [f32;4], color: [f32;3]+pad, kind: u32, intensity: f32,
range: f32, _pad }`. `LightsUniform { lights: [LightStd; 8], count: u32, _pad }`.

### MaterialUniform (group 1)

`base_color: [f32;4]`, `metallic: f32`, `roughness: f32`, `flags: u32`
(`bit0 = has_normal_map`, `bit1 = has_occlusion_map`), `_pad: u32`.

## Bind group layout

| Group | Binding | Resource |
|-------|---------|----------|
| 0 | 0 | `CameraUniform` (uniform) |
| 0 | 1 | `ModelUniform` (uniform) |
| 0 | 2 | `LightsUniform` (uniform) |
| 1 | 0 | `MaterialUniform` (uniform) |
| 1 | 1 | normal-map texture view (or 1×1 placeholder) |
| 1 | 2 | occlusion-map texture view (or 1×1 placeholder) |
| 1 | 3 | sampler (linear, repeat) |

Group 0 = per-frame/per-object data, Group 1 = material. Placeholders are engine-internal 1×1
textures used when a map is absent (research §8).

## Relationships & lifecycle state

- `Engine` owns: the render pipeline + bind group layouts (built once), uniform buffers, the
  depth texture (per color-target size), internal placeholder textures and sampler, and the set
  of uploaded meshes (`MeshHandle` → buffers).
- `Scene` references a `MeshHandle` and borrows host `TextureView`s for material maps.
- Per-frame flow: `update(scene)` records CPU state and marks dirty → `prepare(device, queue,
  target_size)` uploads uniforms, ensures the depth texture matches the color target, and
  updates the material bind group → `render(pass, viewport)` sets the pipeline, bind groups,
  viewport, and scissor, then issues the indexed draw. No pipeline or mesh creation happens in
  `render`.
