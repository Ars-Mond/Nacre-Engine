# Quickstart & Validation: Render a Single PBR Mesh

**Feature**: `001-render-pbr-mesh` | **Date**: 2026-06-10

Runnable scenarios that prove the feature works end to end. Type details are in
[data-model.md](./data-model.md); the API and integration contracts are in
[contracts/](./contracts/).

## Prerequisites

- Rust stable toolchain (edition 2024), `cargo` only — no system packages to build the library
  (constitution Principle I).
- A working GPU adapter to **run** examples/tests: a hardware GPU, or a software adapter
  (lavapipe on Linux, WARP on Windows) for headless CI.
- Windowed examples additionally need a display server at runtime (X11/Wayland on Linux); this
  is an example requirement, not a library one.

## Scenario 1 — Raw-wgpu reference integration (US1, primary MVP)

Proves: host owns the device/window; engine renders a lit primitive into the host's frame
without owning any I/O.

```text
cargo run -p raw-wgpu
```

**Expected**: a window opens showing a shaded cube (or sphere) lit by a directional light, with
correct depth occlusion. The engine created no window/device — the example did. Resizing the
window keeps the object correctly rendered (depth buffer recreated, FR-014 / SC-006).

## Scenario 2 — Custom geometry (US2)

Proves: developer-supplied vertices (position/normal/uv/tangent) + indices render through the
same path as built-ins.

In the raw-wgpu example, switch the mesh from `engine.builtin_mesh(.., Primitive::Cube)` to
`engine.create_mesh(.., &custom_mesh_data)`. **Expected**: the custom geometry renders with
correct shading; an invalid mesh (length mismatch / out-of-range index) returns `EngineError`
at `create_mesh`, not a crash at render.

## Scenario 3 — Material control (US3)

Proves: metallic-roughness response and optional maps.

Render the primitive twice — rough dielectric (`metallic=0, roughness=0.9`) vs smooth metal
(`metallic=1, roughness=0.1`). **Expected** (SC-005): the metal shows a tight, base-color-tinted
highlight; the dielectric shows a broad, white-ish highlight. Supplying a `normal_map`
`TextureView` adds visible surface detail; omitting it falls back to the geometric normal with
no error (FR-010).

## Scenario 4 — Multiple lights (US4)

Proves: a directional light plus point lights each contribute (direct-only).

Provide `lights = [Directional{..}, Point{..}, Point{..}]`. **Expected** (SC-007): each light's
contribution is visible and correctly located; an empty light slice renders a black (unlit)
object without error.

## Scenario 5 — iced reference integration (US5)

Proves: the identical engine API renders inside an iced shader widget.

```text
cargo run -p iced-demo
```

**Expected** (SC-003): the same scene renders inside an iced window, drawn within the widget's
bounds (scissor = clip bounds), leaving the rest of the iced UI untouched. No engine code
differs from the raw-wgpu path.

## Scenario 6 — Viewport containment (SC-002)

Proves: the engine only writes inside the viewport.

The raw-wgpu example renders into a sub-rectangle of a pre-filled target. **Expected**: every
pixel outside the viewport is unchanged from its pre-fill value.

## Golden-image tests (cross-platform parity, FR-019)

Headless render-to-texture comparisons; no window required.

```text
cargo test --test golden
```

**Expected**: each scene's readback matches its per-platform golden within tolerance — ≥99% of
pixels within ±2/255 per channel, none beyond ±8 (SC-004). Goldens live under
`tests/golden/<platform>/`. To (re)generate goldens intentionally, run with the regeneration
env var documented in `tests/golden.rs` and review the diffs before committing.

## Quality gates (run before every PR)

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs all of the above on Windows, Linux, and macOS (constitution Principle II); a failure on
any OS blocks merge.

## Done-when (feature acceptance)

- [ ] Scenarios 1–6 behave as described on a local machine.
- [ ] `cargo test --test golden` passes on all three OSes in CI.
- [ ] The library graph (`cargo tree`) contains `wgpu` + `glam` + `bytemuck` and no `*-sys`
      crate (Principle I audit, research §10).
- [ ] `fmt`, `clippy`, and `test` gates are green on all three OSes.
