//! Functions for managing FreeSurfer brain surface parcellations in annot files.
//!
//! These files assign each vertex of a brain surface mesh to exactly one brain region
//! or label. A so-called colortable contains data on the regions, including the region's
//! name, an RGB display color, and a unique identifier.

use byteordered::{ByteOrdered};

use std::fs::File;
use std::io::{BufReader, BufRead};
use std::path::{Path};
use std::fmt;

use crate::util::read_fixed_length_string;
use crate::error::{NeuroformatsError, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct FsAnnotColortable {
    pub regions: Vec<FsAnnotColorRegion>,
}

impl FsAnnotColortable {

    /// Read a colortable in format version 2 from a reader. The reader must be at the start position of the colortable.
    pub fn from_reader<S>(input: &mut S) -> Result<FsAnnotColortable>
    where
        S: BufRead,
    {
        let mut input = ByteOrdered::be(input);

        let num_chars_orig_filename: i32 = input.read_i32()?; // Length of following string.
        let _orig_filename = read_fixed_length_string(&mut input, num_chars_orig_filename as usize);
        let num_colortable_entries: i32 = input.read_i32()?; // Yes, it is stored twice. Once here, once before.

        let entries = (0..num_colortable_entries)
            .into_iter()
            .map(|_idx| {
                FsAnnotColorRegion::from_reader(input.inner_mut())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(FsAnnotColortable{regions: entries})
    }
}

impl fmt::Display for FsAnnotColortable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Colortable for {} brain regions.", self.regions.len())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsAnnotColorRegion {
    pub id: i32,  // A region index, not really needed. The 'label' is unique as well and more relevant, see below.
    pub name: String,
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
    pub label: i32, // A unique label computed from r,g,b. Used in annot.vertex_labels to identify the region.
}

impl FsAnnotColorRegion {
    pub fn from_reader<S>(input: &mut S) -> Result<FsAnnotColorRegion>
    where
        S: BufRead,
    {
        let mut input = ByteOrdered::be(input);
        let id = input.read_i32()?;
        let num_chars_region_name: i32 = input.read_i32()?; // Length of following string.
        let name = read_fixed_length_string(&mut input, num_chars_region_name as usize)?;
        let r = input.read_i32()?;
        let g = input.read_i32()?;
        let b = input.read_i32()?;
        let a = input.read_i32()?;

        let label = r + g * 2i32.pow(8) + b * 2i32.pow(16) + a * 2i32.pow(24);
        Ok(FsAnnotColorRegion {
            id,
            name,
            r,
            g,
            b,
            a,
            label,
        })
    }
}

/// Models a FreeSurfer brain surface parcellation from an annot file. This is the result of applying a brain atlas (like Desikan-Killiani) to a subject. The `vertex_indices` are the 0-based indices used in FreeSurfer and should be ignored. The `vertex_labels` field contains the mesh vertices in order, and assigns to each vertex a brain region using the `label` field (not the `id` field!) from the `colortable`. The field `colortable` contains an [`FsAnnotColortable`] struct that describes the brain regions.
#[derive(Debug, Clone, PartialEq)]
pub struct FsAnnot {
    pub vertex_indices: Vec<i32>, // 0-based indices, not really needed as all vertices need to be covered in order.
    pub vertex_labels: Vec<i32>,
    pub colortable: FsAnnotColortable,
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

    /// Get the region names contained in the [`FsAnnot`] struct.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
    /// annot.regions();
    /// ```
    pub fn regions(&self) -> Vec<String> {
        self.colortable.regions.iter().map(|entry| entry.name.clone()).collect()
    }


    /// Get the number of regions contained in the [`FsAnnot`] struct, or its [`FsAnnotColortable`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
    /// annot.num_regions();
    /// ```
    pub fn num_regions(&self) -> usize {
        self.regions().len()
    }


    /// Get the indices of all vertices which are part of the given brain region of the [`FsAnnot`] struct.
    ///
    /// Note that it can happen that no vertices are assigned to the region, in which case the result vector is empty.
    ///
    /// # Panics
    ///
    /// If the given `region` is not a valid region name for the [`FsAnnot`] struct.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
    /// annot.region_vertices(String::from("bankssts"));
    /// ```
    pub fn region_vertices(&self, region : String) -> Vec<usize> {
        let region = self.colortable.regions.iter().find(|x| &x.name == &region).expect("No such region in annot.");
        self.vertex_labels
            .iter()
            .enumerate()
            .filter_map(|(idx, vlabel)| (vlabel == &region.label).then_some(idx))
            .collect()
    }


    /// Get the region names for all annot vertices.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
    /// annot.vertex_regions();
    /// ```
    pub fn vertex_regions(&self) -> Vec<String> {
        let mut vert_regions: Vec<String> = vec![String::new(); self.vertex_labels.len()];
        for region in self.colortable.regions.iter() {
            let region_label = region.label;
            let region_name = &region.name;
            for (idx, vlabel) in self.vertex_labels.iter().enumerate() {
                if vlabel == &region_label {
                    vert_regions[idx] = region_name.clone();
                }
            }
        }
        return vert_regions;
    }


    /// Returns the Rust indices into the colortable fields for each vertex.
    ///
    /// # Parameters
    ///
    /// * `unmatched_region_index`: The region index to use for vertices with a label that does not match any region label. Typically they are assigned to an `unknown` region, which should be at the start of the colortable (at index `0`). If in doubt, check the region names of the annot.
    ///
    /// # Panics
    ///
    /// If the `unmatched_region_index` is not a valid index for the [`FsAnnot`] struct, i.e., it is out of range.
    fn vertex_colortable_indices(&self, unmatched_region_index : usize) -> Vec<usize> {
        let mut vert_colortable_indices: Vec<usize> = Vec::with_capacity(self.vertex_labels.len());
        for vlabel in self.vertex_labels.iter() {
            let mut found = false;
            for (region_idx, region) in self.colortable.regions.iter().enumerate() {
                if vlabel == &region.label {
                    vert_colortable_indices.push(region_idx);
                    found = true;
                    break;
                }
            }
            if ! found {
                vert_colortable_indices.push(unmatched_region_index);
            }
        }
        return vert_colortable_indices;
    }


    /// Get the vertex colors for all annot vertices as u8 RGB(A) values.
    ///
    /// The vertex colors are represented as 3 RGB values per vertex if `alpha` is `false`, and as 4 RGBA values per vertex if `alpha` is `true`.
    ///
    /// # Parameters
    ///
    /// * `alpha`: whether to return the alpha channel value.
    /// * `unmatched_region_index`: Determines the region and thus the color that is used for unassigned vertices. This is the region index to use for vertices with a label that does not match any region label. Typically they are assigned to an `unknown` region, which should be at the start of the colortable (at index `0`). If in doubt, check the region names of the annot with [`FsAnnot::regions`].
    ///
    /// # Panics
    ///
    /// * If the `unmatched_region_index` is out of range for this FsAnnot, see [`FsAnnot::num_regions`] to check before calling this function.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
    /// let col_rgba = annot.vertex_colors(true, 0);
    /// assert_eq!(col_rgba.len(), annot.vertex_indices.len() * 4);
    /// let col_rgb = annot.vertex_colors(false, 0);
    /// assert_eq!(col_rgb.len(), annot.vertex_indices.len() * 3);
    /// ```
    pub fn vertex_colors(&self, alpha : bool, unmatched_region_index: usize) -> Vec<u8> {
        let capacity = if alpha { self.vertex_labels.len() * 4 } else { self.vertex_labels.len() * 3 };
        let mut vert_colors: Vec<u8> = Vec::with_capacity(capacity);

        for ct_region_idx in self.vertex_colortable_indices(unmatched_region_index) {
            let region = &self.colortable.regions[ct_region_idx];
            vert_colors.push(region.r as u8);
            vert_colors.push(region.g as u8);
            vert_colors.push(region.b as u8);
            if alpha {
                vert_colors.push(region.a as u8);
            }
        }
        vert_colors
    }

}


impl fmt::Display for FsAnnot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Surface parcellation assigning {} vertices to {} brain regions.", self.vertex_indices.len(), self.colortable.regions.len())
    }
}


/// Read a brain parcellation from a FreeSurfer annot file.
///
/// A parcellation assigns each vertex of a brain surface mesh to exactly one brain region.
/// The colortable contains data on the regions, including the region's
/// name, an RGB display color, and a unique identifier. A parcellation is the result of
/// applying a brain atlas to the brain surface reconstruction of a subject.
///
/// # See also
///
/// One can use the functions [`FsAnnot::regions`], [`FsAnnot::vertex_regions`], and [`FsAnnot::region_vertices`] to
/// perform common tasks related to brain surface parcellations.
///
/// # Examples
///
/// ```no_run
/// let annot = neuroformats::read_annot("/path/to/subjects_dir/subject1/label/lh.aparc.annot").unwrap();
/// println!("Annotation assigns the {} brain mesh vertices to {} different regions.", annot.vertex_indices.len(), annot.regions().len());
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

        assert_eq!(36, annot.colortable.regions.len());

        let first_region = &annot.colortable.regions[0];
        assert_eq!(0, first_region.id);
        assert_eq!("unknown", first_region.name);
        assert_eq!(25, first_region.r);
        assert_eq!(5, first_region.g);
        assert_eq!(25, first_region.b);
        assert_eq!(0, first_region.a);
        assert_eq!(1639705, first_region.label);
    }

    #[test]
    fn annot_region_names_are_read_correctly() {
        const ANNOT_FILE: &str = "resources/subjects_dir/subject1/label/lh.aparc.annot";
        let annot = read_annot(ANNOT_FILE).unwrap();
        let regions : Vec<String> = annot.regions();

        assert_eq!(36, regions.len());
        assert_eq!(36, annot.num_regions());
        assert_eq!(regions[0], "unknown");
        assert_eq!(regions[1], "bankssts");
        assert_eq!(regions[35], "insula");
    }

    #[test]
    fn annot_region_vertices_are_computed_correctly() {
        const ANNOT_FILE: &str = "resources/subjects_dir/subject1/label/lh.aparc.annot";
        let annot = read_annot(ANNOT_FILE).unwrap();
        let region_verts : Vec<usize> = annot.region_vertices(String::from("bankssts"));

        assert_eq!(1722, region_verts.len());
    }

    #[test]
    fn annot_region_indices_are_computed_correctly() {
        const ANNOT_FILE: &str = "resources/subjects_dir/subject1/label/lh.aparc.annot";
        let annot = read_annot(ANNOT_FILE).unwrap();
        assert_eq!(149244, annot.vertex_indices.len());

        let mut region_indices : Vec<usize> = annot.vertex_colortable_indices(0);

        region_indices.sort();
        assert_eq!(*region_indices.first().unwrap(), 0 as usize);
        assert_eq!(*region_indices.last().unwrap(), 35 as usize);

        assert_eq!(149244, region_indices.len());
    }

    #[test]
    fn annot_vertex_colors_are_computed_correctly() {
        let annot = read_annot("resources/subjects_dir/subject1/label/lh.aparc.annot").unwrap();

        assert_eq!(149244, annot.vertex_indices.len());

        let col_rgba = annot.vertex_colors(true, 0);
        assert_eq!(col_rgba.len(), annot.vertex_indices.len() * 4);
        let col_rgb = annot.vertex_colors(false, 0);
        assert_eq!(col_rgb.len(), annot.vertex_indices.len() * 3);
    }


}
