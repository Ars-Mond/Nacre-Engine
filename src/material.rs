//! Host-facing PBR material (metallic-roughness workflow).

use glam::Vec4;

/// A metallic-roughness PBR material. Maps are optional; when absent the engine
/// uses the scalar/default value. Texture maps are host-created GPU texture
/// views — the engine never loads or uploads texture content (FR-018).
pub struct Material<'a> {
    /// Linear RGBA base-color factor.
    pub base_color: Vec4,
    /// Metalness in `[0, 1]`.
    pub metallic: f32,
    /// Roughness in `[0, 1]`.
    pub roughness: f32,
    /// Optional tangent-space normal map (host-created view).
    pub normal_map: Option<&'a wgpu::TextureView>,
    /// Optional ambient-occlusion map (host-created view).
    pub occlusion_map: Option<&'a wgpu::TextureView>,
}

impl Default for Material<'_> {
    fn default() -> Self {
        Self {
            base_color: Vec4::ONE,
            metallic: 0.0,
            roughness: 0.5,
            normal_map: None,
            occlusion_map: None,
        }
    }
}
