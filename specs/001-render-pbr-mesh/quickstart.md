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

**Expected**: a window opens showing a shaded, animated cube lit by a directional + point light,
with correct depth occlusion. The engine created no window/device — the example did. Press
**Space** to cycle cube → sphere → custom mesh; **Esc** quits. Resizing the window keeps the
object correctly rendered (depth buffer recreated, FR-014 / SC-006).

## Scenario 2 — Custom geometry (US2)

Proves: developer-supplied vertices (position/normal/uv/tangent) + indices render through the
same path as built-ins.

In the raw-wgpu example, press **Space** twice to reach the custom mesh — a `create_mesh` quad
uploaded at startup. **Expected**: the custom geometry renders through the same path as the
built-ins. Invalid mesh data (length mismatch / out-of-range index) returns `EngineError` from
`create_mesh` — covered by the `mesh::tests` unit tests — never a render-time crash.

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

The `viewport_containment_leaves_outside_untouched` golden test clears the target to red, renders
into a centered sub-viewport, and checks that every pixel outside the viewport stays red.
**Expected**: the test passes (`cargo test --test golden`).

## Golden-image tests (cross-platform parity, FR-019)

Headless render-to-texture comparisons; no window required.

```text
cargo test --test golden
```

**Expected**: each scene's readback matches its per-OS golden within the FR-019 tolerance — ≥99%
of channels within ±2/255, none beyond ±8 (SC-004). Goldens live under `tests/golden/<os>/`
(`cube_directional`, `custom_mesh`, `material_metal`, `multi_light`). The
`golden_cross_platform_ssim` test additionally asserts SSIM ≥ 0.99 between any two per-OS goldens
of a scene (skipping with a warning until a scene has goldens on ≥2 platforms).

To (re)generate goldens intentionally, run `UPDATE_GOLDEN=1 cargo test --test golden` and review
the diffs before committing. To produce Linux/macOS goldens, dispatch the **update-golden** CI
job (Actions → CI → Run workflow) and commit the uploaded PNG artifacts.

## Quality gates (run before every PR)

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs all of the above on Windows, Linux, and macOS (constitution Principle II); a failure on
any OS blocks merge.

## Done-when (feature acceptance)

- [x] Scenarios 1–6 behave as described on a local machine (verified on Windows).
- [ ] `cargo test --test golden` passes on all three OSes in CI (Windows goldens committed;
      Linux/macOS goldens generated via the update-golden job).
- [x] The library graph (`cargo tree`) is `wgpu` + `glam` + `bytemuck` + `log` plus only
      pure-Rust transitive `*-sys` crates that build with no system packages (Principle I as
      amended in constitution v1.1.0; research §10).
- [x] `fmt`, `clippy`, and `test` gates are green locally (and on Windows CI once dispatched).
