//! Reference integration (b): iced shader widget.
//!
//! The engine's GPU state lives in iced's `shader::Storage` via a `shader::Pipeline`,
//! created once by iced. Each frame iced calls `Primitive::prepare` (→ `engine.prepare`)
//! and `Primitive::render`. Because the iced UI pass has no depth attachment, the
//! adapter begins its own render pass with the engine's depth view attached and the
//! scissor set to the widget's clip bounds, then calls `engine.render` — the exact same
//! engine API as the raw-wgpu integration (see
//! specs/001-render-pbr-mesh/contracts/integration.md). iced re-exports the matching
//! `wgpu` at `iced::widget::shader::wgpu`; the engine pins the same `wgpu = 27`, so the
//! types unify.
//!
//! Run with: `cargo run -p iced-demo`

use nacre_engine::Primitive as Shape;
use nacre_engine::glam::{Mat4, Vec3, Vec4};
use nacre_engine::wgpu;
use nacre_engine::{
    Camera, Engine, EngineConfig, Light, Material, MeshHandle, Scene, ToneMapOperator, ToneMapping,
    Viewport,
};

use iced::mouse;
use iced::widget::{button, column, row, shader, text};
use iced::{Element, Length, Rectangle};

/// The engine GPU state, created once by iced and cached in its `Storage`. All
/// `CubePrimitive` instances of this type share it.
struct EnginePipeline {
    engine: Engine,
    mesh: MeshHandle,
}

impl shader::Pipeline for EnginePipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut engine = Engine::new(
            device,
            queue,
            &EngineConfig {
                target_format: format,
                sample_count: 1,
            },
        );
        let mesh = engine.builtin_mesh(device, Shape::Cube);
        EnginePipeline { engine, mesh }
    }
}

/// The per-frame primitive: just the scene parameters. iced calls `prepare` then
/// `render`; the adapter begins its own pass with the engine's depth view attached.
#[derive(Debug, Clone, Copy)]
struct CubePrimitive {
    angle: f32,
    operator: ToneMapOperator,
    exposure: f32,
}

impl shader::Primitive for CubePrimitive {
    type Pipeline = EnginePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        // Depth must match the color target (the full iced surface), not the widget bounds.
        let target = viewport.physical_size();
        let aspect = (bounds.width / bounds.height).max(0.001);
        let eye = Vec3::new(0.0, 1.2, 3.5);
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
            camera: Camera {
                view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y),
                projection: Mat4::perspective_rh(60_f32.to_radians(), aspect, 0.1, 100.0),
                position: eye,
            },
            mesh: pipeline.mesh,
            transform: Mat4::from_rotation_y(self.angle) * Mat4::from_rotation_x(self.angle * 0.4),
            material: Material {
                base_color: Vec4::new(0.85, 0.4, 0.2, 1.0),
                metallic: 0.1,
                roughness: 0.35,
                normal_map: None,
                occlusion_map: None,
            },
            lights: &lights,
        };
        pipeline.engine.set_tone_mapping(ToneMapping {
            operator: self.operator,
            exposure: self.exposure,
        });
        pipeline.engine.update(&scene);
        pipeline
            .engine
            .prepare(device, queue, (target.width, target.height));
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nacre-iced-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                // Load (do not clear) so the iced UI already in the target is preserved (SC-002).
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: pipeline.engine.depth_view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let vp = Viewport::new(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
        );
        pipeline.engine.render(&mut pass, vp);
    }
}

/// The iced `shader::Program`: produces a `CubePrimitive` each frame.
#[derive(Debug)]
struct CubeProgram {
    angle: f32,
    operator: ToneMapOperator,
    exposure: f32,
}

impl<Message> shader::Program<Message> for CubeProgram {
    type State = ();
    type Primitive = CubePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        CubePrimitive {
            angle: self.angle,
            operator: self.operator,
            exposure: self.exposure,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    CycleOperator,
    ExposureUp,
    ExposureDown,
}

struct App {
    operator: ToneMapOperator,
    exposure: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            operator: ToneMapOperator::None,
            exposure: 1.0,
        }
    }
}

fn update(state: &mut App, message: Message) {
    match message {
        Message::CycleOperator => {
            state.operator = match state.operator {
                ToneMapOperator::None => ToneMapOperator::Reinhard,
                ToneMapOperator::Reinhard => ToneMapOperator::Aces,
                ToneMapOperator::Aces => ToneMapOperator::KhronosPbrNeutral,
                ToneMapOperator::KhronosPbrNeutral => ToneMapOperator::None,
            };
        }
        Message::ExposureUp => state.exposure = (state.exposure * 1.25).clamp(0.05, 64.0),
        Message::ExposureDown => state.exposure = (state.exposure * 0.8).clamp(0.05, 64.0),
    }
}

fn view(state: &App) -> Element<'_, Message> {
    let scene = shader(CubeProgram {
        angle: 0.6,
        operator: state.operator,
        exposure: state.exposure,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let controls = row(vec![
        button(text(format!("Operator: {:?}", state.operator)))
            .on_press(Message::CycleOperator)
            .into(),
        button(text("Exposure -"))
            .on_press(Message::ExposureDown)
            .into(),
        text(format!("exposure {:.2}", state.exposure)).into(),
        button(text("Exposure +"))
            .on_press(Message::ExposureUp)
            .into(),
    ])
    .spacing(12)
    .padding(8);

    column(vec![scene.into(), controls.into()])
        .height(Length::Fill)
        .into()
}

fn main() -> iced::Result {
    iced::application(App::default, update, view)
        .title("NacreEngine — iced shader widget")
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless() -> Option<(wgpu::Device, wgpu::Queue)> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = match instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
            {
                Ok(a) => a,
                Err(_) => instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        force_fallback_adapter: true,
                        compatible_surface: None,
                    })
                    .await
                    .ok()?,
            };
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("nacre-iced-test"),
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

    /// T038: the iced adapter drives the engine headlessly through `prepare` with no
    /// panic and no engine-side host branching — the same engine API as raw wgpu.
    #[test]
    fn iced_primitive_prepare_drives_engine() {
        let Some((device, queue)) = headless() else {
            return;
        };
        let mut pipeline = <EnginePipeline as shader::Pipeline>::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let primitive = CubePrimitive {
            angle: 0.3,
            operator: ToneMapOperator::Aces,
            exposure: 1.5,
        };
        let viewport = shader::Viewport::with_physical_size(iced::Size::new(256, 256), 1.0);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 256.0,
        };
        shader::Primitive::prepare(
            &primitive,
            &mut pipeline,
            &device,
            &queue,
            &bounds,
            &viewport,
        );
        // The depth view is available once prepare has run.
        let _ = pipeline.engine.depth_view();
    }
}
