//! GPU-side uniform structures. All are `#[repr(C)]` + `bytemuck::Pod` with
//! std140-friendly layouts (vec3 padded to 16 bytes).

use bytemuck::{Pod, Zeroable};

/// Maximum number of analytical lights packed into a frame.
pub const MAX_LIGHTS: usize = 8;

/// Light kind tags matching the WGSL shader.
pub(crate) const LIGHT_DIRECTIONAL: u32 = 0;
pub(crate) const LIGHT_POINT: u32 = 1;

/// Material flag bits matching the WGSL shader.
pub(crate) const FLAG_HAS_NORMAL_MAP: u32 = 1 << 0;
pub(crate) const FLAG_HAS_OCCLUSION_MAP: u32 = 1 << 1;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ModelUniform {
    pub model: [[f32; 4]; 4],
    /// Inverse-transpose of the model 3x3, stored in a mat4 for alignment.
    pub normal_matrix: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LightStd {
    /// Direction (directional) or world position (point), xyz; w unused.
    pub position_or_direction: [f32; 4],
    /// Linear RGB color, w unused.
    pub color: [f32; 4],
    pub kind: u32,
    pub intensity: f32,
    pub range: f32,
    pub _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LightsUniform {
    pub lights: [LightStd; MAX_LIGHTS],
    pub count: u32,
    pub _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct MaterialUniform {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub flags: u32,
    pub _pad: u32,
}

impl Default for LightStd {
    fn default() -> Self {
        Self {
            position_or_direction: [0.0; 4],
            color: [0.0; 4],
            kind: LIGHT_DIRECTIONAL,
            intensity: 0.0,
            range: 0.0,
            _pad: 0.0,
        }
    }
}
