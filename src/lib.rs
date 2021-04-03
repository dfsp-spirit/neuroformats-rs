//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on reading surface-based brain morphometry data as produced from MRI images by FreeSurfer, CAT12 and similar software packages.


#[cfg(test)]
extern crate approx;

pub mod util;
pub mod error;
pub mod fs_curv;
pub mod fs_surface;
pub mod fs_label;
pub mod fs_annot;
pub mod fs_mgh;


pub use fs_curv::{FsCurvHeader, FsCurv, read_curv, write_curv};
pub use fs_surface::{FsSurfaceHeader, FsSurface, BrainMesh, read_surf, coord_center, coord_extrema};
pub use fs_label::{FsLabel, read_label};
pub use fs_annot::{FsAnnot, FsAnnotColortable, read_annot};
pub use fs_mgh::{FsMgh, FsMghHeader, FsMghData, read_mgh, MRI_UCHAR, MRI_INT, MRI_FLOAT, MRI_SHORT};
pub use util::{vec32minmax};
