//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on surface-based MRI data as produced by FreeSurfer.


#[cfg(test)]
extern crate approx;

mod util;
pub mod error;
pub mod fs_curv;
pub mod fs_surface;

pub use fs_curv::{CurvHeader, FsCurv, read_curv};
pub use fs_surface::{FsSurfaceHeader, FsSurface, BrainMesh, read_surf};

