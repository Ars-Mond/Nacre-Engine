# Phase 0 Research: Render a Single PBR Mesh

**Feature**: `001-render-pbr-mesh` | **Date**: 2026-06-10

All clarifications from the spec are resolved; this document records the technical
decisions that shape the design, with rationale and rejected alternatives.

## 1. wgpu version (pinned to the iced integration)

- **Decision**: Pin the library to `wgpu = 27`.
- **Rationale**: The latest published wgpu is 29.0.3, but iced 0.14.0 (latest) depends on
  `wgpu ^27`. The iced shader widget hands the engine a `Device`, `Queue`, and target
  `TextureView` that are **iced's** wgpu types. For those to be the same Rust types as the
  engine's, both must resolve to one semver-compatible wgpu instance. Using wgpu 29 in the
  engine would give Cargo two incompatible wgpu copies (27 for iced, 29 for the engine) and
  the iced integration would not compile. Matching iced's major version is mandatory, not
  cosmetic.
- **Coupling rule**: the engine's wgpu version tracks the iced reference integration. When
  iced upgrades its wgpu, the engine follows in the same change. iced re-exports its wgpu at
  `iced::widget::shader::wgpu`; the iced example uses that path to guarantee a single wgpu.
- **Alternatives considered**: (a) wgpu 29 + drop iced parity — rejected, FR-005 requires the
  iced integration; (b) an FFI/ABI shim between wgpu versions — rejected, violates Principle I
  and is infeasible for wgpu's borrow-checked handles.

## 2. iced shader-widget integration shape

- **Decision**: The iced integration lives entirely in the `iced-demo` example crate as an
  implementor of iced's `shader::Primitive`; the library exposes no iced-specific API.
- **Rationale**: iced's `Primitive::prepare(device, queue, format, …)` and
  `Primitive::render(encoder, storage, target, clip_bounds)` give the integrator a command
  encoder plus a target view and expect the integrator to begin its own render pass. The
  adapter begins a pass with the iced target as color and the engine's depth view as depth,
  sets the scissor to `clip_bounds`, then calls the engine's public `prepare()` and
  `render(pass, viewport)`. Because the public API only needs `Device`/`Queue`/pass/target,
  no iced dependency leaks into the library (Principle IV) and the example genuinely proves
  host-agnosticism.
- **Alternatives considered**: ship a first-class `iced` feature with a `NacrePrimitive` in
  the library — rejected for this feature: it couples the library to iced and contradicts the
  spec assumption that reference integrations are examples, not public surface. Can be
  revisited later as an optional feature crate.

## 3. Depth buffer ownership and attachment (FR-016)

- **Decision**: The engine owns a depth texture sized to the viewport, recreated on size
  change, and exposes `depth_view()` and `depth_format()`. The pass opener attaches it.
- **Rationale**: wgpu fixes a render pass's attachments at `begin_render_pass`. Since the host
  begins the pass in the raw-wgpu case, the host must attach the engine's depth view there; in
  iced, the engine's Primitive adapter begins the pass and attaches it. The public
  `render(pass, viewport)` is identical in both. Depth format: **`Depth32Float`** — universally
  supported across DX12/Vulkan/Metal in wgpu and needs no stencil for this feature.
- **Alternatives considered**: `Depth24PlusStencil8` (more memory, stencil unused);
  `render()` taking a color target and beginning its own pass (rejected — the spec's scenario 5
  keeps pass ownership with the host for raw wgpu).

## 4. MSAA handling

- **Decision**: Accept `sample_count` (supported values 1 and 4) at creation; build the
  pipeline's multisample state with it; create the depth texture with the same sample count.
  The engine does **not** own or resolve the multisampled color target.
- **Rationale**: The engine renders into the host's color attachment, so MSAA resolve is the
  host's responsibility (it controls the color attachment and resolve target). The engine only
  needs its pipeline and depth sample counts to match the host's color attachment. wgpu
  guarantees sample counts 1 and 4 broadly.
- **Alternatives considered**: engine-owned MSAA color + resolve — rejected, would mean owning
  the color target and breaks the "render into host's pass" contract.

## 5. Color space and output (FR-017)

- **Decision**: The fragment shader outputs **linear** color. The engine assumes the target
  view is sRGB-encoded (e.g., `Rgba8UnormSrgb` / `Bgra8UnormSrgb`), so the hardware performs
  sRGB encoding on write. No tone mapping or exposure in this feature.
- **Rationale**: This is the simplest physically consistent path: lighting math stays in linear
  space, and the sRGB target view does the gamma encoding for free. Tone mapping/exposure is a
  separate, later feature. Base color inputs are treated as linear factors; if a future base
  color texture is added it must be an sRGB-sampled texture (out of scope here).
- **Alternatives considered**: engine-side manual gamma/tone mapping into a linear `Unorm`
  target — rejected as premature and a source of double-encoding bugs.

## 6. Coordinate conventions

- **Decision**: Right-handed world space, Y-up, counter-clockwise front faces; wgpu clip space
  with depth range 0..1; standard (non-reversed) depth. The host supplies view and projection
  matrices matching these conventions (e.g., glam `Mat4::look_at_rh` + `perspective_rh`).
- **Rationale**: Matches glam's `_rh` helpers and glTF conventions, minimizing surprise for
  integrators. Reverse-Z (better precision) is deferred to keep the skeleton simple.
- **Alternatives considered**: left-handed/Y-down or reverse-Z — rejected as unnecessary
  complexity for the walking skeleton.

## 7. Lighting model (Cook-Torrance, metallic-roughness, direct only)

- **Decision**: Standard metallic-roughness direct BRDF in `pbr.wgsl`:
  - Distribution `D`: GGX / Trowbridge-Reitz.
  - Geometry `G`: Smith with Schlick-GGX, using the direct-lighting `k = (roughness+1)²/8`.
  - Fresnel `F`: Schlick approximation; `F0 = mix(0.04, base_color, metallic)`.
  - Diffuse: Lambertian, scaled by `(1 - metallic)` and energy-conserving `(1 - F)`.
  - Directional lights: constant direction/radiance. Point lights: inverse-square distance
    falloff with an optional range cutoff.
  - Final color = sum over lights of `(diffuse + specular) · radiance · NdotL`. No ambient/IBL
    term (direct only), so unlit regions are black by design (FR-011, US4 empty-light case).
- **Rationale**: This is the textbook glTF-compatible metallic-roughness model; it makes
  SC-005 (roughness broadens highlight, metallic tints specular) hold by construction.
- **Alternatives considered**: multiscatter/energy-compensation terms, IBL ambient — deferred
  (out of scope).

## 8. Optional material maps and binding placeholders (FR-018)

- **Decision**: Normal and AO maps are supplied as host-created `wgpu::TextureView`s. When a
  map is absent, the engine binds a tiny engine-internal 1×1 placeholder view and a uniform
  flag tells the shader to use the scalar/default path instead of sampling it.
- **Rationale**: wgpu requires every declared texture binding to have a valid view, so an
  unused slot still needs *something* bound. A 1×1 internal placeholder is engine resource
  management (like the depth texture and uniform buffers), distinct from FR-018's prohibition,
  which targets **material content**: the engine never decodes files or accepts host raw-pixel
  arrays as material data. This reading is recorded explicitly so the boundary is intentional,
  not accidental.
- **Alternatives considered**: per-combination pipelines/bind-group-layouts to avoid
  placeholders — rejected as combinatorial overkill for two optional maps; requiring the host
  to always pass maps — rejected, contradicts "optional maps" (FR-010).

## 9. Mesh upload strategy

- **Decision**: `Engine::create_mesh(device, &MeshData) -> MeshHandle` uploads vertex/index
  buffers once and returns an opaque handle; built-in primitives are created the same way
  (`Engine::builtin_mesh(device, Primitive)`). `Scene` references a `MeshHandle`. Custom mesh
  data must already include tangents.
- **Rationale**: Uploading static geometry once and referencing it per frame avoids per-frame
  buffer churn (Performance note in plan) while staying within single-mesh scope. Requiring
  tangents up front keeps normal mapping correct without an in-engine tangent generator
  (deferrable). Built-in primitives generate their own normals and tangents.
- **Alternatives considered**: re-upload vertices every `update()` — rejected as wasteful and
  poor contract hygiene; auto-generate tangents when missing — deferred (extra scope).

## 10. Pure-Rust dependency audit (Principle I gate)

- **Decision**: Treat "no `*-sys` / no system build packages" as a verifiable gate: after wiring
  dependencies, run `cargo tree` and confirm the **library** graph (wgpu + glam + bytemuck) has
  no `*-sys` crate and builds on clean Windows/Linux/macOS stable toolchains with no system
  packages. winit/iced (examples only) may bring platform windowing crates; those are outside
  the library and are runtime windowing requirements, not library build dependencies.
- **Rationale**: wgpu's backends load GPU drivers at runtime (Vulkan via runtime loading, DX12
  via the pure-Rust `windows` bindings, Metal via the pure-Rust `metal` crate) rather than
  linking system C/C++ libraries at build time, so the library builds cleanly without `-dev`
  packages. The audit makes this explicit rather than assumed.
- **Alternatives considered**: assume compliance without checking — rejected; Principle I is a
  hard gate and must be demonstrable in CI.

## 11. Golden-image testing and CI adapters (FR-019)

- **Decision**: Headless tests request a wgpu adapter without a surface, render a fixed scene to
  an offscreen texture, copy it to a buffer, read it back, and compare against a golden PNG
  using the FR-019 metric (≥99% of pixels within ±2/255 per channel, none beyond ±8; SSIM ≥ 0.99
  between per-platform goldens). `image` reads/writes PNGs; `pollster` blocks on async wgpu.
  CI uses software adapters where needed (Vulkan/lavapipe on Linux, DX12/WARP on Windows; macOS
  uses its hardware Metal), and goldens are maintained per platform.
- **Rationale**: Software adapters make CI deterministic and GPU-independent; per-platform
  goldens (allowed by FR-019) absorb backend differences while the SSIM cross-check keeps them
  perceptually equivalent. Installing a software Vulkan driver on a Linux CI runner is a
  *runtime test* requirement, not a library build dependency, so Principle I still holds.
- **Alternatives considered**: a single shared golden across backends with byte-exact compare —
  rejected (infeasible across DX12/Vulkan/Metal, explicitly excluded by FR-019).

## 12. Light capacity

- **Decision**: Fixed maximum of **8** lights in a uniform array plus an active count; extra
  lights beyond the cap are ignored with a logged warning (or debug assert).
- **Rationale**: A small fixed cap keeps the uniform layout simple and bindings static for the
  walking skeleton; 8 covers the multi-light acceptance scenario (US4). Dynamic/large light
  counts (storage buffers, clustering) are out of scope.
- **Alternatives considered**: storage-buffer-backed unbounded lights — deferred as premature.

## Open Risks

- **iced ↔ wgpu version drift**: the most likely breakage source over time; mitigated by the
  coupling rule in §1 and by the iced example importing wgpu through `iced::widget::shader::wgpu`.
- **CI software-adapter availability**: lavapipe/WARP provisioning must be encoded in `ci.yml`;
  if a runner lacks an adapter, golden tests are skipped with a clear message rather than passing
  silently.

## Sources

- [wgpu — crates.io](https://crates.io/crates/wgpu) (latest 29.0.3)
- [iced_wgpu — docs.rs](https://docs.rs/crate/iced_wgpu/latest) (0.14.0 depends on `wgpu ^27`)
- [iced — crates.io](https://crates.io/crates/iced) (latest 0.14.0)
- [iced shader widget — docs.rs](https://docs.rs/iced/latest/iced/widget/shader/index.html)
