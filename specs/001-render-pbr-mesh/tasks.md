---
description: "Task list for 001-render-pbr-mesh"
---

# Tasks: Render a Single PBR Mesh into a Host-Provided Texture

**Input**: Design documents from `specs/001-render-pbr-mesh/`

**Prerequisites**: plan.md, spec.md (required); research.md, data-model.md, contracts/, quickstart.md (available)

**Tests**: Golden-image and validation tests **are included** — the spec mandates
cross-platform parity verification (FR-019 / SC-004) and two reference integrations (FR-005).

**Organization**: Tasks are grouped by user story (priority order P1→P5) for independent
implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US5; Setup/Foundational/Polish carry no story label
- Exact file paths are included in every task

## Path Conventions

Single library crate at the workspace root (`src/`, `tests/`), with reference integrations as
member crates under `examples/` and golden fixtures under `tests/golden/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace, dependencies, and project skeleton.

- [x] T001 Convert the root `Cargo.toml` to a Cargo workspace: keep `[package] nacre-engine` as a library, add `[workspace]` with members `examples/raw-wgpu` and `examples/iced-demo`, and set `autoexamples = false` so the root package does not scan `examples/` member crates, in `Cargo.toml`
- [x] T002 Declare dependencies in `Cargo.toml`: `wgpu = "27"`, `glam`, `bytemuck` (with `derive`); `[dev-dependencies]` `pollster`, `image`
- [x] T003 [P] Create the library module skeleton (declare `engine`, `scene`, `mesh`, `material`, `pipeline`, `uniforms`, `depth` modules and re-export the public API; keep `VERSION`) in `src/lib.rs`
- [x] T004 [P] Scaffold the raw-wgpu example member crate (`nacre-engine` path dep + `winit` + `pollster` + `wgpu = "27"`) in `examples/raw-wgpu/Cargo.toml` and a stub `examples/raw-wgpu/src/main.rs`
- [x] T005 [P] Scaffold the iced-demo example member crate (`nacre-engine` path dep + `iced = "0.14"`) in `examples/iced-demo/Cargo.toml` and a stub `examples/iced-demo/src/main.rs`
- [x] T006 [P] Add `.gitattributes` with `* text=auto eol=lf` for cross-platform line-ending parity (Principle II) in `.gitattributes`
- [x] T007 [P] Add the CI matrix (Windows/Linux/macOS) running `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`, provisioning software adapters (lavapipe on Linux, WARP on Windows) for headless tests, in `.github/workflows/ci.yml`
- [x] T008 [P] Remove the superseded placeholder example `examples/demo.rs` (replaced by `examples/raw-wgpu`)

**Checkpoint**: `cargo build` succeeds for an empty workspace; example crates compile as stubs.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared engine plumbing required before ANY user story. No lighting math or map
sampling yet (those belong to the stories that introduce them).

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T009 [P] Define host-facing input types `Viewport`, `Camera`, `Light` (enum), `Scene` in `src/scene.rs`
- [x] T010 [P] Define `Material` (scalars + optional `&wgpu::TextureView` maps) in `src/material.rs`
- [x] T011 [P] Define `Primitive` (enum), `MeshData`, `MeshHandle`, and the `Vertex` bytemuck struct with its 48-byte `wgpu::VertexBufferLayout` in `src/mesh.rs`
- [x] T012 [P] Define GPU uniform structs `CameraUniform`, `ModelUniform`, `LightStd`, `LightsUniform` (`MAX_LIGHTS = 8`), `MaterialUniform` (bytemuck `Pod`) in `src/uniforms.rs`
- [x] T013 [P] Implement depth-texture create/recreate sized to the viewport (`Depth32Float`, matching `sample_count`) in `src/depth.rs`
- [x] T014 Write the WGSL scaffold in `src/shaders/pbr.wgsl`: vertex stage (clip position, world normal/tangent, uv) and a fragment stage that outputs linear base color (no lighting yet); declare all bindings (group 0: camera/model/lights; group 1: material uniform + normal + AO + sampler)
- [x] T015 Implement bind group layouts (group 0, group 1) and the render pipeline (vertex layout + multisample state from `sample_count`, depth-stencil state, color target = `EngineConfig.target_format`) in `src/pipeline.rs`
- [x] T016 Define `EngineConfig`, `EngineError`, the `Engine` struct, `Engine::new` (build pipeline/layouts, uniform buffers, internal 1×1 placeholder normal/AO textures + sampler), and skeleton lifecycle methods `update`/`prepare`/`render` plus `depth_view()`/`depth_format()` in `src/engine.rs`

**Checkpoint**: the engine constructs and can clear-load a pass; foundation ready for stories.

---

## Phase 3: User Story 1 - Minimal visible PBR frame (Priority: P1) 🎯 MVP

**Goal**: From host-owned device/queue + target, render a built-in primitive lit by one
directional light into the host's pass/viewport, depth-tested, without the engine owning any I/O.

**Independent Test**: Run `cargo run -p raw-wgpu` and see a directional-lit, depth-tested cube
drawn into the host frame within the viewport; the engine creates no window/device.

- [x] T017 [US1] Implement `builtin_mesh(Primitive::Cube)` (generated normals + tangents) and the vertex/index buffer upload returning a `MeshHandle` in `src/mesh.rs`
- [x] T018 [US1] Implement the lifecycle bodies in `src/engine.rs`: `update(scene)` records state; `prepare(device, queue, viewport)` uploads camera/model/lights/material uniforms and ensures the depth texture matches the viewport; `render(pass, viewport)` sets pipeline, bind groups, viewport + scissor, and issues the indexed draw (no clear)
- [x] T019 [US1] Add the directional-light Cook-Torrance term to `src/shaders/pbr.wgsl` (GGX `D`, Smith-Schlick `G`, Schlick `F`, `F0 = mix(0.04, base_color, metallic)`, Lambert diffuse × `(1 - metallic)`, summed over directional lights; linear output)
- [x] T020 [US1] Implement the scalar-material path (bind placeholders for absent maps, `flags = 0`) so the frame renders from `base_color`/`metallic`/`roughness` in `src/material.rs`
- [x] T021 [US1] Implement the raw-wgpu reference integration in `examples/raw-wgpu/src/main.rs`: winit window + host adapter/device/queue/surface, engine creation, per-frame `update`/`prepare`/begin-pass-with-`depth_view()`/`render`/submit, directional light + camera + built-in cube, and resize handling (depth recreated via `prepare`)
- [x] T022 [P] [US1] Implement golden-test infrastructure in `tests/golden.rs`: request a headless adapter (no surface), render a scene to an offscreen texture, read it back, and compare to a PNG using the FR-019 metric (≥99% pixels within ±2/255, none beyond ±8) plus an SSIM helper
- [x] T023 [US1] Add the US1 golden test in `tests/golden.rs` (directional-lit cube vs `tests/golden/<platform>/cube_directional.png`) and assert viewport containment — pixels outside the viewport are unchanged (SC-002)

**Checkpoint**: US1 is a fully functional, independently testable MVP — a visible lit frame.

---

## Phase 4: User Story 2 - Custom mesh geometry (Priority: P2)

**Goal**: Render developer-supplied vertices (position/normal/uv/tangent) + indices through the
same path as built-ins.

**Independent Test**: Swap the built-in cube for a hand-built `MeshData`; it renders with correct
shading, and invalid data returns `EngineError` at `create_mesh`.

- [x] T024 [US2] Implement `create_mesh(device, &MeshData) -> Result<MeshHandle, EngineError>` with validation (non-empty, attribute length parity, `indices.len() % 3 == 0`, every index < vertex count) in `src/mesh.rs`
- [x] T025 [P] [US2] Implement `builtin_mesh(Primitive::Sphere)` (UV sphere with generated normals + tangents) in `src/mesh.rs`
- [x] T026 [P] [US2] Add unit tests for mesh validation errors (length mismatch, out-of-range index, non-triangle index count) — implemented as a `#[cfg(test)]` module in `src/mesh.rs` (validator is `pub(crate)`)
- [ ] T027 [P] [US2] Add a golden test for a custom indexed mesh rendering equivalently to a built-in under the same scene (`tests/golden/<platform>/custom_mesh.png`) in `tests/golden.rs`
- [x] T028 [US2] Add a mesh-source toggle (built-in vs custom) to `examples/raw-wgpu/src/main.rs`

**Checkpoint**: Custom and built-in geometry both render; US1 still works.

---

## Phase 5: User Story 3 - Full metallic-roughness material (Priority: P3)

**Goal**: Control base color, metallic, roughness, and optional normal/AO maps with scalar
fallback.

**Independent Test**: Rough dielectric vs smooth metal differ per the model (SC-005); a normal map
adds detail, and omitting it falls back with no error.

- [x] T029 [US3] Add normal-map sampling + tangent-space normal reconstruction to `src/shaders/pbr.wgsl`, gated by the `has_normal_map` flag with geometric-normal fallback
- [x] T030 [US3] Add AO-map sampling gated by the `has_occlusion_map` flag and apply it to the lit result in `src/shaders/pbr.wgsl`
- [x] T031 [US3] Wire `Material.normal_map`/`occlusion_map` (`Option<&TextureView>`) into the material bind group — bind the host view or the 1×1 placeholder and set `flags` accordingly — in `src/material.rs` and `src/engine.rs`
- [ ] T032 [P] [US3] Add golden tests for material response (rough dielectric vs smooth metal, SC-005) and normal-map on/off (`tests/golden/<platform>/material_*.png`) in `tests/golden.rs`

**Checkpoint**: Full material control works; earlier stories still pass.

---

## Phase 6: User Story 4 - Multiple analytical lights (Priority: P4)

**Goal**: Light the object with a set of directional and/or point lights (direct-only).

**Independent Test**: A directional light plus two point lights each contribute visibly (SC-007);
an empty light slice renders black without error.

- [x] T033 [US4] Add the point-light term (inverse-square falloff + `range` cutoff) to `src/shaders/pbr.wgsl` and iterate `0..count` lights, branching on `kind` for directional vs point
- [x] T034 [US4] Pack a `&[Light]` (0..=`MAX_LIGHTS`) into `LightsUniform` with `count`, clamping with a warning when the slice exceeds `MAX_LIGHTS`, in `src/engine.rs`
- [ ] T035 [P] [US4] Add a golden test for directional + two point lights (`tests/golden/<platform>/multi_light.png`) and assert an empty light slice renders without error in `tests/golden.rs`

**Checkpoint**: Multi-light scenes work; earlier stories still pass.

---

## Phase 7: User Story 5 - Same API embedded in iced (Priority: P5)

**Goal**: Drive the identical engine API inside an iced shader widget, proving host-agnosticism.

**Independent Test**: Run `cargo run -p iced-demo` and see the same scene render inside the iced
widget bounds, with no engine code changes vs the raw-wgpu path.

- [x] T036 [US5] Implement `NacrePrimitive` (iced `shader::Primitive`) in `examples/iced-demo/src/main.rs`: `prepare` → `engine.prepare`; `render` → begin a pass with the iced target as color and `engine.depth_view()` as depth, scissor = `clip_bounds`, then `engine.render`; import wgpu via `iced::widget::shader::wgpu`
- [x] T037 [US5] Build the iced application hosting the `shader` widget rendering the same scene as the raw-wgpu example in `examples/iced-demo/src/main.rs`
- [x] T038 [US5] Add an iced-integration smoke check (headless `prepare` call drives the engine with no panic and no engine-side host branching) in `examples/iced-demo/src/main.rs`

**Checkpoint**: Both reference integrations exercise the same public API (SC-003).

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates and constitution compliance.

- [x] T039 [P] Run the Principle I dependency audit: `cargo tree -p nacre-engine -e normal` shows `wgpu`/`glam`/`bytemuck`/`log` plus two pure-Rust transitive `-sys` crates (`glutin_wgl_sys`, `renderdoc-sys`) that build without system packages and only load OS libraries at runtime — explicitly allowed by Principle I as clarified in constitution v1.1.0 (commit 0ccf198)
- [x] T040 [P] Add rustdoc to the public API (`Engine`, lifecycle, `Scene`, types) with a crate-level usage example in `src/lib.rs`; ensure `cargo doc` is clean
- [x] T041 [P] Ensure `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean across the workspace
- [x] T042 Generate and commit the per-platform golden images and document the regeneration procedure (env var + review step) in `tests/golden.rs`
- [ ] T043 Validate quickstart scenarios 1–6 end to end and update `specs/001-render-pbr-mesh/quickstart.md` if any step drifted
- [x] T044 [P] Add the `log` facade and emit a `warn!` when a scene exceeds `MAX_LIGHTS` instead of silently truncating, per Principle VI (done: commit 25df342)
- [x] T045 [P] Enforce the Principle VI/VIII clippy lints (`print_stdout`, `print_stderr`, `dbg_macro`, `undocumented_unsafe_blocks` = deny) for the library in `src/lib.rs` (done: commit 25df342)

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 → T002 (same file) block everything; T003–T008 follow.
- **Foundational (Phase 2)**: depends on Setup; blocks ALL user stories.
- **User stories (Phases 3–7)**: all depend on Foundational. Recommended order is priority
  order P1→P5. US1 must land first (it implements the lifecycle bodies the others build on).
  US2 (geometry), US3 (material), and US4 (lighting) are largely independent in concept but
  share `src/engine.rs` and `src/shaders/pbr.wgsl`, so run them sequentially to avoid file
  conflicts. US5 depends on a working engine (US1).
- **Polish (Phase 8)**: after all targeted stories complete.

### Key blocking points

- T014 (shader scaffold) + T015 (pipeline) + T016 (engine) gate T018/T019 (US1 render).
- T022 (golden infra) gates every later golden test (T023, T027, T032, T035).
- T018 (lifecycle bodies) gates US5's `NacrePrimitive`.

### Within each user story

- Shader/material/engine wiring before the example update; example/engine before the golden test.

---

## Parallel Opportunities

- **Setup**: T003, T004, T005, T006, T007, T008 in parallel (distinct files) after T002.
- **Foundational**: T009, T010, T011, T012, T013 in parallel (distinct files); T014→T015→T016
  are sequential (pipeline depends on shader bindings; engine depends on pipeline).
- **US1**: T022 (golden infra) can proceed in parallel with engine/shader work (T017–T021).
- **US2**: T025, T026, T027 in parallel after T024.
- Within US3/US4, golden tests (T032, T035) are [P] once their shader/engine changes land.

### Parallel example: Foundational data types

```text
Task: "Define Viewport/Camera/Light/Scene in src/scene.rs"        (T009)
Task: "Define Material in src/material.rs"                          (T010)
Task: "Define Primitive/MeshData/MeshHandle/Vertex in src/mesh.rs"  (T011)
Task: "Define GPU uniform structs in src/uniforms.rs"              (T012)
Task: "Implement depth create/recreate in src/depth.rs"            (T013)
```

---

## Implementation Strategy

### MVP first (User Story 1 only)

1. Complete Phase 1 (Setup) and Phase 2 (Foundational).
2. Complete Phase 3 (US1): a directional-lit built-in primitive rendered into the host frame via
   the raw-wgpu example, plus its golden test.
3. **STOP and VALIDATE**: `cargo run -p raw-wgpu` shows a lit cube; `cargo test --test golden`
   passes. This is a shippable walking skeleton.

### Incremental delivery

US1 (MVP) → US2 (custom geometry) → US3 (full material) → US4 (multi-light) → US5 (iced).
Each story keeps prior stories green and adds one distinguishable, independently testable
capability.

### Notes

- `[P]` = different files, no incomplete dependency.
- Golden images are per-platform (FR-019); commit them only after reviewing diffs.
- Keep `src/engine.rs` and `src/shaders/pbr.wgsl` edits serialized across stories (shared files).
- Run the quality gates (`fmt`, `clippy`, `test`) after each task or logical group.
