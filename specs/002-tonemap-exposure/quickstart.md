# Quickstart & Validation: Tone Mapping and Exposure

**Feature**: `002-tonemap-exposure` | **Date**: 2026-06-12

Validation scenarios that prove the feature works end to end. Types are in
[data-model.md](./data-model.md); the additive API is in
[contracts/public-api.md](./contracts/public-api.md).

## Prerequisites

- Same as feature 001: Rust stable (edition 2024); a GPU adapter (hardware, or WARP /
  lavapipe software) to run examples and golden tests. No new dependencies.

## Scenario 1 — Default is unchanged (FR-008, SC-001)

Proves: with no tone-mapping call, output is byte-identical to feature 001.

```text
cargo test --test golden
```

**Expected**: every existing feature-001 golden test passes with **zero** references
regenerated. The new `tonemap_none` golden matches the existing lit references for the
same scene.

## Scenario 2 — Exposure scales brightness (US1, SC-004)

Proves: exposure is a pre-curve multiplier that does not touch light intensities.

The `golden`/behavioral tests render the shared scene at exposure 0.5 / 1.0 / 2.0 under
`None`. **Expected**: mean rendered brightness strictly increases with exposure; at
exposure 1.0 the output equals feature 001; at exposure 0.0 the lit object is black with
alpha and background unaffected.

## Scenario 3 — Operators recover highlights (US2, SC-002, SC-003)

Proves: the four operators compress an overbright scene predictably and differently.

```text
cargo test --test golden            # tonemap_{none,reinhard,aces,pbr_neutral}
```

**Expected**: under `None` the bright scene clips to large flat-white regions; under
Reinhard / ACES / PBR Neutral the count of fully-clipped (255,255,255) pixels is
measurably lower and highlight gradation is visible. The four operator goldens are
mutually distinct and each passes the per-OS tolerance and cross-OS SSIM ≥ 0.99.

## Scenario 4 — Runtime switching in the examples (US3, US4, FR-013)

Proves: operator and exposure change at runtime without breaking the lifecycle, and both
reference integrations still work unmodified at their cores.

```text
cargo run -p raw-wgpu      # cycle operator (T), adjust exposure (+/-)
cargo run -p iced-demo     # operator/exposure switchable in the widget
```

**Expected**: switching the operator/exposure every frame is smooth and error-free; the
rest of each host's code is unchanged from feature 001.

## Scenario 5 — Cross-platform parity (FR-012)

```text
cargo test --test golden   # includes golden_cross_platform_ssim over the new scenes
```

**Expected**: the new operator goldens are SSIM ≥ 0.99 across Windows/Linux/macOS (once
per-OS references exist, generated via the update-golden CI job; skipped-with-warning
until then, as for feature 001).

## Scenario 6 — Manual viewer comparison (FR-014, SC-007 — by eye, not automated)

Render `tests/fixtures/T_NacreEngine.glb` with `ToneMapOperator::KhronosPbrNeutral` and
exposure 1.0, and compare visually against the **Khronos glTF Sample Viewer** with
punctual lighting on and IBL off. **Expected**: the tonal look is comparable (highlight
rolloff and saturation), recorded as passed by a human reviewer. This criterion is
explicitly not automated.

## Quality gates (run before every PR)

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

To (re)generate the new operator goldens: `UPDATE_GOLDEN=1 cargo test --test golden`, or
dispatch the **update-golden** CI job for Linux/macOS references.

## Done-when (feature acceptance)

- [ ] Existing feature-001 goldens pass unchanged (zero regenerated).
- [ ] `tonemap_{none,reinhard,aces,pbr_neutral}` goldens pass per-OS + cross-OS SSIM.
- [ ] Exposure scaling and the overbright clip-reduction assertions pass.
- [ ] Both examples switch operator and exposure at runtime; feature-001 host code is
      untouched at its core.
- [ ] The manual Khronos viewer comparison (FR-014) is recorded as passed.
- [ ] `fmt`, `clippy`, and `test` gates are green on all three OSes.
