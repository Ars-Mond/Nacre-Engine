//! Headless render-to-texture tests. These validate the full pipeline (WGSL,
//! bind groups, depth, draw) without a window. They require a working wgpu
//! adapter (hardware or software); when none is available the tests skip.

use nacre_engine::glam::{Mat4, Vec3, Vec4};
use nacre_engine::wgpu;
use nacre_engine::{
    Camera, Engine, EngineConfig, Light, Material, MeshData, MeshHandle, Primitive, Scene,
    ToneMapOperator, ToneMapping, Viewport,
};

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Try to acquire a headless device/queue, preferring a real adapter and falling back
/// to a software adapter (WARP on Windows, lavapipe on Linux, Metal on macOS). Returns
/// `None` and logs the reason when no adapter is available, so a GPU test is skipped
/// (not failed) on a runner without any usable adapter.
fn headless() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = match request_adapter(&instance, false).await {
            Some(a) => a,
            None => match request_adapter(&instance, true).await {
                Some(a) => a,
                None => {
                    eprintln!(
                        "[skip] no wgpu adapter available (hardware or software); GPU test ignored"
                    );
                    return None;
                }
            },
        };
        match adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nacre-test-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::Off,
            })
            .await
        {
            Ok(device_queue) => Some(device_queue),
            Err(e) => {
                eprintln!("[skip] adapter found but request_device failed ({e}); GPU test ignored");
                None
            }
        }
    })
}

async fn request_adapter(instance: &wgpu::Instance, fallback: bool) -> Option<wgpu::Adapter> {
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: fallback,
            compatible_surface: None,
        })
        .await
        .ok()
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

// ---- Golden-image comparison (T042, FR-019) ----

const REGEN_ENV: &str = "UPDATE_GOLDEN";

/// Prefer a software adapter (WARP / lavapipe) so golden output is deterministic and
/// machine-independent; fall back to any adapter (e.g. hardware Metal on macOS, which
/// has no software backend).
fn headless_deterministic() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = match request_adapter(&instance, true).await {
            Some(a) => a,
            None => request_adapter(&instance, false).await?,
        };
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nacre-golden-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .ok()
    })
}

fn golden_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn golden_dir() -> std::path::PathBuf {
    golden_root().join(std::env::consts::OS)
}

/// Compare RGBA8 `pixels` (`dim`x`dim`) against the per-OS golden PNG `name` using the
/// FR-019 per-pixel tolerance, or (re)generate the golden when `NACRE_REGENERATE_GOLDENS`
/// is set. A missing golden skips with a logged reason rather than failing, so a fresh
/// platform stays green until its goldens are generated.
fn compare_or_regenerate(name: &str, pixels: &[u8], dim: u32) {
    let path = golden_dir().join(format!("{name}.png"));

    if std::env::var(REGEN_ENV).is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("create golden dir");
        image::RgbaImage::from_raw(dim, dim, pixels.to_vec())
            .expect("rgba buffer")
            .save(&path)
            .expect("save golden");
        eprintln!("[regen] wrote golden {}", path.display());
        return;
    }

    let Ok(golden) = image::open(&path) else {
        eprintln!(
            "[skip] golden missing for this OS: {} (set {REGEN_ENV}=1 to create)",
            path.display()
        );
        return;
    };
    let golden = golden.to_rgba8();
    assert_eq!(
        (golden.width(), golden.height()),
        (dim, dim),
        "golden size mismatch for {name}"
    );

    // FR-019: at least 99% of channel samples within +/-2, and none beyond +/-8.
    let golden = golden.as_raw();
    let mut within = 0usize;
    let mut max_diff = 0u8;
    for (&a, &b) in pixels.iter().zip(golden.iter()) {
        let d = a.abs_diff(b);
        max_diff = max_diff.max(d);
        within += usize::from(d <= 2);
    }
    let ratio = within as f64 / pixels.len() as f64;
    assert!(
        max_diff <= 8,
        "max channel diff {max_diff} exceeds 8 for golden {name}"
    );
    assert!(
        ratio >= 0.99,
        "only {:.2}% of channels within +/-2 for golden {name}",
        ratio * 100.0
    );
}

#[test]
fn golden_cube_directional() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let lights = [key_light()];
    // A fixed (non-animated) transform keeps the output deterministic.
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::from_rotation_y(0.6) * Mat4::from_rotation_x(0.3),
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
    compare_or_regenerate("cube_directional", &px, dim);
}

#[test]
fn golden_custom_mesh() {
    let Some((device, queue)) = headless_deterministic() else {
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
    compare_or_regenerate("custom_mesh", &px, dim);
}

#[test]
fn golden_material_metal() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let sphere = engine.builtin_mesh(&device, Primitive::Sphere);
    let lights = [Light::Directional {
        direction: Vec3::new(-0.3, -0.3, -1.0),
        color: Vec3::ONE,
        intensity: 3.0,
    }];
    let scene = Scene {
        camera: test_camera(),
        mesh: sphere,
        transform: Mat4::IDENTITY,
        material: Material {
            base_color: Vec4::new(0.9, 0.7, 0.3, 1.0),
            metallic: 1.0,
            roughness: 0.15,
            normal_map: None,
            occlusion_map: None,
        },
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
    compare_or_regenerate("material_metal", &px, dim);
}

#[test]
fn golden_multi_light() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let lights = [
        Light::Directional {
            direction: Vec3::new(-0.4, -0.4, -1.0),
            color: Vec3::new(1.0, 0.9, 0.8),
            intensity: 2.0,
        },
        Light::Point {
            position: Vec3::new(1.5, 1.0, 1.5),
            color: Vec3::new(0.2, 0.4, 1.0),
            intensity: 4.0,
            range: 8.0,
        },
        Light::Point {
            position: Vec3::new(-1.5, -0.5, 1.0),
            color: Vec3::new(1.0, 0.3, 0.2),
            intensity: 3.0,
            range: 8.0,
        },
    ];
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::from_rotation_y(0.6) * Mat4::from_rotation_x(0.3),
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
    compare_or_regenerate("multi_light", &px, dim);
}

// ---- glTF fixture scene (tests/fixtures/T_NacreEngine.glb) ----

/// Fixed test mapping from glTF light intensity to engine intensity: 1:1. The fixture
/// is authored with engine-scale values (point lights of 1.5), not photometric
/// exports, so the identity mapping is the documented, stable choice for this golden.
const GLTF_INTENSITY_SCALE: f32 = 1.0;

struct GltfScene {
    mesh: MeshData,
    transform: Mat4,
    base_color: Vec4,
    metallic: f32,
    roughness: f32,
    camera: Camera,
    lights: Vec<Light>,
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("T_NacreEngine.glb")
}

/// Load the first mesh primitive, the camera, and the punctual lights from the GLB.
/// The fixture MUST contain tangents (the engine requires them and does not generate
/// them); the camera aspect is overridden with `aspect` to match the render target.
fn load_gltf_fixture(aspect: f32) -> GltfScene {
    let path = fixture_path();
    let (doc, buffers, _images) = gltf::import(&path).expect("import fixture GLB");
    let gltf_scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .expect("fixture GLB has no scene");

    let mut mesh: Option<(MeshData, Mat4, Vec4, f32, f32)> = None;
    let mut cam: Option<(Mat4, f32, f32, Option<f32>)> = None;
    let mut lights: Vec<Light> = Vec::new();

    let mut stack: Vec<(gltf::Node, Mat4)> =
        gltf_scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());

        if let Some(m) = node.mesh() {
            if mesh.is_none() {
                let prim = m
                    .primitives()
                    .next()
                    .expect("fixture mesh has no primitives");
                let reader = prim.reader(|buffer| Some(&buffers[buffer.index()]));
                let positions: Vec<[f32; 3]> = reader
                    .read_positions()
                    .expect("fixture mesh has no positions")
                    .collect();
                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .expect("fixture mesh has no normals")
                    .collect();
                let uvs: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .expect("fixture mesh has no UVs (TEXCOORD_0)")
                    .into_f32()
                    .collect();
                let tangents: Vec<[f32; 4]> = reader
                    .read_tangents()
                    .expect(
                        "fixture mesh has no tangents — re-export the GLB with tangents; \
                         the engine requires them and does not generate them (FR-007)",
                    )
                    .collect();
                let indices: Vec<u32> = reader
                    .read_indices()
                    .expect("fixture mesh has no indices")
                    .into_u32()
                    .collect();
                let pbr = prim.material().pbr_metallic_roughness();
                mesh = Some((
                    MeshData {
                        positions,
                        normals,
                        uvs,
                        tangents,
                        indices,
                    },
                    world,
                    Vec4::from(pbr.base_color_factor()),
                    pbr.metallic_factor(),
                    pbr.roughness_factor(),
                ));
            } else {
                eprintln!(
                    "[warn] fixture has more than one mesh; using the first (single-mesh scope, FR-008)"
                );
            }
        }

        if let Some(c) = node.camera()
            && cam.is_none()
        {
            match c.projection() {
                gltf::camera::Projection::Perspective(p) => {
                    cam = Some((world, p.yfov(), p.znear(), p.zfar()));
                }
                gltf::camera::Projection::Orthographic(_) => {
                    panic!("fixture camera is orthographic; only perspective is supported")
                }
            }
        }

        if let Some(l) = node.light() {
            match l.kind() {
                gltf::khr_lights_punctual::Kind::Directional => {
                    lights.push(Light::Directional {
                        // A glTF directional light shines along its node's -Z axis.
                        direction: world.transform_vector3(Vec3::NEG_Z).normalize(),
                        color: Vec3::from(l.color()),
                        intensity: l.intensity() * GLTF_INTENSITY_SCALE,
                    });
                }
                gltf::khr_lights_punctual::Kind::Point => {
                    lights.push(Light::Point {
                        position: world.transform_point3(Vec3::ZERO),
                        color: Vec3::from(l.color()),
                        intensity: l.intensity() * GLTF_INTENSITY_SCALE,
                        range: l.range().unwrap_or(0.0),
                    });
                }
                gltf::khr_lights_punctual::Kind::Spot { .. } => {
                    eprintln!("[warn] fixture spot light skipped (unsupported light type)");
                }
            }
        }

        for child in node.children() {
            stack.push((child, world));
        }
    }

    let (mesh, transform, base_color, metallic, roughness) =
        mesh.expect("fixture GLB contains no mesh");
    let (cam_world, yfov, znear, zfar) = cam.expect("fixture GLB contains no camera");
    assert!(
        !lights.is_empty(),
        "fixture GLB has no supported KHR_lights_punctual lights (directional/point)"
    );
    eprintln!(
        "[fixture] {} verts / {} indices, base_color {:?}, metallic {}, roughness {}, {} light(s): {:?}",
        mesh.positions.len(),
        mesh.indices.len(),
        base_color,
        metallic,
        roughness,
        lights.len(),
        lights
    );

    GltfScene {
        mesh,
        transform,
        base_color,
        metallic,
        roughness,
        camera: Camera {
            view: cam_world.inverse(),
            // Aspect is overridden to match the render target, per the test contract.
            projection: Mat4::perspective_rh(yfov, aspect, znear, zfar.unwrap_or(100.0)),
            position: cam_world.w_axis.truncate(),
        },
        lights,
    }
}

#[test]
fn golden_gltf_fixture() {
    if !fixture_path().exists() {
        eprintln!(
            "[skip] glTF fixture missing: {} (place the GLB to enable this test)",
            fixture_path().display()
        );
        return;
    }
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let dim = 256;
    let loaded = load_gltf_fixture(1.0); // square target => aspect 1.0
    let mut engine = new_engine(&device, &queue);
    let mesh = engine
        .create_mesh(&device, &loaded.mesh)
        .expect("fixture mesh failed engine validation");
    let scene = Scene {
        camera: loaded.camera,
        mesh,
        transform: loaded.transform,
        material: Material {
            base_color: loaded.base_color,
            metallic: loaded.metallic,
            roughness: loaded.roughness,
            normal_map: None,
            occlusion_map: None,
        },
        lights: &loaded.lights,
    };
    let px = render_scene(
        &device,
        &queue,
        &mut engine,
        &scene,
        dim,
        full(dim),
        wgpu::Color::BLACK,
    );
    compare_or_regenerate("gltf_fixture", &px, dim);
}

/// Windowed mean SSIM (8x8 blocks) on luma, in `[0, 1]` — a standard-shaped structural
/// similarity measure for the cross-platform golden equivalence check (FR-019).
fn ssim(a: &[u8], b: &[u8], width: u32, height: u32) -> f64 {
    let luma = |px: &[u8]| -> Vec<f64> {
        px.chunks_exact(4)
            .map(|c| 0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64)
            .collect()
    };
    let la = luma(a);
    let lb = luma(b);
    let w = width as usize;
    let h = height as usize;
    const WIN: usize = 8;
    let c1 = (0.01 * 255.0_f64).powi(2);
    let c2 = (0.03 * 255.0_f64).powi(2);

    let mut total = 0.0;
    let mut count = 0usize;
    let mut y = 0;
    while y + WIN <= h {
        let mut x = 0;
        while x + WIN <= w {
            let n = (WIN * WIN) as f64;
            let (mut sa, mut sb) = (0.0, 0.0);
            for j in 0..WIN {
                for i in 0..WIN {
                    let idx = (y + j) * w + (x + i);
                    sa += la[idx];
                    sb += lb[idx];
                }
            }
            let (ma, mb) = (sa / n, sb / n);
            let (mut va, mut vb, mut cov) = (0.0, 0.0, 0.0);
            for j in 0..WIN {
                for i in 0..WIN {
                    let idx = (y + j) * w + (x + i);
                    let (da, db) = (la[idx] - ma, lb[idx] - mb);
                    va += da * da;
                    vb += db * db;
                    cov += da * db;
                }
            }
            va /= n - 1.0;
            vb /= n - 1.0;
            cov /= n - 1.0;
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            count += 1;
            x += WIN;
        }
        y += WIN;
    }
    if count == 0 {
        1.0
    } else {
        total / count as f64
    }
}

/// FR-019 cross-platform equivalence: any two per-OS goldens of the same scene must be
/// perceptually equivalent (SSIM >= 0.99). Scenes with fewer than two per-OS goldens are
/// skipped with a warning until those goldens are generated (via the update-golden CI job).
#[test]
fn golden_cross_platform_ssim() {
    const SCENES: [&str; 9] = [
        "gltf_fixture",
        "cube_directional",
        "custom_mesh",
        "material_metal",
        "multi_light",
        "tonemap_none",
        "tonemap_reinhard",
        "tonemap_aces",
        "tonemap_pbr_neutral",
    ];
    const OSES: [&str; 3] = ["windows", "linux", "macos"];

    let mut compared = 0usize;
    for scene in SCENES {
        let mut imgs: Vec<(&str, image::RgbaImage)> = Vec::new();
        for os in OSES {
            let path = golden_root().join(os).join(format!("{scene}.png"));
            match image::open(&path) {
                Ok(img) => imgs.push((os, img.to_rgba8())),
                Err(_) => eprintln!("[warn] no {os} golden for `{scene}`; cross-OS check skipped"),
            }
        }
        if imgs.len() < 2 {
            eprintln!(
                "[warn] `{scene}`: need >=2 per-OS goldens to compare, have {}",
                imgs.len()
            );
            continue;
        }
        for i in 0..imgs.len() {
            for j in (i + 1)..imgs.len() {
                let (na, a) = (imgs[i].0, &imgs[i].1);
                let (nb, b) = (imgs[j].0, &imgs[j].1);
                assert_eq!(
                    (a.width(), a.height()),
                    (b.width(), b.height()),
                    "golden size mismatch {na} vs {nb} for `{scene}`"
                );
                let s = ssim(a.as_raw(), b.as_raw(), a.width(), a.height());
                assert!(
                    s >= 0.99,
                    "SSIM {s:.4} between {na} and {nb} for `{scene}` is below 0.99"
                );
                compared += 1;
            }
        }
    }
    if compared == 0 {
        eprintln!("[warn] cross-platform SSIM: no scene has >=2 per-OS goldens yet");
    }
}

// ---- Tone mapping and exposure (feature 002) ----

fn mean_luminance(data: &[u8]) -> f64 {
    let sum: u64 = data
        .chunks_exact(4)
        .map(|c| c[0] as u64 + c[1] as u64 + c[2] as u64)
        .sum();
    sum as f64 / (data.len() / 4) as f64
}

fn count_clipped_white(data: &[u8]) -> usize {
    data.chunks_exact(4)
        .filter(|c| c[0] == 255 && c[1] == 255 && c[2] == 255)
        .count()
}

/// A bright, fixed scene that clips to flat white under the None operator — shared by
/// the operator goldens and the clip-reduction / large-exposure tests.
fn render_overbright(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    engine: &mut Engine,
    mesh: MeshHandle,
    operator: ToneMapOperator,
    exposure: f32,
    dim: u32,
) -> Vec<u8> {
    let lights = [
        Light::Directional {
            direction: Vec3::new(-0.2, -0.3, -1.0),
            color: Vec3::ONE,
            intensity: 12.0,
        },
        Light::Point {
            position: Vec3::new(1.2, 1.0, 1.5),
            color: Vec3::ONE,
            intensity: 18.0,
            range: 10.0,
        },
    ];
    let scene = Scene {
        camera: test_camera(),
        mesh,
        transform: Mat4::from_rotation_y(0.6) * Mat4::from_rotation_x(0.3),
        material: gray_material(),
        lights: &lights,
    };
    engine.set_tone_mapping(ToneMapping { operator, exposure });
    render_scene(
        device,
        queue,
        engine,
        &scene,
        dim,
        full(dim),
        wgpu::Color::BLACK,
    )
}

#[test]
fn tonemap_exposure_scales_brightness() {
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
    let dim = 256;
    let mut render_at = |exposure: f32| {
        engine.set_tone_mapping(ToneMapping {
            operator: ToneMapOperator::None,
            exposure,
        });
        render_scene(
            &device,
            &queue,
            &mut engine,
            &scene,
            dim,
            full(dim),
            wgpu::Color::BLACK,
        )
    };

    let low = mean_luminance(&render_at(0.5));
    let mid = mean_luminance(&render_at(1.0));
    let high = mean_luminance(&render_at(2.0));
    let black = mean_luminance(&render_at(0.0));

    assert!(
        low < mid && mid < high,
        "exposure must scale brightness: {low} < {mid} < {high}"
    );
    assert!(black < 1.0, "exposure 0.0 must be black, got mean {black}");
}

#[test]
fn tonemap_exposure_sanitized() {
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
    let dim = 256;
    let mut render_at = |exposure: f32| {
        engine.set_tone_mapping(ToneMapping {
            operator: ToneMapOperator::None,
            exposure,
        });
        render_scene(
            &device,
            &queue,
            &mut engine,
            &scene,
            dim,
            full(dim),
            wgpu::Color::BLACK,
        )
    };

    // Invalid exposure (NaN / negative / Inf) is sanitized to 1.0 (and logs a warning),
    // so the frame is byte-for-byte the exposure-1.0 baseline.
    let baseline = render_at(1.0);
    for bad in [f32::NAN, -1.0, f32::INFINITY] {
        assert_eq!(
            render_at(bad),
            baseline,
            "exposure {bad} must sanitize to 1.0"
        );
    }
}

#[test]
fn tonemap_large_exposure_is_finite() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let dim = 256;

    // None: a huge exposure saturates the lit object to white (no NaN garbage).
    let none = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::None,
        1e6,
        dim,
    );
    let c = pixel(&none, dim, dim / 2, dim / 2);
    assert_eq!(
        [c[0], c[1], c[2]],
        [255, 255, 255],
        "None at 1e6 should saturate to white, got {c:?}"
    );

    // ACES: a huge exposure stays bounded and renders a lit center (no NaN/Inf).
    let aces = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::Aces,
        1e6,
        dim,
    );
    assert!(
        luminance(pixel(&aces, dim, dim / 2, dim / 2)) > 0,
        "ACES at 1e6 should render a lit center"
    );
}

#[test]
fn tonemap_preserves_alpha() {
    let Some((device, queue)) = headless() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    // Strong light so a non-None operator visibly changes RGB versus None.
    let lights = [Light::Directional {
        direction: Vec3::new(-0.2, -0.3, -1.0),
        color: Vec3::ONE,
        intensity: 10.0,
    }];
    let scene = Scene {
        camera: test_camera(),
        mesh: cube,
        transform: Mat4::IDENTITY,
        material: Material {
            base_color: Vec4::new(0.8, 0.8, 0.8, 0.5), // semi-transparent
            metallic: 0.0,
            roughness: 0.5,
            normal_map: None,
            occlusion_map: None,
        },
        lights: &lights,
    };
    let dim = 256;
    let mut render_op = |operator: ToneMapOperator| {
        engine.set_tone_mapping(ToneMapping {
            operator,
            exposure: 1.0,
        });
        render_scene(
            &device,
            &queue,
            &mut engine,
            &scene,
            dim,
            full(dim),
            wgpu::Color::BLACK,
        )
    };

    let none = render_op(ToneMapOperator::None);
    let aces = render_op(ToneMapOperator::Aces);
    let (cx, cy) = (dim / 2, dim / 2);
    let p_none = pixel(&none, dim, cx, cy);
    let p_aces = pixel(&aces, dim, cx, cy);

    assert_eq!(
        p_none[3], p_aces[3],
        "alpha must be unchanged by the operator"
    );
    assert!(
        (120..=136).contains(&p_none[3]),
        "alpha should be ~128 (base 0.5), got {}",
        p_none[3]
    );
    assert_ne!(
        [p_none[0], p_none[1], p_none[2]],
        [p_aces[0], p_aces[1], p_aces[2]],
        "the operator should change RGB"
    );
}

#[test]
fn tonemap_runtime_switching() {
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
    let dim = 256;
    let ops = [
        ToneMapOperator::None,
        ToneMapOperator::Reinhard,
        ToneMapOperator::Aces,
        ToneMapOperator::KhronosPbrNeutral,
    ];
    for i in 0..24 {
        let operator = ops[i % ops.len()];
        let exposure = 0.5 + (i as f32) * 0.1;
        engine.set_tone_mapping(ToneMapping { operator, exposure });
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
            luminance(pixel(&px, dim, dim / 2, dim / 2)) > 0,
            "frame {i} ({operator:?}, exposure {exposure}) should render a lit center"
        );
    }
}

#[test]
fn tonemap_reduces_clipping() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let dim = 256;

    let none = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::None,
        1.0,
        dim,
    );
    let reinhard = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::Reinhard,
        1.0,
        dim,
    );
    let aces = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::Aces,
        1.0,
        dim,
    );
    let neutral = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::KhronosPbrNeutral,
        1.0,
        dim,
    );

    let clip_none = count_clipped_white(&none);
    assert!(clip_none > 0, "the overbright scene must clip under None");
    for (name, img) in [
        ("reinhard", &reinhard),
        ("aces", &aces),
        ("pbr_neutral", &neutral),
    ] {
        let clip = count_clipped_white(img);
        assert!(
            clip < clip_none,
            "{name} should clip fewer pixels than None ({clip} vs {clip_none})"
        );
    }

    // The four operator outputs are mutually distinct.
    let imgs = [&none, &reinhard, &aces, &neutral];
    for i in 0..imgs.len() {
        for j in (i + 1)..imgs.len() {
            assert_ne!(imgs[i], imgs[j], "operator outputs {i} and {j} must differ");
        }
    }
}

#[test]
fn golden_tonemap_none() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let px = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::None,
        1.0,
        256,
    );
    compare_or_regenerate("tonemap_none", &px, 256);
}

#[test]
fn golden_tonemap_reinhard() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let px = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::Reinhard,
        1.0,
        256,
    );
    compare_or_regenerate("tonemap_reinhard", &px, 256);
}

#[test]
fn golden_tonemap_aces() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let px = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::Aces,
        1.0,
        256,
    );
    compare_or_regenerate("tonemap_aces", &px, 256);
}

#[test]
fn golden_tonemap_pbr_neutral() {
    let Some((device, queue)) = headless_deterministic() else {
        return;
    };
    let mut engine = new_engine(&device, &queue);
    let cube = engine.builtin_mesh(&device, Primitive::Cube);
    let px = render_overbright(
        &device,
        &queue,
        &mut engine,
        cube,
        ToneMapOperator::KhronosPbrNeutral,
        1.0,
        256,
    );
    compare_or_regenerate("tonemap_pbr_neutral", &px, 256);
}
