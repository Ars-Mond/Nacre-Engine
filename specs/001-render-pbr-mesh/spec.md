# Feature Specification: Render a Single PBR Mesh into a Host-Provided Texture

**Feature Branch**: `001-render-pbr-mesh`

**Created**: 2026-06-09

**Status**: Draft

**Input**: User description: "Render a single PBR mesh into a host-provided texture — a minimal end-to-end slice (walking skeleton) that proves the engine architecture. The 'user' is the developer integrating the engine."

## Overview

This feature delivers the engine's **walking skeleton**: the smallest complete path
from creating the engine to a visible, physically based lit frame, without ever
taking control of the window, surface, event loop, or GPU device away from the host
application. It is the foundation every later feature builds upon, and it proves the
core architectural contract — the host owns input/output and the GPU device; the
engine only renders into a target the host provides.

The "user" throughout this specification is a **developer integrating the engine** as
a library into their own application.

## Clarifications

### Session 2026-06-09

- Q: The engine owns the depth buffer, but the host begins the render pass — wgpu fixes
  a pass's attachments at begin time. How is the engine's depth buffer attached? →
  A: The engine exposes its depth-stencil view and the depth format it expects; whoever
  begins the pass attaches that view. In the raw-wgpu integration the host attaches it
  when beginning the pass; in the iced integration the engine's shader-widget/Primitive
  adapter begins the pass with the same depth view internally. The public
  `render(pass, viewport)` contract is identical for both hosts. (FR-016)
- Q: Is the engine responsible for tone mapping and/or gamma/sRGB encoding? → A: No. The
  engine outputs **linear** color values and assumes the target is an **sRGB-encoded
  view** that performs the final encoding. Tone mapping and exposure are deferred to a
  later feature. (FR-017)
- Q: How are material maps (normal, AO) supplied to the engine? → A: As **already-created
  GPU textures** (texture views). The engine never accepts raw pixel data and never
  uploads, decodes, or loads textures itself (consistent with Principle III, no I/O
  ownership). (FR-018)
- Q: What does cross-platform "identical" output mean, and how is it verified? → A:
  Golden-image comparison with a **tolerance metric, never byte-exact**. Primary metric:
  in 8-bit sRGB space a render passes when at least 99% of pixels are within ±2 (per
  channel, of 255) of the golden and no pixel differs by more than ±8. Golden images may
  be maintained per platform to absorb backend differences, but any two platforms'
  goldens for the same scene must be perceptually equivalent (SSIM ≥ 0.99). Byte-exact
  equality across GPU backends is explicitly not required. (FR-019)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Minimal visible PBR frame on host-owned GPU (Priority: P1)

As a developer, I create an engine instance from my own existing GPU device and
command queue, tell it the target texture format and multisample count, give it a
built-in primitive, a camera, and a single directional light, then call `prepare()`
and `render()` so the engine draws a physically lit object into my frame — without
the engine ever creating a window, surface, event loop, or device.

**Why this priority**: This is the walking skeleton. If only this story ships, the
project already has a viable MVP: a working, embeddable PBR renderer that proves the
architecture end to end. Every other story is an extension of this path.

**Independent Test**: In the raw-wgpu reference integration, hand the engine a device,
a queue, and a target texture; configure a built-in cube, a perspective camera, and
one directional light; render one frame; confirm a correctly shaded, depth-tested
object appears in the target while the host retained ownership of the device and the
window.

**Acceptance Scenarios**:

1. **Given** a host that already owns a GPU device, queue, and a target texture,
   **When** the developer creates the engine by passing the device, queue, target
   texture format, and multisample count, **Then** the engine is ready to render and
   has created no window, surface, event loop, or device of its own.
2. **Given** a configured engine with a built-in primitive, a camera, and one
   directional light, **When** the developer calls `prepare()` and then `render()`
   inside their own render pass for a given viewport, **Then** a physically lit,
   depth-tested object is drawn within that viewport.
3. **Given** a `render()` call scoped to a viewport smaller than the full target,
   **When** the frame completes, **Then** only the pixels inside the viewport are
   affected and every pixel outside the viewport is left unchanged (the engine does
   not clear the whole target).
4. **Given** a viewport whose size differs from the previous frame, **When**
   `prepare()` runs, **Then** the engine creates or recreates its own depth buffer to
   match the new viewport size and the frame renders with correct depth occlusion.

---

### User Story 2 - Custom mesh geometry (Priority: P2)

As a developer, I supply my own vertex data (position, normal, UV, tangent) together
with indices, instead of a built-in primitive, so I can render my own geometry without
any external asset pipeline.

**Why this priority**: Built-in primitives prove the path, but real integrations need
to draw their own geometry. This unlocks practical use while staying asset-free.

**Independent Test**: Provide a hand-built indexed mesh (e.g., a quad or triangle with
explicit position/normal/UV/tangent) and confirm it renders identically in shape and
shading to an equivalent built-in primitive.

**Acceptance Scenarios**:

1. **Given** a custom vertex set with position, normal, UV, and tangent plus an index
   list, **When** the developer assigns it as the mesh, **Then** the engine renders
   that geometry using the same lighting and material path as built-in primitives.
2. **Given** a custom mesh, **When** it is rendered, **Then** normals and tangents are
   respected so lighting and (if present) normal mapping are correct.

---

### User Story 3 - Full metallic-roughness material control (Priority: P3)

As a developer, I set a PBR material — base color, metallic, roughness, and optionally
normal and ambient-occlusion (AO) maps — and when a map is absent the engine uses the
scalar value, so I can control surface appearance with or without textures.

**Why this priority**: Material control is what makes the renderer physically
expressive. It builds directly on the lit object from US1/US2.

**Independent Test**: Render the same primitive twice — once as a rough dielectric and
once as a smooth metal — and confirm the specular response differs as the
metallic-roughness model predicts; render once with and once without a normal map and
confirm surface detail appears only when the map is supplied.

**Acceptance Scenarios**:

1. **Given** a material with scalar base color, metallic, and roughness and no maps,
   **When** the object is rendered, **Then** shading follows the metallic-roughness
   model using those scalar values.
2. **Given** a material that additionally supplies a normal map, **When** the object is
   rendered, **Then** per-pixel surface detail from the normal map is visible.
3. **Given** a material that supplies an AO map, **When** the object is rendered,
   **Then** ambient/occluded regions are darkened accordingly.
4. **Given** a material that omits the normal and/or AO map, **When** the object is
   rendered, **Then** the engine falls back to the scalar/default behavior with no
   error.

---

### User Story 4 - Multiple analytical lights (Priority: P4)

As a developer, I provide a set of analytical lights — directional and/or point — so
the object is lit by more than one source, with only direct lighting (no image-based
lighting).

**Why this priority**: A single light proves the path; multiple lights make scenes
usable. Direct-only keeps scope tight.

**Independent Test**: Light one object with a directional light plus one or more point
lights and confirm each contributes a distinguishable, correctly positioned
contribution.

**Acceptance Scenarios**:

1. **Given** a set containing a directional light and one or more point lights,
   **When** the object is rendered, **Then** each light contributes direct illumination
   consistent with its type, direction/position, color, and intensity.
2. **Given** an empty light set, **When** the object is rendered, **Then** the engine
   renders without error (the object receives no direct illumination).

---

### User Story 5 - Same API embedded in iced (Priority: P5)

As a developer using iced, I drive the identical engine API from inside an iced shader
widget/Primitive, so the same rendering contract that works in a raw-wgpu application
also works inside an iced application.

**Why this priority**: The second reference integration proves the integration contract
is genuinely host-agnostic and not accidentally coupled to one host shape. It is the
portability proof, but it depends on US1 existing first.

**Independent Test**: Embed the engine in an iced shader widget, feed it the device,
queue, and target provided by iced, and confirm the same scene renders as in the
raw-wgpu reference integration.

**Acceptance Scenarios**:

1. **Given** an iced application using a shader widget/Primitive, **When** it creates
   and drives the engine with the device, queue, and target iced provides, **Then** the
   engine renders the scene inside the widget using the same `update → prepare → render`
   lifecycle as the raw-wgpu integration, with no engine-specific changes required for
   the iced host.

---

### Edge Cases

- **Zero-size or empty viewport**: How does the engine behave when the viewport has
  zero width or height? (Expected: render is a safe no-op for that frame.)
- **Viewport exceeding the target**: What happens when the requested viewport extends
  beyond the target texture bounds?
- **Unsupported multisample count**: What happens when the multisample count passed at
  creation is not supported for the target format on the current backend?
- **Normal map without tangents**: What happens when a material supplies a normal map
  but the custom mesh has no usable tangents?
- **Degenerate or empty geometry**: What happens when a custom mesh has no indices or
  no vertices?
- **Target format mismatch**: What happens when the render pass the host passes to
  `render()` was created with a color format different from the one declared at engine
  creation?
- **Invalid camera projection**: What happens with a degenerate projection (e.g., zero
  or negative near plane, zero field of view)?

## Requirements *(mandatory)*

### Functional Requirements

**Ownership & integration contract**

- **FR-001**: The engine MUST NOT create a window, surface, event loop, or GPU
  device/queue. All of these MUST be supplied by the host.
- **FR-002**: The engine MUST accept, at creation time, the host's GPU device, the
  host's command queue, the target texture color format, and the multisample (MSAA)
  sample count.
- **FR-003**: The engine MUST render into a render target the host provides, within a
  host-specified viewport, and MUST NOT clear or otherwise modify pixels outside that
  viewport.
- **FR-004**: The engine MUST expose a stable `update → prepare → render` lifecycle as
  its public contract (consistent with the project constitution, Principle V).
- **FR-005**: The feature MUST ship with two reference integrations that exercise the
  same public API: (a) a raw-wgpu application and (b) an iced application using a shader
  widget/Primitive.

**Geometry**

- **FR-006**: The engine MUST provide built-in primitives, at minimum a cube and a
  sphere, usable without any external asset.
- **FR-007**: The engine MUST accept custom mesh data consisting of per-vertex position,
  normal, UV, and tangent, together with an index list.
- **FR-008**: The engine MUST render exactly one mesh with one material per frame for
  this feature (single-mesh scope).

**Material (metallic-roughness PBR)**

- **FR-009**: The engine MUST support a metallic-roughness material defined by base
  color, metallic, and roughness scalar values.
- **FR-010**: The engine MUST optionally accept a normal map and an AO map; when a map
  is absent, the engine MUST fall back to the corresponding scalar/default value with no
  error.
- **FR-011**: Lighting MUST be physically based using the metallic-roughness workflow
  and MUST use **direct lighting only** (no image-based lighting).

**Lighting & camera**

- **FR-012**: The engine MUST accept a camera defined by a view transform and a
  projection transform.
- **FR-013**: The engine MUST support a set of analytical lights of type directional
  and point, each with at least direction/position (as appropriate), color, and
  intensity.

**Depth & frame correctness**

- **FR-014**: The engine MUST create and own a depth buffer and MUST recreate it to
  match the current viewport size when that size changes.
- **FR-015**: The engine MUST perform depth testing so that nearer surfaces correctly
  occlude farther surfaces.
- **FR-016**: The engine MUST expose its owned depth-stencil texture view and the depth
  format it expects, and `render(pass, viewport)` MUST be called with a render pass that
  already has that depth-stencil view attached. In the raw-wgpu integration the host
  attaches the engine's depth view when it begins the pass; in the iced integration the
  engine's shader-widget/Primitive adapter begins the pass with the same depth view
  internally. The public `render(pass, viewport)` contract is therefore identical for
  both hosts.

**Color & output correctness**

- **FR-017**: The engine MUST output **linear** color values and MUST assume the target
  is an **sRGB-encoded view** that performs the final encoding. The engine MUST NOT apply
  tone mapping or exposure in this feature; those are deferred to a later feature.

**Material texture supply**

- **FR-018**: Material maps (normal, AO) MUST be supplied to the engine as
  **already-created GPU textures** (texture views). The engine MUST NOT accept raw pixel
  data and MUST NOT upload, decode, or load textures itself (consistent with Principle
  III, no I/O ownership).

**Cross-platform parity**

- **FR-019**: The engine MUST produce equivalent rendering output for the same scene on
  Windows, Linux, and macOS (constitution, Principle II). Parity MUST be verified against
  golden reference images using a tolerance metric, **never** byte-exact comparison. The
  primary metric: in 8-bit sRGB space, a render passes when at least **99% of pixels are
  within ±2** (per channel, of 255) of the golden and **no pixel differs by more than
  ±8**. Golden images MAY be maintained per platform to absorb backend-specific
  differences; any two platforms' goldens for the same scene MUST be perceptually
  equivalent (**SSIM ≥ 0.99**). Byte-exact equality across GPU backends is explicitly NOT
  required.

### Key Entities

- **Engine instance**: The renderer the developer creates and drives; holds no window,
  surface, event loop, or device of its own — only references the host's device and
  queue.
- **Render target description**: The target's color format and multisample count,
  declared at creation so the engine's rendering matches the host's frame.
- **Viewport**: The host-specified rectangular sub-region of the target into which the
  current frame is drawn; also drives depth-buffer sizing.
- **Mesh / Geometry**: Either a built-in primitive (cube, sphere) or custom data with
  per-vertex position, normal, UV, tangent and an index list.
- **PBR material**: Base color, metallic, roughness, and optional normal and AO maps,
  following the metallic-roughness workflow.
- **Camera**: A view transform and a projection transform.
- **Light**: An analytical light, directional or point, with color and intensity (plus
  direction or position by type).
- **Depth buffer**: Engine-owned per-frame depth resource, sized to the viewport and
  recreated on size change.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can go from engine creation to a visible, physically lit frame
  in their own target texture while writing **zero** lines of code that create a window,
  surface, event loop, or GPU device — the host supplies all of them.
- **SC-002**: When `render()` is scoped to a viewport, **100%** of pixels outside the
  viewport remain unchanged from before the call (verifiable by comparing the target
  before and after).
- **SC-003**: The **same** public engine API renders the same scene successfully in both
  reference integrations (raw-wgpu and iced), with no host-specific branches in the
  engine.
- **SC-004**: For the same scene, the rendered frame is consistent across Windows, Linux,
  and macOS within the FR-019 tolerance — at least 99% of pixels within ±2/255 per
  channel of the golden (none beyond ±8), and SSIM ≥ 0.99 between any two platforms'
  goldens — with no requirement of byte-exact equality across backends.
- **SC-005**: Material changes are visibly correct: increasing roughness from 0 to 1
  monotonically broadens the specular highlight, and increasing metallic from 0 to 1
  shifts the specular response toward the base color — both verifiable from reference
  renders.
- **SC-006**: After a viewport size change, the very next frame renders with correct
  depth occlusion and no stale-depth artifacts.
- **SC-007**: A scene with a directional light plus at least one point light shows each
  light's distinguishable, correctly located contribution.

## Assumptions

- **Coordinate conventions**: Right-handed world space, Y-up, counter-clockwise
  front-facing winding (glTF-like) are assumed unless clarified otherwise. The host's
  view and projection transforms are expected to match these conventions.
- **Multisample counts**: Sample counts 1 and 4 are assumed to be the supported baseline
  (the values wgpu guarantees broadly); other counts are out of scope for this feature.
- **Per-frame state**: Geometry, material, camera, and lights are settable before
  `prepare()` each frame; the walking skeleton renders a single mesh with a single
  material per frame.
- **Light capacity**: A small fixed maximum number of lights is assumed sufficient for
  this feature; the exact cap is a planning detail, not a scope driver.
- **Built-in primitive tangents**: Built-in primitives are assumed to ship with
  generated normals and tangents so they work with normal-mapped materials.
- **Target lifetime**: The host keeps the device, queue, and target texture valid for as
  long as the engine is used; the engine does not outlive or take ownership of them.
- **Reference integrations are examples**: The two reference integrations live as example
  binaries/apps that own the window and device, consistent with the constitution
  (Principle III); they are not part of the library's public surface.

## Out of Scope *(explicitly deferred)*

- Image-based lighting (IBL), shadows, transparency, and post-processing.
- glTF and any external asset loading.
- Instancing/batching and a scene graph with a transform hierarchy.
- Light types beyond directional and point.
- Rendering more than one mesh or more than one material per frame.
