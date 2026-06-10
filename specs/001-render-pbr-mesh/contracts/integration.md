# Contract: Host Integration

**Feature**: `001-render-pbr-mesh` | **Date**: 2026-06-10

How a host wires the engine in. Both reference integrations use the *same* public API
(FR-005); they differ only in who begins the render pass. The key shared rule: **whoever
begins the pass attaches the engine's depth view** (FR-016).

## Shared rules

- The host owns the window, event loop, surface, `Device`, and `Queue`.
- The host (or its adapter) begins the render pass with:
  - **color attachment** = the host's target view, format and `sample_count` matching
    `EngineConfig`, `ops.load = LoadOp::Load` (the engine never clears — SC-002);
  - **depth attachment** = `engine.depth_view()`, format `engine.depth_format()`.
- The engine's `wgpu` must be the *same crate instance* as the host's (research §1). For iced,
  import wgpu through `iced::widget::shader::wgpu`.

## Reference integration (a): raw wgpu + winit

The host begins the pass and attaches the engine depth view directly.

```text
// setup
let (device, queue) = /* host-created from its own Adapter */;
let mut engine = Engine::new(&device, &queue, &EngineConfig { target_format, sample_count });
let mesh = engine.builtin_mesh(&device, Primitive::Cube);

// per frame
engine.update(&scene_using(mesh));
engine.prepare(&device, &queue, (width, height)); // color-attachment (surface) size

let mut encoder = device.create_command_encoder(&Default::default());
{
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &frame_view,                 // host target (LoadOp::Load to preserve)
            resolve_target,                    // host-owned if sample_count > 1
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: StoreOp::Store },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: engine.depth_view(),         // <-- engine-owned depth (FR-016)
            depth_ops: Some(wgpu::Operations { load: LoadOp::Clear(1.0), store: StoreOp::Store }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    engine.render(&mut pass, viewport);        // viewport + scissor; draws the mesh
}
queue.submit([encoder.finish()]);
```

## Reference integration (b): iced shader widget

The engine's adapter implements iced's `shader::Primitive`; iced supplies the encoder and the
target view, and the adapter begins the pass (attaching the engine depth view) and sets the
scissor to iced's clip bounds. This adapter lives in the `iced-demo` example crate, not the
library.

```text
impl shader::Primitive for NacrePrimitive {
    fn prepare(&self, device, queue, format, storage, bounds, viewport) {
        // ensure an Engine exists in `storage`, then:
        engine.update(&self.scene);
        engine.prepare(device, queue, target_size(viewport)); // full iced target size
    }

    fn render(&self, encoder, storage, target /* &TextureView */, clip_bounds) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(ColorAttachment {
                view: target, ops: Operations { load: LoadOp::Load, store: Store }, ..
            })],
            depth_stencil_attachment: Some(DepthStencilAttachment {
                view: engine.depth_view(),     // <-- same engine-owned depth view
                depth_ops: Some(Operations { load: LoadOp::Clear(1.0), store: Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        let vp = viewport_from(clip_bounds);
        engine.render(&mut pass, vp);          // identical engine call as integration (a)
    }
}
```

## Why the API is identical

In both cases the engine call is `engine.prepare(device, queue, target_size)` then
`engine.render(&mut pass, viewport)`. The only difference is the *site* of
`begin_render_pass` (host vs adapter). The engine exposes `depth_view()`/`depth_format()` so
either site can attach depth. No engine code branches on the host type, proving
host-agnosticism (FR-005, SC-003).

## Conformance checklist

- [ ] Color attachment format + sample count equal `EngineConfig`.
- [ ] Color attachment uses `LoadOp::Load` (engine never clears the whole target).
- [ ] Depth attachment is `engine.depth_view()` with `engine.depth_format()`.
- [ ] `prepare()` is called every frame and after any target (color-attachment) size change.
- [ ] The engine and host share one `wgpu` crate instance (version match).
