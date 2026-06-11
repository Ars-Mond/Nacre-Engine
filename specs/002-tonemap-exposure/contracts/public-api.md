# Contract: Public API (additive)

**Feature**: `002-tonemap-exposure` | **Date**: 2026-06-12

This feature is **additive and source-compatible** (Principle V, FR-009). The entire
feature-001 public API — `Engine::new/create_mesh/builtin_mesh/update/prepare/render`,
`depth_view`/`depth_format`, and the `Scene`/`Camera`/`Light`/`Material`/`Viewport`/
`EngineConfig` types — is **unchanged**. Only the items below are new.

## New types (re-exported from the crate root)

```rust
/// Fixed tone-mapping operators. `None` is the default and reproduces feature 001.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneMapOperator {
    #[default]
    None,
    Reinhard,
    Aces,
    KhronosPbrNeutral,
}

/// Tone-mapping settings: an operator plus a linear exposure multiplier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneMapping {
    pub operator: ToneMapOperator,
    /// Linear multiplier applied before the tone curve (default 1.0). Non-finite or
    /// negative values are sanitized to 1.0 with a logged warning; 0.0 is valid.
    pub exposure: f32,
}

impl Default for ToneMapping {
    fn default() -> Self {
        Self { operator: ToneMapOperator::None, exposure: 1.0 }
    }
}
```

## New method

```rust
impl Engine {
    /// Set the tone-mapping operator and exposure used by subsequent frames. May be
    /// called at any time, including between frames (the change takes effect on the next
    /// `prepare`). The default (set at `new`) is `ToneMapping::default()` — operator
    /// `None`, exposure 1.0 — which is byte-identical to feature 001.
    pub fn set_tone_mapping(&mut self, settings: ToneMapping);
}
```

## Contract guarantees

1. **Source compatibility**: no existing signature, type, or field changes; feature-001
   host code compiles and runs unmodified (FR-009, SC-006). This ships as a SemVer
   **minor** version.
2. **Default byte-identity**: an engine that never calls `set_tone_mapping` (operator
   `None`, exposure 1.0) produces output byte-identical to feature 001; existing golden
   references remain valid (FR-008, SC-001).
3. **Pipeline order**: `lighting → × exposure → tone curve → sRGB target`; alpha is never
   modified (FR-001, FR-002).
4. **Runtime-switchable**: changing the operator or exposure between frames is safe and
   needs no mesh/material/engine recreation (FR-010); the `update → prepare → render`
   lifecycle is unchanged.
5. **Exposure sanitization**: non-finite or negative exposure is replaced with 1.0 and a
   warning is logged; `0.0` is honored (black lit result) (FR-003).

## Lifecycle ordering (informative)

```text
new(device, queue, config)
create_mesh / builtin_mesh ...
engine.set_tone_mapping(ToneMapping { operator: Aces, exposure: 1.5 }) // optional, any frame
loop each frame:
    update(&scene)
    prepare(device, queue, target_size)   // uploads tone-map uniform too
    render(&mut pass, viewport)
```
