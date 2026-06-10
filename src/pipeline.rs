//! Render pipeline and bind group layouts (built once at engine creation).

use crate::depth::DEPTH_FORMAT;
use crate::mesh::Vertex;

pub(crate) struct Pipelines {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl_frame: wgpu::BindGroupLayout,
    pub bgl_material: wgpu::BindGroupLayout,
}

pub(crate) fn build(
    device: &wgpu::Device,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
) -> Pipelines {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nacre-pbr-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pbr.wgsl").into()),
    });

    let uniform = |binding: u32, visibility: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };

    let bgl_frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nacre-frame-bgl"),
        entries: &[
            uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            uniform(1, wgpu::ShaderStages::VERTEX_FRAGMENT),
            uniform(2, wgpu::ShaderStages::FRAGMENT),
        ],
    });

    let bgl_material = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nacre-material-bgl"),
        entries: &[
            uniform(0, wgpu::ShaderStages::FRAGMENT),
            texture(1),
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nacre-pipeline-layout"),
        bind_group_layouts: &[&bgl_frame, &bgl_material],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nacre-pbr-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Vertex::layout()],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    });

    Pipelines {
        pipeline,
        bgl_frame,
        bgl_material,
    }
}
