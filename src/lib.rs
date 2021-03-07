//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on reading surface-based brain morphometry data as produced from MRI images by FreeSurfer, CAT12 and similar software packages.


#[cfg(test)]
extern crate approx;

mod util;
pub mod error;
pub mod fs_curv;
pub mod fs_surface;
pub mod fs_label;
pub mod fs_annot;
pub mod fs_mgh;


pub use fs_curv::{FsCurvHeader, FsCurv, read_curv};
pub use fs_surface::{FsSurfaceHeader, FsSurface, BrainMesh, read_surf};
pub use fs_label::{FsLabel, read_label};
pub use fs_annot::{FsAnnot, FsAnnotColortable, read_annot};
pub use fs_mgh::{FsMghHeader, read_mgh};


