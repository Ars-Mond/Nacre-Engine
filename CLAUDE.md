<!-- SPECKIT START -->
## Active feature: 001-render-pbr-mesh

Plan: `specs/001-render-pbr-mesh/plan.md` (see also `spec.md`, `research.md`,
`data-model.md`, `contracts/`, `quickstart.md`).

**What**: Walking skeleton — render one metallic-roughness PBR mesh into a
host-provided texture via the stable `update → prepare → render` lifecycle.

**Stack**: Rust stable, edition 2024. Library deps: `wgpu = 27` (pinned to match
iced 0.14's `wgpu ^27` so the shared Device/types unify), `glam`, `bytemuck`.
Shaders: WGSL only. Example/test deps (`winit`, `iced`, `pollster`, `image`) are
isolated in example member crates and `[dev-dependencies]`.

**Hard constraints** (constitution): engine never creates a window/surface/event
loop/Device/Queue (Principle III); pure-Rust build, no system packages
(Principle I); identical-within-tolerance output on Windows/Linux/macOS
(Principle II). Engine owns/recreates the depth buffer and exposes `depth_view()`;
the pass opener attaches it. Linear color into an sRGB target; no tone mapping.
Material maps are host-created GPU textures.

For full technology, structure, and command details, read the plan above.
<!-- SPECKIT END -->
