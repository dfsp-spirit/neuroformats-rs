//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on surface-based MRI data as produced by FreeSurfer.

pub mod fs_curv;

pub use fs_curv::CurvHeader;
