//! Tone-mapping operators and settings (feature 002).

/// Fixed tone-mapping operators. `None` is the default and reproduces feature 001
/// (linear lighting written straight into the sRGB target, no tonal compression).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneMapOperator {
    /// Passthrough — the feature-001 behavior (identity curve).
    #[default]
    None,
    /// Per-channel Reinhard `c / (1 + c)`.
    Reinhard,
    /// ACES filmic, Stephen Hill RRT+ODT fit (as in the Khronos glTF Sample Viewer).
    Aces,
    /// Khronos PBR Neutral tone mapper.
    KhronosPbrNeutral,
}

impl ToneMapOperator {
    /// The shader selector tag (must match `pbr.wgsl`).
    pub(crate) fn tag(self) -> u32 {
        match self {
            ToneMapOperator::None => 0,
            ToneMapOperator::Reinhard => 1,
            ToneMapOperator::Aces => 2,
            ToneMapOperator::KhronosPbrNeutral => 3,
        }
    }
}

/// Tone-mapping settings: an operator plus a linear exposure multiplier applied to the
/// lighting result before the tone curve.
///
/// The default — operator [`ToneMapOperator::None`], exposure `1.0` — is byte-identical
/// to feature 001 (the None curve is the identity and `× 1.0` is exact).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneMapping {
    /// The tone-mapping operator.
    pub operator: ToneMapOperator,
    /// Linear multiplier applied to the lighting result before the curve (default 1.0).
    /// Non-finite or negative values are sanitized to 1.0 with a logged warning; `0.0`
    /// is valid (black lit result).
    pub exposure: f32,
}

impl Default for ToneMapping {
    fn default() -> Self {
        Self {
            operator: ToneMapOperator::None,
            exposure: 1.0,
        }
    }
}
