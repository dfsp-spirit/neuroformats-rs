//! Functions for reading FreeSurfer label files.
//!
//! A label groups a number of vertices (for surface label) or voxels (for volume labels) together. E.g., all
//! vertices which are part of a certain brain region can be stored in a label. Note though that nothing requires that the
//! vertices of a label form a spatially adjacent patch. Each vertex or voxel that is part of the label can be assigned a scalar value.


use std::fs::File;
use std::io::{BufRead, BufReader, Write, LineWriter};
use std::path::{Path};
use std::fmt;


use crate::error::{NeuroformatsError, Result};
use crate::util::vec32minmax;

#[derive(Debug, Clone, PartialEq)]
pub struct FsLabel {
    pub vertexes: Vec<FsLabelVertex>,
}

impl FsLabel {

    /// Determine whether this is a binary label. 
    ///
    /// A binary label assigns the same value (typically `0.0`) to all its vertices.
    /// Such a label is typically used to define a region of some sort, e.g., a single brain region extracted from a brain
    /// surface parcellation (see FsAnnot). Whether or not the label is intended as a binary inside/outside region definition
    /// cannot be known, so treat the return value as an educated guess.
    ///
    /// # Panics
    ///
    /// * If the label is empty, i.e., contains no values.
    pub fn is_binary(&self) -> bool {
        let mut values_iter = self.vertexes.iter().map(|x| x.value);
        let first_val = values_iter.next().expect("Empty label");
        values_iter.all(|val| val == first_val)
    }


    /// Determine for each vertex whether it is part of this label.
    ///
    /// This is a simple convenience function. Note that you need to supply the total number of vertices of
    /// the respective surface, as that number is not stored in the label.
    ///
    /// # Panics
    ///
    /// * If `num_surface_verts` is smaller than the max index stored in the label. If this happens, the label cannot belong to the respective surface.
    pub fn is_surface_vertex_in_label(&self, num_surface_verts: usize) -> Vec<bool> {
        if num_surface_verts < self.vertexes.len() {
            // In this case, num_surface_verts is definitely wrong, but we do not check the max index here, which means stuff can still go wrong below.
            panic!("Invalid vertex count 'num_surface_verts' for surface: label contains {} vertices, surface cannot contain only {}.", self.vertexes.len(), num_surface_verts);
        }
        let mut data_bin = vec![false; num_surface_verts];
        for label_vert in self.vertexes.iter() {
            data_bin[label_vert.index as usize] = true;
        }
        data_bin
    }


    /// Generate data for the whole surface from this label.
    ///
    /// This is a simple convenience function that creates a data vector with the specified length and fills it with the label
    /// value for vertices which are part of this label and sets the rest to the `not_in_label_value` (typically `f32::NAN`).
    ///
    /// # Panics
    ///
    /// * If `num_surface_verts` is smaller than the max index stored in the label. If this happens, the label cannot belong to the respective surface.
    pub fn as_surface_data(&self, num_surface_verts : usize, not_in_label_value : f32) -> Vec<f32> {
        let mut surface_data : Vec<f32> = vec![not_in_label_value; num_surface_verts];
        for surface_vert in self.vertexes.iter() {
            surface_data[surface_vert.index as usize] = surface_vert.value;
        }
        surface_data
    }
}

impl fmt::Display for FsLabel {    
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {        
        let (min, max) = vec32minmax(self.vertexes.iter().map(|v| v.value), false);
        write!(f, "Label for {} vertices/voxels, with label values in range {} to {}.", self.vertexes.len(), min, max)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsLabelVertex {
    pub index: i32,
    pub coord1: f32,
    pub coord2: f32,
    pub coord3: f32,
    pub value: f32,
}

/// Read a surface label or volume label from a file in FreeSurfer label format.
///
/// A label groups a number of vertices (for surface label) or voxels (for volume labels) together. It can
/// also assign a scalar value to each element.
///
/// # Examples
///
/// ```no_run
/// let label = neuroformats::read_label("/path/to/subjects_dir/subject1/label/lh.entorhinal_exvivo.label").unwrap();
/// let first = &label.vertexes[0];
/// println!("Vertex #{} has coordinates {} {} {} and is assigned value {}.", first.index, first.coord1, first.coord2, first.coord3, first.value);
/// ```
pub fn read_label<P: AsRef<Path>>(path: P) -> Result<FsLabel> {

    let reader = BufReader::new(File::open(path)?);

    // Read the file line by line using the lines() iterator from std::io::BufRead.
    let mut lines = reader.lines();
    // We ignore the first line at index 0: it is a comment line.
    let _comment_line = lines.next().transpose()?;
    // The line 1 (after comment) is the header
    let hdr_num_entries: i32 = lines.next().transpose()?.and_then(|header| header.parse::<i32>().ok()).expect("Could not parse label header line.");
    let mut vertexes = Vec::with_capacity(hdr_num_entries as usize);
    for line in lines {
        let line = line?;
        let mut iter = line.split_whitespace();
        let index = iter.next().unwrap().parse::<i32>().expect("Expected vertex index of type i32.");
        let coord1 = iter.next().unwrap().parse::<f32>().expect("Expected coord1 of type f32.");
        let coord2 = iter.next().unwrap().parse::<f32>().expect("Expected coord2 of type f32.");
        let coord3 = iter.next().unwrap().parse::<f32>().expect("Expected coord3 of type f32.");
        let value = iter.next().unwrap().parse::<f32>().expect("Expected vertex value of type f32.");
        vertexes.push(FsLabelVertex{ index, coord1, coord2, coord3, value });
    }

    if hdr_num_entries as usize != vertexes.len() {
        Err(NeuroformatsError::InvalidFsLabelFormat)
    } else {
        Ok(FsLabel{ vertexes })
    }
}


/// Write an FsLabel struct to a new file.
pub fn write_label<P: AsRef<Path> + Copy>(path: P, label : &FsLabel) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut file = LineWriter::new(file);

    let header_lines = format!("# FreeSurfer label.\n{}\n", label.vertexes.len());
    let header_lines = header_lines.as_bytes();
    file.write_all(header_lines)?;

    for vertex in label.vertexes.iter() {
        let vline = format!("{} {} {} {} {}\n", vertex.index, vertex.coord1, vertex.coord2, vertex.coord3, vertex.value);
        let vline = vline.as_bytes();
        file.write_all(vline)?;
    }

    file.flush()?;

    Ok(())
}


#[cfg(test)]
mod test { 
    use super::*;
    use tempfile::{tempdir};

    #[test]
    fn the_demo_surface_label_file_can_be_read() {
        const LABEL_FILE: &str = "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label";
        let label = read_label(LABEL_FILE).unwrap();

        let expected_vertex_count: usize = 1085;
        assert_eq!(expected_vertex_count, label.vertexes.len());
    }

    #[test]
    fn the_label_utility_functions_work() {
        const LABEL_FILE: &str = "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label";
        let label = read_label(LABEL_FILE).unwrap();

        let num_surface_verts: usize = 160_000;
        let label_mask = label.is_surface_vertex_in_label(num_surface_verts);
        assert_eq!(num_surface_verts, label_mask.len());

        let surface_data = label.as_surface_data(num_surface_verts, f32::NAN);
        assert_eq!(num_surface_verts, surface_data.len());

        assert_eq!(false, label.is_binary());
    }

    #[test]
    fn a_label_file_can_be_written_and_reread() {
        const LABEL_FILE: &str = "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label";
        let label = read_label(LABEL_FILE).unwrap();

        let dir = tempdir().unwrap();

        let tfile_path = dir.path().join("temp-file.label");
        let tfile_path = tfile_path.to_str().unwrap();
        write_label(tfile_path, &label).unwrap();

        let label_re = read_label(tfile_path).unwrap();
        let expected_vertex_count: usize = 1085;
        assert_eq!(expected_vertex_count, label_re.vertexes.len());
    }

}
