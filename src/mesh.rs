//! Geometry: the GPU vertex layout, host-facing mesh data, built-in primitives,
//! and the opaque handle that references uploaded GPU buffers.

use crate::error::EngineError;
use bytemuck::{Pod, Zeroable};
use std::f32::consts::{PI, TAU};

/// Interleaved vertex matching `pbr.wgsl` (`array_stride` 48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}

impl Vertex {
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x3, // position
            1 => Float32x3, // normal
            2 => Float32x2, // uv
            3 => Float32x4, // tangent
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

/// A built-in primitive shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    /// Unit cube centered at the origin (half-extent 0.5).
    Cube,
    /// UV sphere of radius 0.5 centered at the origin.
    Sphere,
}

/// Host-supplied geometry. All per-vertex attributes must have the same length;
/// tangents (xyz + handedness w) are required (no auto-generation in this feature).
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    /// Per-vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals (same length as `positions`).
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex texture coordinates (same length as `positions`).
    pub uvs: Vec<[f32; 2]>,
    /// Per-vertex tangents as xyz + handedness `w` (same length as `positions`).
    pub tangents: Vec<[f32; 4]>,
    /// Triangle indices: length a multiple of 3, each value `< positions.len()`.
    pub indices: Vec<u32>,
}

/// Opaque handle to an uploaded mesh, referenced by [`crate::Scene`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshHandle(pub(crate) usize);

/// Uploaded GPU buffers for one mesh.
pub(crate) struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl MeshData {
    /// Validate attribute parity and index integrity.
    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        let n = self.positions.len();
        if n == 0 || self.indices.is_empty() {
            return Err(EngineError::EmptyGeometry);
        }
        if self.normals.len() != n {
            return Err(EngineError::AttributeLengthMismatch {
                attribute: "normals",
                expected: n,
                found: self.normals.len(),
            });
        }
        if self.uvs.len() != n {
            return Err(EngineError::AttributeLengthMismatch {
                attribute: "uvs",
                expected: n,
                found: self.uvs.len(),
            });
        }
        if self.tangents.len() != n {
            return Err(EngineError::AttributeLengthMismatch {
                attribute: "tangents",
                expected: n,
                found: self.tangents.len(),
            });
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(EngineError::IndicesNotTriangles {
                len: self.indices.len(),
            });
        }
        for &idx in &self.indices {
            if idx as usize >= n {
                return Err(EngineError::IndexOutOfRange {
                    index: idx,
                    vertex_count: n as u32,
                });
            }
        }
        Ok(())
    }

    /// Interleave attributes into the GPU vertex layout.
    pub(crate) fn to_vertices(&self) -> Vec<Vertex> {
        (0..self.positions.len())
            .map(|i| Vertex {
                position: self.positions[i],
                normal: self.normals[i],
                uv: self.uvs[i],
                tangent: self.tangents[i],
            })
            .collect()
    }
}

/// Mesh data for a built-in primitive (with generated normals and tangents).
pub(crate) fn primitive_data(primitive: Primitive) -> MeshData {
    match primitive {
        Primitive::Cube => cube(),
        Primitive::Sphere => sphere(),
    }
}

/// Unit cube centered at the origin (half-extent 0.5), 24 vertices / 36 indices.
fn cube() -> MeshData {
    // (normal, tangent/U, bitangent/V) per face.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // +Z
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // -Z
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]), // +X
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), // -X
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]), // +Y
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), // -Y
    ];
    let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    let mut data = MeshData::default();
    for (i, (n, t, b)) in faces.iter().enumerate() {
        for (k, (sx, sy)) in corners.iter().enumerate() {
            data.positions.push([
                (n[0] + t[0] * sx + b[0] * sy) * 0.5,
                (n[1] + t[1] * sx + b[1] * sy) * 0.5,
                (n[2] + t[2] * sx + b[2] * sy) * 0.5,
            ]);
            data.normals.push(*n);
            data.uvs.push(uvs[k]);
            data.tangents.push([t[0], t[1], t[2], 1.0]);
        }
        let o = (i * 4) as u32;
        data.indices
            .extend_from_slice(&[o, o + 1, o + 2, o, o + 2, o + 3]);
    }
    data
}

/// UV sphere of radius 0.5 centered at the origin.
fn sphere() -> MeshData {
    const STACKS: usize = 24;
    const SECTORS: usize = 48;
    const RADIUS: f32 = 0.5;

    let mut data = MeshData::default();
    for i in 0..=STACKS {
        let v = i as f32 / STACKS as f32;
        let phi = v * PI; // 0..pi (north to south)
        let (sp, cp) = phi.sin_cos();
        for j in 0..=SECTORS {
            let u = j as f32 / SECTORS as f32;
            let theta = u * TAU;
            let (st, ct) = theta.sin_cos();
            let n = [sp * ct, cp, sp * st];
            data.positions
                .push([n[0] * RADIUS, n[1] * RADIUS, n[2] * RADIUS]);
            data.normals.push(n);
            data.uvs.push([u, v]);
            // Tangent along +theta.
            data.tangents.push([-st, 0.0, ct, 1.0]);
        }
    }
    let cols = SECTORS + 1;
    for i in 0..STACKS {
        for j in 0..SECTORS {
            let a = (i * cols + j) as u32;
            let b = ((i + 1) * cols + j) as u32;
            data.indices
                .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> MeshData {
        MeshData {
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0; 2]; 3],
            tangents: vec![[1.0, 0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn accepts_well_formed_mesh() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn rejects_empty_geometry() {
        let mut m = valid();
        m.positions.clear();
        assert_eq!(m.validate(), Err(EngineError::EmptyGeometry));
    }

    #[test]
    fn rejects_attribute_length_mismatch() {
        let mut m = valid();
        m.normals.pop();
        assert!(matches!(
            m.validate(),
            Err(EngineError::AttributeLengthMismatch {
                attribute: "normals",
                ..
            })
        ));
    }

    #[test]
    fn rejects_non_triangle_indices() {
        let mut m = valid();
        m.indices = vec![0, 1];
        assert_eq!(
            m.validate(),
            Err(EngineError::IndicesNotTriangles { len: 2 })
        );
    }

    #[test]
    fn rejects_out_of_range_index() {
        let mut m = valid();
        m.indices = vec![0, 1, 9];
        assert_eq!(
            m.validate(),
            Err(EngineError::IndexOutOfRange {
                index: 9,
                vertex_count: 3,
            })
        );
    }

    #[test]
    fn builtin_primitives_are_valid() {
        assert!(primitive_data(Primitive::Cube).validate().is_ok());
        assert!(primitive_data(Primitive::Sphere).validate().is_ok());
    }
}
