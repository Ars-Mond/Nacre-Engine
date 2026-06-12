# Phase 1 Data Model: Tone Mapping and Exposure

**Feature**: `002-tonemap-exposure` | **Date**: 2026-06-12

All additions are layered onto the feature-001 model; nothing existing changes shape.

## Host-facing types (new)

### ToneMapOperator

A closed enum naming the fixed operator set. `#[derive(Default)]` with `None` as default.

| Variant | Tag (u32) | Curve |
|---------|-----------|-------|
| `None` (default) | 0 | identity (passthrough — feature-001 behavior) |
| `Reinhard` | 1 | per-channel `c / (1 + c)` (FR-005) |
| `Aces` | 2 | Hill RRT+ODT fit, ACEScg matrices (FR-006) |
| `KhronosPbrNeutral` | 3 | Khronos PBR Neutral reference (FR-007) |

The numeric tags match the WGSL `operator` selector (research §5).

### ToneMapping

The settings the developer controls, with defaults that reproduce feature 001.

| Field | Type | Default | Notes / Validation |
|-------|------|---------|--------------------|
| `operator` | `ToneMapOperator` | `None` | One of the four fixed operators. |
| `exposure` | `f32` | `1.0` | Linear multiplier applied **before** the curve. Non-finite or negative is sanitized to `1.0` with a `log::warn!` at upload (research §6); `0.0` is valid. |

`Default for ToneMapping` = `{ operator: None, exposure: 1.0 }`.

## GPU-side structure (new)

### ToneMapUniform (bytemuck `Pod`)

```text
exposure: f32     // sanitized
operator: u32     // ToneMapOperator tag
_pad:     [u32;2] // -> 16-byte size
```

Bound at `@group(0) @binding(3)` (fragment visibility), alongside the per-frame
camera/model/lights uniforms.

## Bind group change

| Group | Binding | Resource | Status |
|-------|---------|----------|--------|
| 0 | 0 | `CameraUniform` | unchanged |
| 0 | 1 | `ModelUniform` | unchanged |
| 0 | 2 | `LightsUniform` | unchanged |
| 0 | **3** | **`ToneMapUniform`** | **new (fragment)** |
| 1 | 0..3 | material uniform + normal/AO textures + sampler | unchanged |

## Shader pipeline order (FR-001)

In `fs_main`, after the lighting loop produces linear `color`:

```text
exposed = color * tonemap.exposure        // FR-003 (× exposure before the curve)
mapped  = apply_operator(exposed, op)     // FR-004 operator; None = identity
return vec4(mapped, material.base_color.a) // alpha untouched (FR-002); sRGB target encodes
```

With `op = None` and `exposure = 1.0`, `mapped == color` → byte-identical to feature 001
(FR-008).

## Engine state & lifecycle (additive)

- `Engine` gains: a `ToneMapUniform` buffer (created in `new`, bound in the frame bind
  group) and a CPU-side current `ToneMapping` (default), plus a dirty flag.
- `Engine::set_tone_mapping(&mut self, ToneMapping)` records the new settings (callable any
  frame; FR-010). Does **not** touch `update`/`prepare`/`render` signatures (FR-009).
- `prepare` sanitizes exposure and writes the `ToneMapUniform` (one extra `write_buffer`).
- `update`/`render` are unchanged; `Scene`, `EngineConfig`, `Material`, `Camera`, `Light`,
  `Viewport` are unchanged.
