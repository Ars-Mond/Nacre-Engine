//! Host-facing scene description handed to the engine each frame.

use crate::material::Material;
use crate::mesh::MeshHandle;
use glam::{Mat4, Vec3};

/// A rectangular sub-region of the target to render into. Also drives depth-buffer
/// sizing. A zero-width or zero-height viewport makes the frame a safe no-op.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    /// Left edge of the draw rectangle within the target, in pixels.
    pub x: f32,
    /// Top edge of the draw rectangle within the target, in pixels.
    pub y: f32,
    /// Width of the draw rectangle, in pixels.
    pub width: f32,
    /// Height of the draw rectangle, in pixels.
    pub height: f32,
    /// Minimum depth written to the depth buffer (usually 0.0).
    pub min_depth: f32,
    /// Maximum depth written to the depth buffer (usually 1.0).
    pub max_depth: f32,
}

impl Viewport {
    /// A viewport at `(x, y)` with the given size and depth range `0.0..1.0`.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// View and projection transforms plus the camera world position (for the
/// specular view vector). Right-handed, Y-up; projection uses wgpu depth `0..1`.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// World-to-view transform (right-handed, Y-up).
    pub view: Mat4,
    /// View-to-clip projection (wgpu depth range 0..1).
    pub projection: Mat4,
    /// Camera world position, used for the specular view vector.
    pub position: Vec3,
}

/// An analytical light. Direct lighting only (no IBL).
#[derive(Clone, Copy, Debug)]
pub enum Light {
    /// A light infinitely far away, casting parallel rays.
    Directional {
        /// Direction the light travels (need not be normalized).
        direction: Vec3,
        /// Linear RGB color.
        color: Vec3,
        /// Scalar intensity multiplier.
        intensity: f32,
    },
    /// A light at a world position, with inverse-square falloff.
    Point {
        /// World-space position of the light.
        position: Vec3,
        /// Linear RGB color.
        color: Vec3,
        /// Scalar intensity multiplier.
        intensity: f32,
        /// Soft cutoff distance.
        range: f32,
    },
}

/// The per-frame scene: one mesh, one material, a camera, and a set of lights.
pub struct Scene<'a> {
    /// Camera that views the scene.
    pub camera: Camera,
    /// Handle to the single mesh to draw this frame.
    pub mesh: MeshHandle,
    /// Model-to-world transform (default identity).
    pub transform: Mat4,
    /// Surface material for the mesh.
    pub material: Material<'a>,
    /// Up to [`crate::MAX_LIGHTS`] lights are used; extras are ignored.
    pub lights: &'a [Light],
}
