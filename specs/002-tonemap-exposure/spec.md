# Feature Specification: Tone Mapping and Exposure

**Feature Branch**: `002-tonemap-exposure`

**Created**: 2026-06-12

**Status**: Draft

**Input**: User description: "Tone mapping and exposure — map the HDR lighting result
into displayable range. The 'user' is the developer integrating the engine."

## Overview

Feature 001 (FR-017) deliberately wrote the linear lighting result straight into an
sRGB target with no tonal compression: bright scenes hard-clip to flat white, and the
image does not match reference viewers (Khronos glTF Sample Viewer). This feature pays
off that debt. The developer gains a scalar **exposure** control applied before the
tone curve and a fixed set of **tone-mapping operators** to compress HDR lighting into
the displayable range — without touching light intensities and without any change to
existing integrations (the defaults reproduce feature-001 output byte for byte).

The "user" throughout this specification is a **developer integrating the engine**.

## Clarifications

### Session 2026-06-12

- Q: Which Reinhard formulation should the operator use? → A: Simple per-channel
  `c / (1 + c)` — the classic real-time variant; cheapest, adds no public parameter.
  Bright channels desaturate slightly toward white; hue preservation in the operator
  set is covered by Khronos PBR Neutral. (FR-005)
- Q: Which ACES formulation should the operator use? → A: The Hill fit (RRT+ODT
  approximation with ACEScg input/output matrices), matching the Khronos glTF Sample
  Viewer so the FR-014 by-eye comparison also exercises ACES, not only PBR Neutral.
  (FR-006)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Exposure control (Priority: P1)

As a developer, I set an exposure value (a scalar multiplier, default 1.0) that is
applied to the lighting result BEFORE the tone curve, so I can control scene brightness
without editing every light's intensity.

**Why this priority**: Exposure is the smallest independently valuable slice: even with
the passthrough operator it gives integrators brightness control they do not have today,
and every operator builds on it (the pipeline order fixes exposure before the curve).

**Independent Test**: Render the same scene with exposure 0.5 / 1.0 / 2.0 under the
default (passthrough) operator and confirm mean brightness decreases / matches
feature 001 / increases accordingly.

**Acceptance Scenarios**:

1. **Given** a scene with default settings, **When** the developer changes only the
   exposure, **Then** the rendered brightness scales accordingly and no light intensity
   was modified.
2. **Given** exposure 1.0 and the default operator, **When** a frame is rendered,
   **Then** the output is identical to feature 001's output for the same scene.
3. **Given** an exposure of 0.0, **When** a frame is rendered, **Then** the lit result
   is black (background and alpha unaffected) and no error occurs.

---

### User Story 2 - Selecting a tone-mapping operator (Priority: P2)

As a developer, I choose a tone-mapping operator from a fixed set — **None**
(passthrough, the feature-001 behavior), **Reinhard**, **ACES (filmic)**, and
**Khronos PBR Neutral** — so bright scenes compress predictably into displayable range
and the result is comparable with reference viewers.

**Why this priority**: This is the core of the feature — it fixes the hard clipping
debt. It depends on the exposure stage (US1) being in place for the defined pipeline
order.

**Independent Test**: Render one deliberately overbright scene with each of the four
operators and confirm: None clips to flat white; each other operator recovers gradation
in the highlights; each operator's output is visibly distinct.

**Acceptance Scenarios**:

1. **Given** an overbright scene that clips to flat white under None, **When** rendered
   with Reinhard, ACES, or PBR Neutral, **Then** highlight gradation is visible (no
   large flat-white regions).
2. **Given** the operator set, **When** the developer selects any operator, **Then**
   only the tonal response changes — geometry, materials, lights, and alpha are
   untouched.
3. **Given** the default configuration (no operator chosen explicitly), **When** a
   frame is rendered, **Then** the operator is None and output is byte-identical to
   feature 001 — all existing golden references remain valid without regeneration.

---

### User Story 3 - Runtime switching (Priority: P3)

As a developer, I change the operator and exposure between frames at runtime, and this
does not break the `update → prepare → render` cycle.

**Why this priority**: Interactive tuning (sliders, hotkeys) is how integrators
actually pick values; it must be safe every frame, but it builds on US1+US2 existing.

**Independent Test**: Switch the operator and exposure every frame for many consecutive
frames and confirm rendering stays correct with no errors and no lifecycle changes.

**Acceptance Scenarios**:

1. **Given** a running render loop, **When** the operator or exposure changes between
   any two frames, **Then** the next frame reflects the new settings and the lifecycle
   contract is unchanged.
2. **Given** per-frame switching over an extended run, **When** frames render
   continuously, **Then** there are no errors, no leaks, and no progressive slowdown.

---

### User Story 4 - Existing integrations keep working (Priority: P4)

As a developer with an existing integration (raw wgpu or iced), I update the engine
without changing my code: the public `prepare`/`render` contract is unchanged
(Principle V) and all new settings have defaults.

**Why this priority**: The compatibility guarantee protects every existing integrator;
it is validated last because it spans the whole feature.

**Independent Test**: Build and run both feature-001 reference integrations unmodified
against the new engine version and confirm identical behavior; then extend both to
demonstrate runtime switching of operator and exposure.

**Acceptance Scenarios**:

1. **Given** feature-001 host code with no modifications, **When** it is compiled and
   run against the new engine version, **Then** it builds and renders identically.
2. **Given** both reference integrations, **When** the feature ships, **Then** each
   demonstrates runtime switching of the operator and exposure.

---

### Edge Cases

- **Exposure 0**: lit output is black; background pixels and alpha are unaffected; no
  error.
- **Very large exposure**: operators saturate gracefully (no NaN/Inf artifacts); None
  clips as feature 001 does.
- **Negative, NaN, or infinite exposure**: the value is sanitized to a safe value and a
  warning is logged (no crash, no GPU fault).
- **Operator changed every frame**: no resource churn, leaks, or stalls.
- **None with exposure ≠ 1.0**: exposure still applies (the curve is identity); values
  above the displayable range clip exactly as in feature 001.
- **Alpha with semi-transparent base color**: tone mapping changes RGB only; alpha
  passes through unchanged.

## Requirements *(mandatory)*

### Functional Requirements

**Pipeline & semantics**

- **FR-001**: The engine MUST apply tone mapping to the linear HDR lighting result
  before sRGB encoding, in this exact order: lighting → × exposure → tone curve →
  sRGB-encoded target.
- **FR-002**: The alpha channel MUST NOT be affected by exposure or the tone curve.
- **FR-003**: Exposure MUST be a scalar linear multiplier with default 1.0, applied
  before the tone curve for every operator (including None, whose curve is identity).
  Negative or non-finite exposure values MUST be sanitized to a safe value with a
  logged warning.

**Operators**

- **FR-004**: The engine MUST provide exactly this fixed operator set: **None**
  (passthrough), **Reinhard**, **ACES (filmic)**, and **Khronos PBR Neutral**. The
  default MUST be None.
- **FR-005**: The Reinhard operator MUST use the simple per-channel formulation
  `c / (1 + c)` applied independently to each RGB channel. No additional parameter
  (e.g. white point) is exposed.
- **FR-006**: The ACES (filmic) operator MUST use the Hill fit — the RRT+ODT
  approximation with ACEScg input/output matrices, as used by the Khronos glTF Sample
  Viewer — so output is directly comparable with that reference viewer.
- **FR-007**: The Khronos PBR Neutral operator MUST follow the published Khronos PBR
  Neutral specification.

**Compatibility (Principle V)**

- **FR-008**: With default settings (None, exposure 1.0) the output MUST be
  byte-identical to feature 001 for the same scene; all existing golden references MUST
  remain valid without regeneration.
- **FR-009**: The public `update → prepare → render` contract MUST NOT change; the new
  settings MUST be optional with defaults so existing host code compiles and behaves
  identically without modification.
- **FR-010**: The operator and exposure MUST be changeable between frames at runtime
  without breaking the lifecycle and without requiring the host to recreate meshes,
  materials, or the engine.

**Verification**

- **FR-011**: At least one common scene MUST be rendered with every operator as golden
  references through the established pipeline: per-OS references, the FR-019 (001)
  per-pixel tolerance, cross-OS SSIM ≥ 0.99, and regeneration via the UPDATE_GOLDEN
  flow.
- **FR-012**: Behavior MUST be identical within the established tolerance on Windows,
  Linux, and macOS (CI matrix; cross-OS SSIM).
- **FR-013**: Both reference integrations (raw wgpu and iced) MUST demonstrate runtime
  switching of the operator and exposure.
- **FR-014**: A documented manual check MUST exist: the `T_NacreEngine.glb` fixture
  rendered with Khronos PBR Neutral at exposure 1.0 is visually comparable to the
  Khronos glTF Sample Viewer (punctual lighting, IBL off). This is a human, by-eye
  criterion — explicitly not automated.

### Key Entities

- **Tone-mapping settings**: the operator selection (one of the four fixed operators)
  plus the exposure scalar; carries defaults (None, 1.0) and is part of the per-frame
  rendering inputs the developer controls.
- **Tone-mapping operator**: a named, fixed tonal response curve applied after
  exposure; not user-extensible in this feature.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With default settings, **100%** of the existing feature-001 golden tests
  pass unchanged — zero golden references regenerated.
- **SC-002**: For a reference overbright scene, the share of fully clipped (pure
  white) pixels under each non-None operator is **measurably lower** than under None,
  and highlight gradation is visible in the goldens.
- **SC-003**: One common scene × four operators yields **four distinct golden
  references**, each passing the per-OS tolerance and the cross-OS SSIM ≥ 0.99 check.
- **SC-004**: Raising exposure strictly increases mean rendered brightness and
  lowering it strictly decreases it (verified on at least three exposure values).
- **SC-005**: Switching operator and exposure every frame for a sustained run
  completes with **zero** errors and no observable degradation.
- **SC-006**: Feature-001 host code compiles and runs **unmodified** against the new
  version with identical output.
- **SC-007**: The documented manual viewer comparison (FR-014) has been performed and
  recorded as passed by a human reviewer.

## Assumptions

- **Exposure semantics**: a plain linear multiplier (EV-stop input is explicitly out of
  scope); it applies to the lit RGB result only, never to alpha or the host's
  background pixels.
- **Byte-identity scope**: the byte-identical guarantee (FR-008) applies to the default
  pair (None, exposure 1.0); None with a non-default exposure scales then clips, which
  is new — allowed — behavior.
- **Settings shape**: settings are per-frame inputs with defaults; their exact API
  placement is a planning detail, constrained only by FR-009/FR-010.
- **Operator set is closed**: custom or user-supplied curves are out of scope; the four
  operators are engine built-ins.
- **Existing goldens stay**: feature-001 golden references remain the None references;
  new goldens are added only for the other operators.

## Out of Scope *(explicitly deferred)*

- Auto-exposure / eye adaptation.
- Bloom and other post-processing, color grading / LUTs, white balance.
- HDR display output (scRGB / HDR10 surfaces).
- Image-based lighting (next feature).
- Exposure expressed in EV stops (linear multiplier only).
