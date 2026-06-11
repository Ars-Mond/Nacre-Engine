# Phase 0 Research: Tone Mapping and Exposure

**Feature**: `002-tonemap-exposure` | **Date**: 2026-06-12

The spec is clarification-complete (Reinhard = per-channel `c/(1+c)`; ACES = Hill fit).
This records the remaining technical decisions, with rationale and rejected alternatives.

## 1. API shape — additive setter, not a `Scene`/`EngineConfig` field

- **Decision**: Expose settings through `Engine::set_tone_mapping(ToneMapping)` (engine
  holds the current value; default `ToneMapping { operator: None, exposure: 1.0 }`).
- **Rationale**: FR-009 requires existing host code to compile unmodified. Rust struct
  literals must name every field, so adding a field to `Scene` or `EngineConfig` would
  break every existing `Scene { .. }` / `EngineConfig { .. }` literal — a source-breaking
  change. A new setter is purely additive: hosts that never call it get the default
  (None, 1.0). This also satisfies FR-010 (runtime-switchable: call it any frame before
  `prepare`).
- **Alternatives considered**: a `Scene.tone_mapping` field (rejected — breaks literals,
  violates FR-009); a `#[non_exhaustive]` `Scene` + `..default()` (rejected — `Scene` has
  borrowed fields and existing literals still must list all fields; no compatibility win).

## 2. Pipeline placement — in `fs_main`, no extra pass or target

- **Decision**: Apply `× exposure` then the tone curve at the end of `fs_main`, just
  before returning the color to the sRGB target. No separate HDR offscreen texture and no
  second pass.
- **Rationale**: The engine is a single-pass forward renderer that writes the final color
  per fragment; the operators (FR-004) are all per-pixel functions, so they fit inline
  with zero extra GPU resources, matching the "no new pass/target" scope and Principle IV.
  A separate HDR target would only be needed for screen-space post-effects (bloom), which
  are explicitly out of scope.
- **Alternatives considered**: render to an HDR `Rgba16Float` offscreen, then a fullscreen
  tone-map pass to the sRGB target (rejected — needs the engine to own an intermediate
  target and a second pipeline/pass; unnecessary for per-pixel operators and heavier).

## 3. Uniform placement — group 0, binding 3

- **Decision**: A new `ToneMapUniform { exposure: f32, operator: u32, _pad: [u32; 2] }`
  bound at `@group(0) @binding(3)` (fragment visibility), alongside camera/model/lights
  (the per-frame group). Engine adds one uniform buffer and one bind-group entry.
- **Rationale**: Tone mapping is per-frame state like the camera and lights, so it belongs
  in group 0. Binding 3 is the next free slot; the material group (1) is unrelated.
- **Alternatives considered**: a push constant (rejected — not universally available
  across wgpu backends/limits; a uniform is the portable baseline); folding into
  `CameraUniform` (rejected — muddies an unrelated struct and its `_pad`).

## 4. Default path is a provable no-op (FR-008 / SC-001)

- **Decision**: With `operator = None` and `exposure = 1.0`, `fs_main` computes
  `tonemap(color * 1.0, None)` where `None` returns its input unchanged → output equals
  feature 001's `color`. Existing goldens stay valid without regeneration.
- **Rationale**: `× 1.0` is exact in IEEE-754 and the None branch is the identity, so the
  result is byte-identical, not merely within tolerance. (Even if a backend's shader
  recompilation perturbed a low bit, the golden per-pixel ε of ±2 would still pass — but
  the design target is byte-identity.)
- **Verification**: re-run the existing golden suite after the shader change; it must pass
  with zero regenerated references.

## 5. Operator formulations (WGSL)

All operate on the linear RGB result *after* `× exposure`; alpha is carried through
untouched (FR-002). Each clamps its output into `[0, 1]` for the sRGB `Unorm` target.

- **None** (`operator = 0`, default): `return c;`
- **Reinhard** (`1`): per channel `c / (1 + c)` (clarified FR-005).
- **ACES** (`2`): the Stephen Hill RRT+ODT fit used by the Khronos glTF Sample Viewer
  (clarified FR-006):
  - input matrix (sRGB/linear → ACEScg), rows:
    `(0.59719, 0.35458, 0.04823)`, `(0.07600, 0.90834, 0.01566)`,
    `(0.02840, 0.13383, 0.83777)`;
  - `rrt_odt_fit(v) = (v*(v + 0.0245786) - 0.000090537) / (v*(0.983729*v + 0.4329510) + 0.238081)`;
  - output matrix (ACEScg → sRGB/linear), rows:
    `( 1.60475, -0.53108, -0.07367)`, `(-0.10208, 1.10813, -0.00605)`,
    `(-0.00327, -0.07276, 1.07602)`;
  - pipeline: `out = clamp(M_out * rrt_odt_fit(M_in * c), 0, 1)`.
- **Khronos PBR Neutral** (`3`): the published reference implementation:
  ```
  startCompression = 0.8 - 0.04
  desaturation     = 0.15
  x      = min(c.r, c.g, c.b)
  offset = (x < 0.08) ? x - 6.25*x*x : 0.04
  c      = c - offset
  peak   = max(c.r, c.g, c.b)
  if peak < startCompression: return c
  d       = 1 - startCompression
  newPeak = 1 - d*d / (peak + d - startCompression)
  c       = c * (newPeak / peak)
  g       = 1 - 1 / (desaturation*(peak - newPeak) + 1)
  return mix(c, vec3(newPeak), g)
  ```
- **Rationale**: Reinhard/ACES are the clarified choices; ACES Hill + PBR Neutral both
  match the Khronos viewer, making the FR-014 by-eye comparison meaningful for both.
- **Alternatives considered**: Narkowicz ACES and luminance Reinhard — rejected in the
  clarification session.

## 6. Exposure sanitization

- **Decision**: On upload, if `exposure` is non-finite (NaN/Inf) or negative, replace it
  with `1.0` and emit `log::warn!` once with context; the GPU only ever sees a finite,
  non-negative multiplier. Exposure `0.0` is valid (black lit result).
- **Rationale**: Principle VI forbids silently swallowing bad input and forbids panics for
  recoverable conditions; a NaN reaching the shader would poison the whole frame.
- **Alternatives considered**: clamp to a tiny epsilon (rejected — `0.0` is a legitimate,
  documented value, edge case in the spec); panic (rejected — Principle VIII).

## 7. Golden scenes for the operators (FR-011 / SC-002 / SC-003)

- **Decision**: Add one shared, deliberately bright deterministic scene rendered with each
  of the four operators → `tonemap_none`, `tonemap_reinhard`, `tonemap_aces`,
  `tonemap_pbr_neutral`. Each goes through the established golden pipeline (per-OS
  reference, per-pixel ε, cross-OS SSIM, `UPDATE_GOLDEN`). A behavioral assertion checks
  SC-002: the count of fully-clipped (255,255,255) pixels under each non-None operator is
  lower than under None. The four operator goldens must be mutually distinct.
- **Rationale**: Reuses the feature-001 golden machinery wholesale (Principle II), adds
  only data. The shared bright scene makes operator differences visible.
- **Alternatives considered**: reuse existing dim scenes (rejected — they barely clip, so
  operators would look identical and SC-002 could not be shown).

## 8. Example UI for runtime switching (FR-013)

- **Decision**: raw-wgpu cycles the operator on a key (e.g. `T`) and nudges exposure on
  `+`/`-`; iced-demo carries operator + exposure in its `Primitive` and exposes the same
  switching. Both call `engine.set_tone_mapping(..)` each frame before `prepare`.
- **Rationale**: Demonstrates FR-010/FR-013 with the runtime path the spec requires.

## Open Risks

- **Cross-backend ACES/PBR-Neutral parity**: rational/exp-free math should match closely
  across WARP/lavapipe/Metal; per-OS goldens + the SSIM ≥ 0.99 cross-check absorb minor
  float differences, with `UPDATE_GOLDEN` for regeneration if a backend drifts.

## Sources

- Stephen Hill, ACES filmic fit (`baking-lab` / used by the Khronos glTF Sample Viewer).
- Khronos PBR Neutral tone mapper — `KhronosGroup/ToneMapping` reference implementation.
