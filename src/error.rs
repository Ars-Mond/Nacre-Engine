//! Error type returned by fallible engine operations (mesh upload validation).

use std::fmt;

/// Errors produced when validating host-supplied mesh data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The mesh had no vertices or no indices.
    EmptyGeometry,
    /// A per-vertex attribute did not match the position count.
    AttributeLengthMismatch {
        /// Name of the mismatched attribute (e.g. `"normals"`).
        attribute: &'static str,
        /// Expected length (the position count).
        expected: usize,
        /// Actual length found.
        found: usize,
    },
    /// An index referenced a vertex outside the buffer.
    IndexOutOfRange {
        /// The offending index value.
        index: u32,
        /// Number of vertices available.
        vertex_count: u32,
    },
    /// The index count was not a multiple of three (not whole triangles).
    IndicesNotTriangles {
        /// The index count that was not a multiple of three.
        len: usize,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::EmptyGeometry => write!(f, "mesh has no vertices or no indices"),
            EngineError::AttributeLengthMismatch {
                attribute,
                expected,
                found,
            } => write!(
                f,
                "attribute `{attribute}` length {found} does not match position count {expected}"
            ),
            EngineError::IndexOutOfRange {
                index,
                vertex_count,
            } => write!(
                f,
                "index {index} is out of range for {vertex_count} vertices"
            ),
            EngineError::IndicesNotTriangles { len } => {
                write!(f, "index count {len} is not a multiple of 3")
            }
        }
    }
}

impl std::error::Error for EngineError {}
