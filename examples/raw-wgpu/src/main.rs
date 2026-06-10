//! Reference integration (a): raw wgpu + winit.
//!
//! The host (this example) owns the window, event loop, surface, and the wgpu
//! `Device`/`Queue`. NacreEngine only renders into the host's frame: each frame the
//! host calls `engine.prepare(device, queue, target_size)` and then `engine.render`
//! inside a render pass it begins with the engine's depth view attached
//! (see specs/001-render-pbr-mesh/contracts/integration.md).
//!
//! Run with: `cargo run -p raw-wgpu`

use std::sync::Arc;
use std::time::Instant;

use nacre_engine::glam::{Mat4, Vec3, Vec4};
use nacre_engine::wgpu;
use nacre_engine::{
    Camera, Engine, EngineConfig, Light, Material, MeshData, MeshHandle, Primitive, Scene, Viewport,
};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Host-owned GPU + engine state.
struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    engine: Engine,
    /// Built-in cube, built-in sphere, and a custom mesh — cycled with Space.
    meshes: Vec<MeshHandle>,
    active: usize,
    start: Instant,
}

impl State {
    async fn new(window: Arc<Window>) -> State {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("no suitable GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nacre-raw-wgpu"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        // Prefer an sRGB target so the engine's linear output is encoded for us (FR-017).
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            // Latency 1 keeps dragging responsive (fewer frames queued behind vsync).
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let mut engine = Engine::new(
            &device,
            &queue,
            &EngineConfig {
                target_format: format,
                sample_count: 1,
            },
        );
        let cube = engine.builtin_mesh(&device, Primitive::Cube);
        let sphere = engine.builtin_mesh(&device, Primitive::Sphere);
        let custom = engine
            .create_mesh(&device, &custom_quad())
            .expect("valid custom mesh");

        State {
            window,
            surface,
            device,
            queue,
            config,
            engine,
            meshes: vec![cube, sphere, custom],
            active: 0,
            start: Instant::now(),
        }
    }

    fn cycle_mesh(&mut self) {
        self.active = (self.active + 1) % self.meshes.len();
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn render(&mut self) {
        let t = self.start.elapsed().as_secs_f32();
        let aspect = (self.config.width as f32 / self.config.height as f32).max(0.001);
        let eye = Vec3::new(0.0, 1.2, 3.5);
        let camera = Camera {
            view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
            projection: Mat4::perspective_rh(60_f32.to_radians(), aspect, 0.1, 100.0),
            position: eye,
        };
        let lights = [
            Light::Directional {
                direction: Vec3::new(-0.4, -0.7, -0.6),
                color: Vec3::new(1.0, 0.97, 0.92),
                intensity: 3.0,
            },
            Light::Point {
                position: Vec3::new(1.5, 1.0, 1.5),
                color: Vec3::new(0.3, 0.5, 1.0),
                intensity: 4.0,
                range: 8.0,
            },
        ];
        let scene = Scene {
            camera,
            mesh: self.meshes[self.active],
            transform: Mat4::from_rotation_y(t * 0.6) * Mat4::from_rotation_x(t * 0.25),
            material: Material {
                base_color: Vec4::new(0.85, 0.4, 0.2, 1.0),
                metallic: 0.1,
                roughness: 0.35,
                normal_map: None,
                occlusion_map: None,
            },
            lights: &lights,
        };

        self.engine.update(&scene);
        self.engine.prepare(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
        );

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(_) => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // The host owns the clear; the engine never clears (SC-002).
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.03,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.engine.depth_view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let viewport = Viewport::new(
                0.0,
                0.0,
                self.config.width as f32,
                self.config.height as f32,
            );
            self.engine.render(&mut pass, viewport);
        }
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        frame.present();
    }
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("NacreEngine — raw wgpu");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let state = pollster::block_on(State::new(window.clone()));
        window.request_redraw();
        self.state = Some(state);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Space) => state.cycle_mesh(),
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                _ => {}
            },
            WindowEvent::RedrawRequested => state.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Drive continuous redraws here rather than from RedrawRequested, so the
        // Windows modal move/resize loop keeps animating smoothly instead of stalling
        // on a self-scheduled redraw that blocks behind vsync.
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }
}

/// A custom indexed quad facing +Z, demonstrating `Engine::create_mesh`.
fn custom_quad() -> MeshData {
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

fn main() {
    println!("NacreEngine raw-wgpu demo — Space: cycle mesh (cube / sphere / custom), Esc: quit");
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run app");
}
