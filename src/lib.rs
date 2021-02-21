//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on surface-based MRI data as produced by FreeSurfer.

mod util;
pub mod error;
pub mod fs_curv;

pub use fs_curv::{CurvHeader, FsCurv, read_curv};
