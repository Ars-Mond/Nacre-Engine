# Implementation Plan: Tone Mapping and Exposure

**Branch**: `002-tonemap-exposure` | **Date**: 2026-06-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/002-tonemap-exposure/spec.md`

## Summary

Add an in-shader tone-mapping stage that maps the linear HDR lighting result into
displayable range, plus a scalar exposure applied before the tone curve. The pipeline
order is `lighting → × exposure → tone curve → sRGB target`; alpha is untouched. Four
fixed operators ship: **None** (passthrough), **Reinhard** (per-channel `c/(1+c)`),
**ACES** (Hill fit), and **Khronos PBR Neutral**. Settings are exposed through a new
**additive setter** on `Engine` with defaults (None, exposure 1.0), so the public
`update → prepare → render` contract is unchanged and existing host code compiles and
renders identically — the default path is a provable no-op, keeping feature-001 goldens
byte-valid.

## Technical Context

**Language/Version**: Rust, stable, edition 2024 (unchanged).

**Primary Dependencies**: unchanged — `wgpu = 27`, `glam`, `bytemuck`, `log`. No new
runtime or dev dependencies (operators are math in WGSL + a small uniform).

**Storage**: N/A.

**Testing**: `cargo test` with the existing headless golden harness; new golden scenes
(one shared scene × four operators) through the established pipeline (per-OS references,
FR-019/001 per-pixel tolerance, cross-OS SSIM ≥ 0.99, `UPDATE_GOLDEN`).

**Target Platform**: Windows (DX12), Linux (Vulkan), macOS (Metal), via wgpu.

**Project Type**: Single library crate + two example member crates + golden tests
(unchanged shape).

**Performance Goals**: Negligible — tone mapping is a handful of per-fragment ALU ops;
no extra pass, render target, or pipeline.

**Constraints**: Default output byte-identical to feature 001 (FR-008); public lifecycle
unchanged (FR-009, Principle V); runtime-switchable (FR-010); alpha untouched (FR-002);
identical-within-tolerance across the three OSes.

**Scale/Scope**: One fragment-shader post-stage, one new uniform (group 0, binding 3),
one new public settings type + setter, four operator functions. No new pass or target.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Gate | Status |
|-----------|------|--------|
| I. Pure Rust, zero native deps | No new dependencies; operators are WGSL math. | PASS |
| II. Cross-platform parity | New golden scenes per OS + cross-OS SSIM; CI matrix unchanged. | PASS |
| III. Library, not I/O owner | No change to ownership; still renders into the host target. | PASS |
| IV. Minimal explicit dependencies | Zero new deps; one small uniform + setter. | PASS |
| V. Integration first, API is a contract | `update → prepare → render` unchanged; tone mapping is an **additive** setter with defaults, so this is a **MINOR** (non-breaking) change — no major bump. Existing host code is untouched. | PASS |
| VI. Structured logging | Non-finite/negative exposure is sanitized with a `log::warn!` (no panic). | PASS |
| VII. Phase commit discipline | Each Spec Kit phase committed via `/speckit-git-commit`. | PASS |
| VIII. Safe code | No new `unwrap`/`unsafe`; shader-only math + plain uniform. | PASS |
| Tech constraints | Operators authored in WGSL only; English artifacts. | PASS |

No violations. Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/002-tonemap-exposure/
├── spec.md              # Clarification-complete
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── public-api.md     # The additive public API (unchanged lifecycle + new setter)
├── checklists/
│   └── requirements.md  # 19/19
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (affected files only — additive changes to the feature-001 crate)

```text
src/
├── lib.rs               # + re-export ToneMapping, ToneMapOperator
├── tonemap.rs           # NEW: ToneMapOperator (enum), ToneMapping (operator + exposure, Default)
├── uniforms.rs          # + ToneMapUniform { exposure, operator, _pad }
├── pipeline.rs          # + group-0 binding 3 (uniform, FRAGMENT) in the frame bind group layout
├── engine.rs            # + tonemap buffer/state, set_tone_mapping(), upload in prepare()
└── shaders/
    └── pbr.wgsl         # + ToneMap uniform (group 0, binding 3) + exposure & 4 operators in fs_main

tests/
├── golden.rs            # + one shared scene × {None, Reinhard, ACES, PBR Neutral} golden tests
└── golden/<os>/         # + tonemap_<operator>.png references

examples/raw-wgpu/src/main.rs   # + keys to cycle operator and adjust exposure at runtime
examples/iced-demo/src/main.rs  # + operator/exposure carried in the primitive, switchable
```

**Structure Decision**: Purely additive within the existing crate and examples — no new
crates, passes, or render targets. Tone mapping is computed in `fs_main` immediately
before the color is returned to the sRGB target; a new group-0 uniform carries the
operator + exposure. Settings reach the engine through `Engine::set_tone_mapping`, never
through `Scene`/`EngineConfig`/`prepare`/`render` signatures, preserving source
compatibility (FR-009).

## Complexity Tracking

> No constitution violations — this section is intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
