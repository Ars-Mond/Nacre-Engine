# Implementation Plan: Render a Single PBR Mesh into a Host-Provided Texture

**Branch**: `001-render-pbr-mesh` | **Date**: 2026-06-10 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-render-pbr-mesh/spec.md`

## Summary

Deliver the engine's walking skeleton: a library that renders one metallic-roughness
PBR mesh into a host-provided texture, driven by a stable `update → prepare → render`
lifecycle, while the host keeps ownership of the window, event loop, and the wgpu
`Device`/`Queue`. Lighting is direct-only (Cook-Torrance BRDF, metallic-roughness
workflow). The engine owns and resizes its depth buffer and exposes the depth view so
whoever begins the render pass (the host in raw wgpu, the engine's adapter in iced) can
attach it. Two reference integrations — a raw-wgpu app and an iced shader widget — prove
the public API is host-agnostic. Cross-platform parity is validated with golden-image
tests under a tolerance metric.

## Technical Context

**Language/Version**: Rust, stable toolchain, edition 2024 (constitution: stable only).

**Primary Dependencies** (library crate): `wgpu = 27` (pinned to match iced 0.14's
`wgpu ^27` so the shared `Device`/`Queue`/types unify into one crate instance), `glam`
(math), `bytemuck` (GPU data packing), and `log` (logging facade, required by Principle VI). No
other runtime dependencies.

**Dev/Example Dependencies** (isolated, never reach library consumers): `pollster` +
`image` as library `[dev-dependencies]` for headless golden-image tests; `winit` +
`pollster` in the raw-wgpu example crate; `iced = 0.14` in the iced example crate.

**Storage**: N/A (no persistence; golden PNGs are test fixtures only).

**Testing**: `cargo test` with headless wgpu (adapter requested without a surface),
render-to-texture → buffer readback → PNG compare against golden images using the FR-019
tolerance metric. `cargo fmt --check` and `cargo clippy --all-targets -D warnings` as
gates.

**Target Platform**: Windows (DX12), Linux (Vulkan), macOS (Metal) desktop, via wgpu.

**Project Type**: Single library crate (`nacre-engine`) at the workspace root, plus two
example member crates and golden-image tests. WGSL shaders only.

**Performance Goals**: Real-time; a single mesh renders far above 60 fps. Performance is
not a binding constraint for this feature (the goal is architectural proof), but the
prepare/render split must avoid per-frame pipeline or mesh re-creation.

**Constraints**: Engine never creates a window/surface/event loop/`Device`/`Queue`
(Principle III). Pure-Rust build with no system packages (Principle I). WGSL shaders
only. Identical-within-tolerance output across the three OSes (FR-019). Linear color
output into an sRGB target view; no tone mapping (FR-017). Material maps supplied as
host-created GPU textures (FR-018).

**Scale/Scope**: One mesh + one material per frame; a small fixed maximum number of
lights (planned cap: 8); built-in cube and UV sphere primitives.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Gate | Status |
|-----------|------|--------|
| I. Pure Rust, zero native deps | Library deps are `wgpu` + `glam` + `bytemuck`, all pure Rust; `cargo build` works on a clean stable toolchain with no system C/C++ packages. The runtime GPU driver/adapter is a *runtime* requirement of GPU rendering, not a *build* dependency, so it does not breach Principle I. | PASS (dependency-tree audit is a Phase 0 research task) |
| II. Cross-platform parity | CI matrix builds and tests on Windows, Linux, macOS; golden-image tests enforce FR-019 parity; no platform-specific code outside documented abstractions. | PASS (CI workflow added in implementation) |
| III. Library, not I/O owner | Public API accepts `Device`, `Queue`, target format, and a render pass/target from the host; the engine creates no window, surface, event loop, or device. | PASS by design |
| IV. Minimal explicit dependencies | Exactly four library dependencies (`wgpu`, `glam`, `bytemuck`, `log`), each justified; example-only deps (`winit`, `iced`) live in separate workspace member crates; test-only deps (`pollster`, `image`) are `[dev-dependencies]`. None reach library consumers. | PASS |
| V. Integration first, API is contract | `update → prepare → render` is the public lifecycle; depth-view exposure and types are documented in `contracts/`; breaking changes follow SemVer major. | PASS |
| VI. Structured logging discipline | All logging goes through the `log` facade; no `println!`/`eprintln!`/`dbg!` in library code; clippy `print_stdout`/`print_stderr`/`dbg_macro` denied; errors are propagated or logged (e.g. a `warn!` when lights exceed `MAX_LIGHTS`), never silently dropped. | PASS (enforced in `src/lib.rs`; commit 25df342) |
| VII. Phase commit discipline | Each Spec Kit phase is committed via `/speckit-git-commit` before the next; one commit per phase. | PASS (followed throughout) |
| VIII. Safe code discipline | No bare `.unwrap()` in library code; `.expect()` only with an invariant message; no `unsafe` in the engine; clippy `undocumented_unsafe_blocks` denied. | PASS (enforced in `src/lib.rs`; commit 25df342) |
| Tech constraints | Shaders are WGSL only (`pbr.wgsl`); all code/comments/docs in English. | PASS |

No violations. Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-render-pbr-mesh/
├── spec.md              # Feature specification (clarification-complete)
├── plan.md              # This file (/speckit-plan output)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── public-api.md     # The library's public Rust API (the primary contract)
│   └── integration.md    # Host integration contract (raw wgpu + iced), depth-view protocol
├── checklists/
│   └── requirements.md  # Spec quality checklist (all items pass)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

The repository becomes a Cargo workspace: the library at the root, example integrations
as member crates (keeps `winit`/`iced` out of the library's dependency graph).

```text
Cargo.toml               # [package] nacre-engine (library) + [workspace] members
src/
├── lib.rs               # Public API surface + re-exports (Engine, Scene, types)
├── engine.rs            # Engine: new(), update(), prepare(), render(); owns GPU resources
├── scene.rs             # Scene, Camera, Light, Viewport (host-facing input types)
├── mesh.rs              # MeshData, Vertex (bytemuck), MeshHandle, built-in cube/sphere
├── material.rs          # Material (scalars + optional host texture views), internal defaults
├── pipeline.rs          # Render pipeline + bind group layouts (cached)
├── uniforms.rs          # GPU uniform structs (CameraUniform, LightsUniform, MaterialUniform)
├── depth.rs             # Depth texture creation/recreation sized to viewport
└── shaders/
    └── pbr.wgsl         # WGSL: vertex + Cook-Torrance metallic-roughness fragment

tests/
├── golden.rs            # Headless render-to-texture golden-image tests
└── golden/              # Reference images (per-platform per FR-019)
    ├── windows/
    ├── linux/
    └── macos/

examples/
├── raw-wgpu/            # Reference integration (a): bin crate — winit + wgpu + nacre-engine
│   ├── Cargo.toml
│   └── src/main.rs
└── iced-demo/           # Reference integration (b): bin crate — iced shader widget + nacre-engine
    ├── Cargo.toml
    └── src/main.rs

.github/workflows/
└── ci.yml               # build + test + fmt + clippy on windows/linux/macos
```

**Structure Decision**: Single library crate at the workspace root (the deliverable),
with the two reference integrations as separate member crates so their heavy dependencies
(`winit`, `iced`) never appear in the library's graph. The placeholder `examples/demo.rs`
from the project scaffold is superseded by the `examples/raw-wgpu` member crate.

## Complexity Tracking

> No constitution violations — this section is intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
