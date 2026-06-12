//! Reference integration (a): raw wgpu + winit, with rendering on a dedicated thread.
//!
//! The host (this example) owns the window, event loop, surface, and the wgpu
//! `Device`/`Queue`. NacreEngine only renders into the host's frame via
//! `engine.prepare(device, queue, target_size)` + `engine.render` inside a pass the
//! host begins with the engine's depth view (see
//! specs/001-render-pbr-mesh/contracts/integration.md).
//!
//! Rendering runs on a **separate thread** so it is decoupled from the OS modal
//! move/resize loop (on Windows that loop blocks the event-loop thread, which would
//! otherwise freeze a main-thread render). GPU setup (surface/device) happens on the
//! main thread — winit only exposes the raw window handle there — and the ready state
//! is moved into the render thread; the event-loop thread then only forwards
//! resize / cycle-mesh / exit messages over a channel — the reference pattern for
//! winit + wgpu.
//!
//! Run with: `cargo run -p raw-wgpu` (Space: cycle mesh, Esc: quit).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Instant;

use nacre_engine::glam::{Mat4, Vec3, Vec4};
use nacre_engine::wgpu;
use nacre_engine::{
    Camera, Engine, EngineConfig, Light, Material, MeshData, MeshHandle, Primitive, Scene,
    ToneMapOperator, ToneMapping, Viewport,
};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Messages from the event-loop thread to the render thread.
enum RenderMsg {
    Resize(PhysicalSize<u32>),
    CycleMesh,
    CycleToneMap,
    ScaleExposure(f32),
    Exit,
}

/// Host-owned GPU + engine state, lives on the render thread.
struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    engine: Engine,
    /// Built-in cube, built-in sphere, and a custom mesh — cycled with Space.
    meshes: Vec<MeshHandle>,
    active: usize,
    tonemap: ToneMapping,
    start: Instant,
}

impl State {
    async fn new(window: Arc<Window>) -> State {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        // The surface keeps the window alive (it owns the Arc), so State needs no
        // separate window field.
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
            surface,
            device,
            queue,
            config,
            engine,
            meshes: vec![cube, sphere, custom],
            active: 0,
            tonemap: ToneMapping::default(),
            start: Instant::now(),
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn cycle_mesh(&mut self) {
        self.active = (self.active + 1) % self.meshes.len();
    }

    fn cycle_tonemap(&mut self) {
        self.tonemap.operator = match self.tonemap.operator {
            ToneMapOperator::None => ToneMapOperator::Reinhard,
            ToneMapOperator::Reinhard => ToneMapOperator::Aces,
            ToneMapOperator::Aces => ToneMapOperator::KhronosPbrNeutral,
            ToneMapOperator::KhronosPbrNeutral => ToneMapOperator::None,
        };
        println!("tone-map operator: {:?}", self.tonemap.operator);
    }

    fn scale_exposure(&mut self, factor: f32) {
        self.tonemap.exposure = (self.tonemap.exposure * factor).clamp(0.05, 64.0);
        println!("exposure: {:.3}", self.tonemap.exposure);
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

        self.engine.set_tone_mapping(self.tonemap);
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
                    // The host owns the clear; the engine never clears (SC-002).
                    ops: wgpu::Operations {
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
        frame.present();
    }
}

/// The render loop: own the GPU state and draw continuously. `Fifo` present paces the
/// loop to vsync, so it stays at refresh rate without busy-spinning, independent of the
/// event-loop thread (and thus of the OS modal move/resize loop).
///
/// `State` is created on the main thread and moved in: winit only hands out the raw
/// window handle on the main thread, so `create_surface` panics with
/// `RawHandle(Unavailable)` if attempted here. All wgpu resources are `Send`, so
/// moving the constructed state across threads is fine.
fn render_loop(mut state: State, rx: Receiver<RenderMsg>) {
    loop {
        loop {
            match rx.try_recv() {
                Ok(RenderMsg::Resize(size)) => state.resize(size),
                Ok(RenderMsg::CycleMesh) => state.cycle_mesh(),
                Ok(RenderMsg::CycleToneMap) => state.cycle_tonemap(),
                Ok(RenderMsg::ScaleExposure(f)) => state.scale_exposure(f),
                Ok(RenderMsg::Exit) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }
        state.render();
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    sender: Option<Sender<RenderMsg>>,
    render_thread: Option<JoinHandle<()>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("NacreEngine — raw wgpu");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Create the surface/device on the MAIN thread (winit exposes the raw window
        // handle only here), then hand the ready state to the render thread.
        let state = pollster::block_on(State::new(window.clone()));

        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("nacre-render".into())
            .spawn(move || render_loop(state, rx))
            .expect("spawn render thread");

        self.window = Some(window);
        self.sender = Some(tx);
        self.render_thread = Some(handle);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let _ = sender.send(RenderMsg::Resize(size));
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Space) => {
                    let _ = sender.send(RenderMsg::CycleMesh);
                }
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                Key::Character(s) => match s.as_str() {
                    "t" | "T" => {
                        let _ = sender.send(RenderMsg::CycleToneMap);
                    }
                    "+" | "=" => {
                        let _ = sender.send(RenderMsg::ScaleExposure(1.25));
                    }
                    "-" | "_" => {
                        let _ = sender.send(RenderMsg::ScaleExposure(0.8));
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Stop the render thread and wait for it to finish (drops GPU resources cleanly).
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(RenderMsg::Exit);
        }
        if let Some(handle) = self.render_thread.take() {
            let _ = handle.join();
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
    println!(
        "NacreEngine raw-wgpu demo — Space: cycle mesh · T: tone-map operator · +/-: exposure · Esc: quit"
    );
    let event_loop = EventLoop::new().expect("create event loop");
    // The render thread drives frames; the main thread only waits for OS events.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run app");
}
