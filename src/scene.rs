//! Host-facing scene description handed to the engine each frame.

use crate::material::Material;
use crate::mesh::MeshHandle;
use glam::{Mat4, Vec3};

/// A rectangular sub-region of the target to render into. Also drives depth-buffer
/// sizing. A zero-width or zero-height viewport makes the frame a safe no-op.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
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
    pub view: Mat4,
    pub projection: Mat4,
    pub position: Vec3,
}

/// An analytical light. Direct lighting only (no IBL).
#[derive(Clone, Copy, Debug)]
pub enum Light {
    Directional {
        /// Direction the light travels (need not be normalized).
        direction: Vec3,
        color: Vec3,
        intensity: f32,
    },
    Point {
        position: Vec3,
        color: Vec3,
        intensity: f32,
        /// Soft cutoff distance.
        range: f32,
    },
}

/// The per-frame scene: one mesh, one material, a camera, and a set of lights.
pub struct Scene<'a> {
    pub camera: Camera,
    pub mesh: MeshHandle,
    /// Model-to-world transform (default identity).
    pub transform: Mat4,
    pub material: Material<'a>,
    /// Up to [`crate::MAX_LIGHTS`] lights are used; extras are ignored.
    pub lights: &'a [Light],
}
