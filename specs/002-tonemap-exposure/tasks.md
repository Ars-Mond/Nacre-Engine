---
description: "Task list for 002-tonemap-exposure"
---

# Tasks: Tone Mapping and Exposure

**Input**: Design documents from `specs/002-tonemap-exposure/`

**Prerequisites**: plan.md, spec.md (required); research.md, data-model.md, contracts/, quickstart.md (available)

**Tests**: Golden and behavioral tests **are included** — the spec mandates golden
verification per operator (FR-011), clip-reduction (SC-002), exposure scaling (SC-004),
exposure sanitization (FR-003), alpha passthrough (FR-002), and cross-OS parity (FR-012).

**Organization**: Tasks are grouped by user story (priority P1→P4). This feature is
**additive** to the feature-001 crate — no signatures change.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4; Setup/Foundational/Polish carry no story label
- Exact file paths are included in every task

## Path Conventions

Additive edits to the single library crate (`src/`), the golden tests (`tests/`), and the
two example member crates (`examples/`).

---

## Phase 1: Setup

**Purpose**: Scaffold the new module.

- [x] T001 Create `src/tonemap.rs` and declare `mod tonemap;` in `src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared tone-mapping plumbing — types, uniform, bind group, setter, and a
shader dispatch that is the identity for every operator so the default path stays
byte-identical to feature 001. No exposure multiply or operator math yet.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T002 [P] Define `ToneMapOperator` (enum: `None` default, `Reinhard`, `Aces`, `KhronosPbrNeutral`, with `u32` tag mapping 0..3) and `ToneMapping` (operator + `exposure: f32`, `Default` = None / 1.0) in `src/tonemap.rs`; re-export both from `src/lib.rs`
- [x] T003 [P] Add `ToneMapUniform { exposure: f32, operator: u32, _pad: [u32; 2] }` (bytemuck `Pod`) and operator tag constants to `src/uniforms.rs`
- [x] T004 Add a group-0 binding-3 uniform entry (FRAGMENT visibility) to the frame bind group layout in `src/pipeline.rs`
- [x] T005 In `src/shaders/pbr.wgsl`, declare the `ToneMap` uniform (`@group(0) @binding(3)`) and a `tone_map(c, op)` dispatch that returns `c` for every operator (placeholder), and apply it at the fragment output: `return vec4(tone_map(color, tm.operator), material.base_color.a)` (no exposure yet; None ⇒ byte-identical to feature 001)
- [x] T006 In `src/engine.rs`, add the tone-map uniform buffer + CPU `ToneMapping` state (default), bind it in `bind_group_frame`, add `pub fn set_tone_mapping(&mut self, ToneMapping)`, and upload the uniform in `prepare`
- [x] T007 Verify the default path: run the existing golden suite (`cargo test --test golden`) and confirm it passes with **zero** regenerated references (FR-008 / SC-001)

**Checkpoint**: engine builds; default output is byte-identical to feature 001.

---

## Phase 3: User Story 1 - Exposure control (Priority: P1)

**Goal**: A scalar exposure (default 1.0) applied before the tone curve scales scene
brightness without touching light intensities.

**Independent Test**: Render a scene at exposure 0.5 / 1.0 / 2.0 under None and confirm
mean brightness decreases / matches feature 001 / increases.

- [x] T008 [US1] Apply `× tm.exposure` before the curve at the fragment output in `src/shaders/pbr.wgsl` (`tone_map(color * tm.exposure, tm.operator)`)
- [x] T009 [US1] Sanitize exposure in `Engine::prepare`: replace non-finite or negative values with 1.0 and emit a `log::warn!` with context, in `src/engine.rs`
- [x] T010 [P] [US1] Add a behavioral test in `tests/golden.rs`: render a fixed scene at exposure 0.5 / 1.0 / 2.0 under None; assert mean brightness strictly increases, exposure 1.0 equals the feature-001 baseline, and exposure 0.0 yields a black lit object with alpha/background unaffected
- [x] T011 [P] [US1] Add an exposure-sanitization test in `tests/golden.rs` (FR-003): rendering with exposure `NaN`, `-1.0`, or `+Inf` yields a finite frame equal to the exposure-1.0 baseline (sanitized to 1.0, no NaN/black-screen); the sanitization path emits a `log::warn!`
- [x] T012 [P] [US1] Add a large-exposure test in `tests/golden.rs` (edge case): exposure `1e6` renders a finite, saturated frame with no NaN/Inf pixels, under None and at least one non-None operator

**Checkpoint**: exposure works and is robust; default (1.0) still byte-identical.

---

## Phase 4: User Story 2 - Tone-mapping operators (Priority: P2)

**Goal**: The four fixed operators (None, Reinhard, ACES Hill, Khronos PBR Neutral)
compress an overbright scene predictably and distinctly.

**Independent Test**: Render one overbright scene with each operator; None clips to flat
white, the others recover highlight gradation, and all four outputs are distinct.

- [x] T013 [US2] Implement the operator functions in `src/shaders/pbr.wgsl` (Reinhard `c/(1+c)`; ACES Hill with the ACEScg input/output matrices + `rrt_odt_fit`; Khronos PBR Neutral reference impl — see research §5), wire them into `tone_map`, each clamped to `[0, 1]`
- [x] T014 [US2] Add a deterministic overbright shared-scene helper for the operator goldens in `tests/golden.rs`
- [x] T015 [P] [US2] Add golden tests `golden_tonemap_{none,reinhard,aces,pbr_neutral}` for the overbright scene via `compare_or_regenerate`, and add the four scene names to the `golden_cross_platform_ssim` SCENES list, in `tests/golden.rs`
- [x] T016 [P] [US2] Add a clip-reduction assertion (SC-002) in `tests/golden.rs`: the count of fully-clipped `(255,255,255)` pixels under each non-None operator is lower than under None, and the four operator outputs are mutually distinct
- [x] T017 [P] [US2] Add an alpha-passthrough test in `tests/golden.rs` (FR-002): with a semi-transparent base color (alpha < 1) and a non-None operator, the RGB channels are tone-mapped but the readback alpha is unchanged versus None
- [x] T018 [US2] Generate and commit the Windows operator goldens via `UPDATE_GOLDEN=1 cargo test --test golden` into `tests/golden/windows/`

**Checkpoint**: all four operators render distinctly; goldens pass; alpha untouched.

---

## Phase 5: User Story 3 - Runtime switching (Priority: P3)

**Goal**: Changing operator and exposure between frames is safe and does not break the
lifecycle.

**Independent Test**: Switch operator + exposure every frame for many frames and confirm
each frame renders correctly with no errors.

- [x] T019 [P] [US3] Add a test in `tests/golden.rs` that drives one engine over many consecutive frames, changing operator and exposure each frame via `set_tone_mapping`, and asserts each frame renders without error and reflects the new settings

**Checkpoint**: per-frame switching is safe.

---

## Phase 6: User Story 4 - Existing integrations keep working (Priority: P4)

**Goal**: Feature-001 host code compiles and runs unmodified; both examples demonstrate
runtime switching.

**Independent Test**: Build/run both feature-001 reference integrations unchanged at their
cores; then exercise operator/exposure switching in each.

- [x] T020 [US4] Confirm source compatibility: no public signature/type/field of feature 001 changed and the full feature-001 golden suite passes unchanged; record the check in the PR (`cargo test --workspace`)
- [x] T021 [US4] Add runtime tone-mapping controls to `examples/raw-wgpu/src/main.rs`: `T` cycles the operator, `+`/`-` adjust exposure (calling `engine.set_tone_mapping` each frame); update the controls hint line
- [x] T022 [US4] Carry operator + exposure in the iced-demo primitive and switch them at runtime via `engine.set_tone_mapping` in `examples/iced-demo/src/main.rs`

**Checkpoint**: both integrations switch operator/exposure; feature-001 cores untouched.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Docs, gates, and cross-OS goldens.

- [x] T023 [P] Add rustdoc for `ToneMapOperator`, `ToneMapping`, and `Engine::set_tone_mapping` (pipeline order + defaults + sanitization note); ensure `cargo doc` is clean, in `src/tonemap.rs` and `src/engine.rs`
- [ ] T024 [P] Perform the manual Khronos glTF Sample Viewer comparison (FR-014: `T_NacreEngine.glb`, Khronos PBR Neutral, exposure 1.0, punctual lighting, IBL off) and record the result in `specs/002-tonemap-exposure/quickstart.md` — **deferred**: manual GUI step, needs a desktop session with the Khronos glTF Sample Viewer (not runnable in this headless environment)
- [x] T025 [P] Ensure `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` are green
- [x] T026 Dispatch the update-golden CI job to produce the Linux/macOS operator goldens and commit the uploaded PNG artifacts under `tests/golden/<os>/` (activates cross-OS SSIM for the new scenes) — **done**: Linux/macOS goldens added; cross-OS SSIM now active for all 9 scenes

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (T001)** blocks everything.
- **Foundational (T002–T007)** depends on Setup and blocks ALL user stories. Within it:
  T002/T003 are `[P]`; T004 needs the uniform shape (T003); T005 (shader) is independent
  of the Rust plumbing; T006 (engine) needs T002+T003+T004; T007 is the closing check.
- **User stories** depend on Foundational. Recommended order P1→P4. US1, US2, US3 all edit
  `src/shaders/pbr.wgsl` and/or `tests/golden.rs`, so run them sequentially to avoid file
  conflicts. US4 depends on US1+US2 existing (examples switch real operators/exposure).
- **Polish (T023–T026)** after the targeted stories complete.

### Key blocking points

- T005 (shader dispatch) + T006 (engine setter/upload) gate T008 (exposure) and T013 (operators).
- T014 (overbright scene) gates T015/T016 (operator goldens/assertions).
- T013 (operators) gates T017 (alpha test, needs a non-None operator) and T021/T022 (examples switch real operators).

---

## Parallel Opportunities

- **Foundational**: T002 and T003 in parallel; T005 (shader) in parallel with T002/T003.
- **US1**: the tests T010, T011, T012 can be written alongside the impl tasks T008/T009.
- **US2**: T015, T016, T017 in parallel once T013+T014 land.
- **Polish**: T023, T024, T025 in parallel.

### Parallel example: Foundational types

```text
Task: "Define ToneMapOperator + ToneMapping in src/tonemap.rs"   (T002)
Task: "Add ToneMapUniform in src/uniforms.rs"                    (T003)
Task: "Declare ToneMap uniform + identity dispatch in pbr.wgsl"  (T005)
```

---

## Implementation Strategy

### MVP first (US1)

1. Setup + Foundational (default byte-identical, plumbing in place).
2. US1: exposure control + its tests (scaling, sanitization, large-exposure).
3. **STOP and VALIDATE**: existing goldens unchanged; exposure scales brightness.

### Incremental delivery

US1 (exposure) → US2 (operators + goldens) → US3 (runtime switching) → US4
(compatibility + example UIs). Each keeps the default byte-identical and prior stories
green.

### Notes

- The default path (None, exposure 1.0) MUST keep the feature-001 golden suite passing —
  re-run it after shader edits (byte-identity is the design intent, the golden tolerance
  is the enforced gate).
- Keep edits to `src/shaders/pbr.wgsl`, `src/engine.rs`, and `tests/golden.rs` serialized
  across stories (shared files).
- Per-OS operator goldens: commit Windows now (T018); Linux/macOS via the update-golden
  job (T026). Cross-OS SSIM skips-with-warning until both exist.
