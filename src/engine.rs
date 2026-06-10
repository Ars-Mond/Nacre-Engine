//! The engine: owns GPU resources derived from the host's device/queue and drives
//! the `update -> prepare -> render` lifecycle. It never creates a window, surface,
//! event loop, or device/queue (Principle III).

use crate::depth::{DEPTH_FORMAT, DepthBuffer};
use crate::error::EngineError;
use crate::mesh::{self, GpuMesh, MeshData, MeshHandle, Primitive};
use crate::pipeline::{self, Pipelines};
use crate::scene::{Light, Scene, Viewport};
use crate::uniforms::{
    CameraUniform, FLAG_HAS_NORMAL_MAP, FLAG_HAS_OCCLUSION_MAP, LIGHT_DIRECTIONAL, LIGHT_POINT,
    LightStd, LightsUniform, MAX_LIGHTS, MaterialUniform, ModelUniform,
};
use glam::Mat3;
use wgpu::util::DeviceExt;

/// Configuration declared once at engine creation.
pub struct EngineConfig {
    /// Color format of the host's target view (an sRGB format is recommended).
    pub target_format: wgpu::TextureFormat,
    /// MSAA sample count (1 or 4); must match the host color attachment.
    pub sample_count: u32,
}

/// The renderer. Construct with [`Engine::new`], drive with
/// [`Engine::update`] -> [`Engine::prepare`] -> [`Engine::render`].
pub struct Engine {
    sample_count: u32,

    pipeline: wgpu::RenderPipeline,
    bgl_material: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    placeholder_normal: wgpu::TextureView,
    placeholder_ao: wgpu::TextureView,

    camera_buf: wgpu::Buffer,
    model_buf: wgpu::Buffer,
    lights_buf: wgpu::Buffer,
    material_buf: wgpu::Buffer,
    bind_group_frame: wgpu::BindGroup,
    bind_group_material: wgpu::BindGroup,

    depth: Option<DepthBuffer>,
    meshes: Vec<GpuMesh>,

    // CPU-side frame state captured by `update`, uploaded by `prepare`.
    camera_uniform: CameraUniform,
    model_uniform: ModelUniform,
    lights_uniform: LightsUniform,
    material_uniform: MaterialUniform,
    current_mesh: Option<MeshHandle>,
    normal_view: Option<wgpu::TextureView>,
    ao_view: Option<wgpu::TextureView>,
    material_dirty: bool,
}

impl Engine {
    /// Create the engine from host-owned GPU handles. Builds the pipeline, bind
    /// group layouts, uniform buffers, sampler, and internal placeholder textures.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, config: &EngineConfig) -> Self {
        let Pipelines {
            pipeline,
            bgl_frame,
            bgl_material,
        } = pipeline::build(device, config.target_format, config.sample_count);

        let camera_buf = uniform_buffer(device, "nacre-camera", size_of::<CameraUniform>());
        let model_buf = uniform_buffer(device, "nacre-model", size_of::<ModelUniform>());
        let lights_buf = uniform_buffer(device, "nacre-lights", size_of::<LightsUniform>());
        let material_buf = uniform_buffer(device, "nacre-material", size_of::<MaterialUniform>());

        let bind_group_frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nacre-frame-bg"),
            layout: &bgl_frame,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: model_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights_buf.as_entire_binding(),
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nacre-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Internal placeholders: flat tangent-space normal and white AO.
        let placeholder_normal = solid_texture(device, queue, [128, 128, 255, 255], "nacre-normal");
        let placeholder_ao = solid_texture(device, queue, [255, 255, 255, 255], "nacre-ao");

        let bind_group_material = build_material_bind_group(
            device,
            &bgl_material,
            &material_buf,
            &placeholder_normal,
            &placeholder_ao,
            &sampler,
        );

        Self {
            sample_count: config.sample_count,
            pipeline,
            bgl_material,
            sampler,
            placeholder_normal,
            placeholder_ao,
            camera_buf,
            model_buf,
            lights_buf,
            material_buf,
            bind_group_frame,
            bind_group_material,
            depth: None,
            meshes: Vec::new(),
            camera_uniform: CameraUniform {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                camera_pos: [0.0; 4],
            },
            model_uniform: ModelUniform {
                model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                normal_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
            },
            lights_uniform: LightsUniform {
                lights: [LightStd::default(); MAX_LIGHTS],
                count: 0,
                _pad: [0; 3],
            },
            material_uniform: MaterialUniform {
                base_color: [1.0; 4],
                metallic: 0.0,
                roughness: 0.5,
                flags: 0,
                _pad: 0,
            },
            current_mesh: None,
            normal_view: None,
            ao_view: None,
            material_dirty: false,
        }
    }

    /// Upload custom geometry once; returns a handle referenced from [`Scene`].
    pub fn create_mesh(
        &mut self,
        device: &wgpu::Device,
        data: &MeshData,
    ) -> Result<MeshHandle, EngineError> {
        data.validate()?;
        Ok(self.upload_mesh(device, data))
    }

    /// Upload a built-in primitive (generated normals and tangents).
    pub fn builtin_mesh(&mut self, device: &wgpu::Device, primitive: Primitive) -> MeshHandle {
        let data = mesh::primitive_data(primitive);
        self.upload_mesh(device, &data)
    }

    fn upload_mesh(&mut self, device: &wgpu::Device, data: &MeshData) -> MeshHandle {
        let vertices = data.to_vertices();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nacre-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nacre-indices"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: data.indices.len() as u32,
        });
        handle
    }

    /// Phase 1 — record the scene to draw this frame (CPU-side).
    pub fn update(&mut self, scene: &Scene) {
        let view_proj = (scene.camera.projection * scene.camera.view).to_cols_array_2d();
        self.camera_uniform = CameraUniform {
            view_proj,
            camera_pos: [
                scene.camera.position.x,
                scene.camera.position.y,
                scene.camera.position.z,
                1.0,
            ],
        };

        let normal_mat = Mat3::from_mat4(scene.transform).inverse().transpose();
        self.model_uniform = ModelUniform {
            model: scene.transform.to_cols_array_2d(),
            normal_matrix: glam::Mat4::from_mat3(normal_mat).to_cols_array_2d(),
        };

        if scene.lights.len() > MAX_LIGHTS {
            log::warn!(
                "scene has {} lights but MAX_LIGHTS is {MAX_LIGHTS}; ignoring the extras",
                scene.lights.len()
            );
        }
        let count = scene.lights.len().min(MAX_LIGHTS);
        let mut lights = [LightStd::default(); MAX_LIGHTS];
        for (slot, light) in lights.iter_mut().zip(scene.lights.iter()).take(count) {
            *slot = light_to_std(light);
        }
        self.lights_uniform = LightsUniform {
            lights,
            count: count as u32,
            _pad: [0; 3],
        };

        let mut flags = 0u32;
        if scene.material.normal_map.is_some() {
            flags |= FLAG_HAS_NORMAL_MAP;
        }
        if scene.material.occlusion_map.is_some() {
            flags |= FLAG_HAS_OCCLUSION_MAP;
        }
        self.material_uniform = MaterialUniform {
            base_color: scene.material.base_color.to_array(),
            metallic: scene.material.metallic,
            roughness: scene.material.roughness,
            flags,
            _pad: 0,
        };

        self.normal_view = scene.material.normal_map.cloned();
        self.ao_view = scene.material.occlusion_map.cloned();
        self.material_dirty = true;
        self.current_mesh = Some(scene.mesh);
    }

    /// Phase 2 — upload uniforms, (re)create the depth buffer to match the color
    /// target, and refresh the material bind group. `target_size` is the size of
    /// the host's color attachment in pixels; the engine's depth attachment must
    /// match it (a wgpu requirement), so depth is sized to the target, not the
    /// (possibly smaller) draw viewport.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target_size: (u32, u32)) {
        let width = target_size.0.max(1);
        let height = target_size.1.max(1);
        let needs_depth = match &self.depth {
            Some(d) => d.width != width || d.height != height,
            None => true,
        };
        if needs_depth {
            self.depth = Some(DepthBuffer::new(device, width, height, self.sample_count));
        }

        queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&self.camera_uniform),
        );
        queue.write_buffer(&self.model_buf, 0, bytemuck::bytes_of(&self.model_uniform));
        queue.write_buffer(
            &self.lights_buf,
            0,
            bytemuck::bytes_of(&self.lights_uniform),
        );
        queue.write_buffer(
            &self.material_buf,
            0,
            bytemuck::bytes_of(&self.material_uniform),
        );

        if self.material_dirty {
            let normal = self
                .normal_view
                .as_ref()
                .unwrap_or(&self.placeholder_normal);
            let ao = self.ao_view.as_ref().unwrap_or(&self.placeholder_ao);
            self.bind_group_material = build_material_bind_group(
                device,
                &self.bgl_material,
                &self.material_buf,
                normal,
                ao,
                &self.sampler,
            );
            self.material_dirty = false;
        }
    }

    /// Phase 3 — encode the draw into a render pass the caller has begun, clipped
    /// to `viewport`. The pass MUST have `depth_view()` as its depth attachment.
    /// Modifies only pixels inside the viewport (no clear).
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>, viewport: Viewport) {
        if viewport.is_empty() {
            return;
        }
        let Some(handle) = self.current_mesh else {
            return;
        };
        let mesh = &self.meshes[handle.0];

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group_frame, &[]);
        pass.set_bind_group(1, &self.bind_group_material, &[]);
        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_viewport(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            viewport.min_depth,
            viewport.max_depth,
        );
        pass.set_scissor_rect(
            viewport.x as u32,
            viewport.y as u32,
            viewport.width.ceil() as u32,
            viewport.height.ceil() as u32,
        );
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }

    /// The depth format the engine uses; pass it when building the pass's depth attachment.
    pub fn depth_format(&self) -> wgpu::TextureFormat {
        DEPTH_FORMAT
    }

    /// The engine-owned depth view to attach when beginning the pass. Valid after `prepare()`.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self
            .depth
            .as_ref()
            .expect("prepare() must be called before depth_view()")
            .view
    }
}

fn light_to_std(light: &Light) -> LightStd {
    match *light {
        Light::Directional {
            direction,
            color,
            intensity,
        } => LightStd {
            position_or_direction: [direction.x, direction.y, direction.z, 0.0],
            color: [color.x, color.y, color.z, 0.0],
            kind: LIGHT_DIRECTIONAL,
            intensity,
            range: 0.0,
            _pad: 0.0,
        },
        Light::Point {
            position,
            color,
            intensity,
            range,
        } => LightStd {
            position_or_direction: [position.x, position.y, position.z, 1.0],
            color: [color.x, color.y, color.z, 0.0],
            kind: LIGHT_POINT,
            intensity,
            range,
            _pad: 0.0,
        },
    }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn build_material_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    material_buf: &wgpu::Buffer,
    normal: &wgpu::TextureView,
    ao: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("nacre-material-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(normal),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(ao),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn solid_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: [u8; 4],
    label: &str,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
