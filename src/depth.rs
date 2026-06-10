//! Engine-owned depth buffer, sized to the viewport and recreated on size change.

/// Depth format used by the engine (universally supported across wgpu backends).
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub(crate) struct DepthBuffer {
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl DepthBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, sample_count: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nacre-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            view,
            width,
            height,
        }
    }
}
