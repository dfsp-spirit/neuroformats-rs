//! Rust implementation of several structural neuroimaging file formats.
//!
//! The focus of this package is on reading surface-based brain morphometry data as produced from MRI images by FreeSurfer, CAT12 and similar software packages.

#[cfg(test)]
extern crate approx;

pub mod error;
pub mod fs_annot;
pub mod fs_curv;
pub mod fs_label;
pub mod fs_mgh;
pub mod fs_surface;
pub mod util;

pub use fs_annot::{read_annot, FsAnnot, FsAnnotColortable};
pub use fs_curv::{read_curv, write_curv, FsCurv, FsCurvHeader};
pub use fs_label::{read_label, write_label, FsLabel};
pub use fs_mgh::{
    read_mgh, write_mgh, FsMgh, FsMghData, FsMghHeader, MRI_FLOAT, MRI_INT, MRI_SHORT, MRI_UCHAR,
};
pub use fs_surface::{
    coord_center, coord_extrema, read_surf, write_surf, BrainMesh, FsSurface, FsSurfaceHeader,
};
pub use util::{values_to_colors, vec32minmax};
