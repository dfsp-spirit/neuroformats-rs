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
    pub vertex_index: Vec<i32>,
    pub coord1: Vec<f32>,
    pub coord2: Vec<f32>,
    pub coord3: Vec<f32>,
    pub value: Vec<f32>,
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
        let first_val = self.value.first().expect("Empty label");
        for (idx, val) in self.value.iter().enumerate() {
            if idx > 0 {
                if val != first_val {
                    return false;
                }
            }
        }
        true
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
        if num_surface_verts < self.vertex_index.len() {
            // In this case, num_surface_verts is definitely wrong, but we do not check the max index here, which means stuff can still go wrong below.
            panic!("Invalid vertex count 'num_surface_verts' for surface: label contains {} vertices, surface cannot contain only {}.", self.vertex_index.len(), num_surface_verts);
        }
        let mut data_bin = vec![false; num_surface_verts];
        for label_vert_idx in self.vertex_index.iter() {
            data_bin[*label_vert_idx as usize] = true;
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
        for (label_vert_idx, surface_vert_idx) in self.vertex_index.iter().enumerate() {
            surface_data[*surface_vert_idx as usize] = self.value[label_vert_idx];
        }
        surface_data
    }
}

impl fmt::Display for FsLabel {    
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {        
        write!(f, "Label for {} vertices/voxels, with label values in range {} to {}.", self.vertex_index.len(), vec32minmax(&self.value, false).0, vec32minmax(&self.value, false).1)
    }
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
/// println!("Vertex #{} has coordinates {} {} {} and is assigned value {}.", label.vertex_index[0], label.coord1[0], label.coord2[0], label.coord3[0], label.value[0]);
/// ```
pub fn read_label<P: AsRef<Path>>(path: P) -> Result<FsLabel> {

    let reader = BufReader::new(File::open(path)?);

    let mut label = FsLabel {
        vertex_index : Vec::new(),
        coord1 : Vec::new(),
        coord2 : Vec::new(),
        coord3 : Vec::new(),
        value : Vec::new(),
    };

    let mut hdr_num_entries: i32 = 0;

    // Read the file line by line using the lines() iterator from std::io::BufRead.
    for (index, line) in reader.lines().enumerate() {
        // We ignore the first line at index 0: it is a comment line.
        
        if index == 1 {
            hdr_num_entries = line?.parse::<i32>().expect("Could not parse label header line.");
        }
        else if index >= 2 {
            let line = line?;
            let mut iter = line.split_whitespace();
            label.vertex_index.push(iter.next().unwrap().parse::<i32>().expect("Expected vertex index of type i32."));
            label.coord1.push(iter.next().unwrap().parse::<f32>().expect("Expected coord1 of type f32."));
            label.coord2.push(iter.next().unwrap().parse::<f32>().expect("Expected coord2 of type f32."));
            label.coord3.push(iter.next().unwrap().parse::<f32>().expect("Expected coord3 of type f32."));
            label.value.push(iter.next().unwrap().parse::<f32>().expect("Expected vertex value of type f32."));
        }        
    }

    if hdr_num_entries as usize != label.vertex_index.len() {
        Err(NeuroformatsError::InvalidFsLabelFormat)
    } else {
        Ok(label)
    }
}


/// Write an FsLabel struct to a new file.
pub fn write_label<P: AsRef<Path> + Copy>(path: P, label : &FsLabel) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut file = LineWriter::new(file);

    let header_lines = format!("# FreeSurfer label.\n{}\n", label.vertex_index.len());
    let header_lines = header_lines.as_bytes();
    file.write_all(header_lines)?;

    for (idx, _) in label.vertex_index.iter().enumerate() {
        let vline = format!("{} {} {} {} {}\n", label.vertex_index[idx], label.coord1[idx], label.coord2[idx], label.coord3[idx], label.value[idx]);
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
        assert_eq!(expected_vertex_count, label.vertex_index.len());
        assert_eq!(expected_vertex_count, label.coord1.len());
        assert_eq!(expected_vertex_count, label.coord2.len());
        assert_eq!(expected_vertex_count, label.coord3.len());
        assert_eq!(expected_vertex_count, label.value.len());
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
        assert_eq!(expected_vertex_count, label_re.vertex_index.len());
        assert_eq!(expected_vertex_count, label_re.coord1.len());
        assert_eq!(expected_vertex_count, label_re.coord2.len());
        assert_eq!(expected_vertex_count, label_re.coord3.len());
        assert_eq!(expected_vertex_count, label_re.value.len());
    }

}
