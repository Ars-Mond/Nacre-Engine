//! Headless render-to-texture tests. These validate the full pipeline (WGSL,
//! bind groups, depth, draw) without a window. They require a working wgpu
//! adapter (hardware or software); when none is available the tests skip.

use nacre_engine::glam::{Mat4, Vec3, Vec4};
use nacre_engine::wgpu;
use nacre_engine::{
    Camera, Engine, EngineConfig, Light, Material, MeshData, Primitive, Scene, Viewport,
};

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Try to acquire a headless device/queue. Returns `None` if no adapter exists.
fn headless() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nacre-test-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .ok()?;
        Some((device, queue))
    })
}

/// Render `scene` into a `dim`x`dim` offscreen sRGB texture cleared to `clear`,
/// drawing only inside `viewport`, and read back RGBA8. The host owns the clear;
/// the engine draws only inside the viewport (scissor).
fn render_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    engine: &mut Engine,
    scene: &Scene,
    dim: u32,
    viewport: Viewport,
    clear: wgpu::Color,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-target"),
        size: wgpu::Extent3d {
            width: dim,
            height: dim,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    engine.update(scene);
    engine.prepare(device, queue, (dim, dim));

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("test-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: engine.depth_view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        engine.render(&mut pass, viewport);
    }

    let bytes_per_row = dim * 4; // dim is a multiple of 64, so this is 256-aligned
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test-readback"),
        size: (bytes_per_row * dim) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(dim),
            },
        },
        wgpu::Extent3d {
            width: dim,
            height: dim,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll failed");
    rx.recv().expect("map channel").expect("map failed");
    let data = slice.get_mapped_range().to_vec();
    buffer.unmap();
    data
}

fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

fn luminance(p: [u8; 4]) -> u32 {
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

fn max_luminance(data: &[u8]) -> u32 {
    data.chunks_exact(4)
        .map(|c| luminance([c[0], c[1], c[2], c[3]]))
        .max()
        .unwrap_or(0)
}

fn full(dim: u32) -> Viewport {
    Viewport::new(0.0, 0.0, dim as f32, dim as f32)
}

fn test_camera() -> Camera {
    let position = Vec3::new(0.0, 0.0, 3.0);
    Camera {
        view: Mat4::look_at_rh(position, Vec3::ZERO, Vec3::Y),
        projection: Mat4::perspective_rh(45.0_f32.to_radians(), 1.0, 0.1, 100.0),
        position,
    }
}

fn gray_material() -> Material<'static> {
    Material {
        base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
        metallic: 0.0,
        roughness: 0.5,
        normal_map: None,
        occlusion_map: None,
    }
}

fn key_light() -> Light {
    Light::Directional {
        direction: Vec3::new(-0.3, -0.4, -1.0),
        color: Vec3::ONE,
        intensity: 3.0,
    }
}

fn new_engine(device: &wgpu::Device, queue: &wgpu::Queue) -> Engine {
    Engine::new(
        device,
        queue,
        &EngineConfig {
            target_format: TARGET_FORMAT,
            sample_count: 1,
        },
    )
}

/// A unit quad in the XY plane facing +Z (custom geometry for US2).
fn quad() -> MeshData {
    MeshData {
        positions: vec![
            [-0.7, -0.7, 0.0],
            [0.7, -0.7, 0.0],
            [0.7, 0.7, 0.0],
            [-0.7, 0.7, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        tangents: vec![[1.0, 0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

#[test]
fn directional_lit_cube_is_visible() {
    let Some((device, queue)) = headless() else {
        eprintln!("skipping: no wgpu adapter available");
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let lights = [key_light()];
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::IDENTITY,
        material: gray_material(),
        lights: &lights,
    };

    let dim = 256;
    let px = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        dim,
        full(dim),
        wgpu::Color::BLACK,
    );
    assert!(
        luminance(pixel(&px, dim, dim / 2, dim / 2)) > 60,
        "center should be lit"
    );
    assert!(
        luminance(pixel(&px, dim, 4, 4)) < 12,
        "corner should be background"
    );
}

#[test]
fn viewport_containment_leaves_outside_untouched() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let lights = [key_light()];
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::IDENTITY,
        material: gray_material(),
        lights: &lights,
    };

    // Clear the whole 256x256 target to red; render only into a centered sub-viewport.
    let dim = 256;
    let red = wgpu::Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    let sub = Viewport::new(64.0, 64.0, 128.0, 128.0);
    let px = render_scene(&device, &queue, &mut engine, &scene, dim, sub, red);

    let outside = pixel(&px, dim, 4, 4);
    assert!(
        outside[0] > 220 && outside[1] < 20 && outside[2] < 20,
        "outside should stay red, got {outside:?}"
    );
    let inside = pixel(&px, dim, 128, 128);
    assert!(
        inside[1] > 20,
        "inside should show the lit cube, got {inside:?}"
    );
}

#[test]
fn roughness_dims_specular_highlight() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let sphere = engine.builtin_mesh(&device, Primitive::Sphere);
    let lights = [Light::Directional {
        direction: Vec3::new(0.0, 0.0, -1.0),
        color: Vec3::ONE,
        intensity: 3.0,
    }];
    let dim = 256;

    let mut peak = |roughness: f32| {
        let scene = Scene {
            camera: test_camera(),
            mesh: sphere,
            transform: Mat4::IDENTITY,
            material: Material {
                base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
                metallic: 0.0,
                roughness,
                normal_map: None,
                occlusion_map: None,
            },
            lights: &lights,
        };
        max_luminance(&render_scene(
            &device,
            &queue,
            &mut engine,
            &scene,
            dim,
            full(dim),
            wgpu::Color::BLACK,
        ))
    };

    let smooth_peak = peak(0.05);
    let rough_peak = peak(0.9);
    assert!(
        smooth_peak > rough_peak,
        "smooth highlight ({smooth_peak}) should exceed rough ({rough_peak})"
    );
}

#[test]
fn empty_light_set_renders_without_error() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::IDENTITY,
        material: gray_material(),
        lights: &[],
    };

    let dim = 256;
    let px = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        dim,
        full(dim),
        wgpu::Color::BLACK,
    );
    assert!(
        luminance(pixel(&px, dim, dim / 2, dim / 2)) < 12,
        "unlit object should be black"
    );
}

#[test]
fn depth_buffer_recreated_on_target_resize() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let lights = [key_light()];
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::IDENTITY,
        material: gray_material(),
        lights: &lights,
    };

    // Render at 256, then at 128 (forces depth recreation), then it must still render.
    let _ = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        256,
        full(256),
        wgpu::Color::BLACK,
    );
    let small = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        128,
        full(128),
        wgpu::Color::BLACK,
    );
    assert!(
        luminance(pixel(&small, 128, 64, 64)) > 60,
        "cube should render after resize"
    );
}

#[test]
fn custom_mesh_renders() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let mesh = engine.create_mesh(&device, &quad()).expect("valid mesh");
    let lights = [Light::Directional {
        direction: Vec3::new(0.0, 0.0, -1.0),
        color: Vec3::ONE,
        intensity: 3.0,
    }];
    let scene = Scene {
        camera: test_camera(),
        mesh,
        transform: Mat4::IDENTITY,
        material: gray_material(),
        lights: &lights,
    };

    let dim = 256;
    let px = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        dim,
        full(dim),
        wgpu::Color::BLACK,
    );
    assert!(
        luminance(pixel(&px, dim, dim / 2, dim / 2)) > 60,
        "custom quad should be lit at center"
    );
}
