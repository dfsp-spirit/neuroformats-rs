//! Functions for managing FreeSurfer brain surface parcellations in annot files.
//!
//! These files assign each vertex of a brain surface mesh to exactly one brain region
//! or label. A so-called colortable contains data on the regions, including the region's
//! name, an RGB display color, and a unique identifier.

use byteordered::{ByteOrdered};

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path};

use crate::util::read_fixed_length_string;
use crate::error::{NeuroformatsError, Result};


#[derive(Debug, Clone, PartialEq)]
pub struct FsAnnotColortable {
    pub id: Vec<i32>,  // A region index, not really needed. The 'label' is unique as well and more relevant, see below.
    pub name: Vec<String>,
    pub r: Vec<i32>,
    pub g: Vec<i32>,
    pub b: Vec<i32>,
    pub a: Vec<i32>,
    pub label: Vec<i32>, // A unique label computed from r,g,b. Used in annot.vertex_labels to identify the region.
}

impl FsAnnotColortable {

    /// Read a colortable in format version 2 from a reader. The reader must be at the start position of the colortable.
    pub fn from_reader<S>(input: &mut S) -> Result<FsAnnotColortable>
    where
        S: Read,
    {
        let mut input = ByteOrdered::be(input);

        let num_chars_orig_filename: i32 = input.read_i32()?; // Length of following string.
        let _orig_filename = read_fixed_length_string(&mut input, num_chars_orig_filename as usize);
        let num_colortable_entries: i32 = input.read_i32()?; // Yes, it is stored twice. Once here, once before.

        let mut id: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
        let mut name: Vec<String> = Vec::with_capacity(num_colortable_entries as usize);
        let mut r: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
        let mut g: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
        let mut b: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
        let mut a: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
        let mut label: Vec<i32> = Vec::with_capacity(num_colortable_entries as usize);
    
        for idx in 0..num_colortable_entries as usize {
            id.push(input.read_i32()?);
            let num_chars_region_name: i32 = input.read_i32()?; // Length of following string.
            name.push(read_fixed_length_string(&mut input, num_chars_region_name as usize));
            r.push(input.read_i32()?);
            g.push(input.read_i32()?);
            b.push(input.read_i32()?);
            a.push(input.read_i32()?);

            label.push(r[idx] + g[idx]*2^8 + b[idx]*2^16 + a[idx]*2^24);
        }

        let ct = FsAnnotColortable { 
            id: id,
            name: name,
            r: r,
            g: g,
            b: b,
            a: a,
            label: label,
        };

        Ok(ct)
    }

}

/// Models a FreeSurfer brain surface parcellation from an annot file. This is the result of applying a brain atlas (like Desikan-Killiani) to a subject. The `vertex_indices` are the 0-based indices used in FreeSurfer and should be ignored. The `vertex_labels` field contains the mesh vertices in order, and assigns to each vertex a brain region using the `label` field (not the `id` field!) from the `colortable`. The field `colortable` contains an [`FsAnnotColortable`] struct that describes the brain regions.
#[derive(Debug, Clone, PartialEq)]
pub struct FsAnnot {
    vertex_indices: Vec<i32>, // 0-based indices, not really needed.
    vertex_labels: Vec<i32>,
    colortable: FsAnnotColortable,
}

impl FsAnnot {
    /// Read an FsSurface instance from a file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsAnnot> {

        let file = BufReader::new(File::open(path)?);
        let mut file = ByteOrdered::be(file);

        let num_vertices: i32 = file.read_i32()?;

        let mut vertex_indices : Vec<i32> = Vec::with_capacity(num_vertices as usize);
        let mut vertex_labels : Vec<i32> = Vec::with_capacity(num_vertices as usize);
        for _ in 1..=num_vertices {
            vertex_indices.push(file.read_i32()?);
            vertex_labels.push(file.read_i32()?);
        }

        let has_colortable: i32 = file.read_i32()?;

        if has_colortable != 1 {
            return Err(NeuroformatsError::UnsupportedFsAnnotFormatVersion);
        }

        let num_colortable_entries: i32 = file.read_i32()?;

        if num_colortable_entries == -2 { // If this is negative, the absolute value encodes the file format version. We only support version 2.
            let _num_colortable_entries: i32 = file.read_i32()?;  // For version 2, the next i32 stores the actual number of entries.

            let colortable: FsAnnotColortable = FsAnnotColortable::from_reader(&mut file)?;

            let annot = FsAnnot { 
                vertex_indices: vertex_indices,
                vertex_labels: vertex_labels,
                colortable: colortable,
            };

            Ok(annot)    
        } else {
            Err(NeuroformatsError::UnsupportedFsAnnotFormatVersion)
        }
    }
}


/// Read a brain parcellation from a FreeSurfer annot file.
///
/// A parcellation assigns each vertex of a brain surface mesh to exactly one brain region.
/// The colortable contains data on the regions, including the region's
/// name, an RGB display color, and a unique identifier. A parcellation is the result of 
/// applying a brain atlas to the brain surface reconstruction of a subject.
///
/// # Examples
///
/// ```no_run
/// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot");
/// ```
pub fn read_annot<P: AsRef<Path> + Copy>(path: P) -> Result<FsAnnot> {
    FsAnnot::from_file(path)
}




#[cfg(test)]
mod test { 
    use super::*;

    #[test]
    fn the_demo_annot_file_can_be_read() {
        const ANNOT_FILE: &str = "resources/subjects_dir/subject1/label/lh.aparc.annot";
        let annot = read_annot(ANNOT_FILE).unwrap();

        assert_eq!(149244, annot.vertex_indices.len());
        assert_eq!(149244, annot.vertex_labels.len());

        assert_eq!(36, annot.colortable.id.len());
        assert_eq!(36, annot.colortable.name.len());
        assert_eq!(36, annot.colortable.r.len());
        assert_eq!(36, annot.colortable.g.len());
        assert_eq!(36, annot.colortable.b.len());
        assert_eq!(36, annot.colortable.a.len());
        assert_eq!(36, annot.colortable.label.len());
    }
}
