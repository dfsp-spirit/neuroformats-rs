/// Functions for reading FreeSurfer label files.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path};


use crate::error::{NeuroformatsError, Result};


#[derive(Debug, Clone, PartialEq)]
pub struct FsLabel {
    pub vertex_index: Vec<i32>,
    pub coord1: Vec<f32>,
    pub coord2: Vec<f32>,
    pub coord3: Vec<f32>,
    pub value: Vec<f32>,
}




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
            hdr_num_entries = line.unwrap().parse::<i32>().unwrap();
        }
        else if index >= 2 {
            let line = line.unwrap();
            let mut iter = line.split_whitespace();
            label.vertex_index.push(iter.next().unwrap().parse::<i32>().unwrap());
            label.coord1.push(iter.next().unwrap().parse::<f32>().unwrap());
            label.coord2.push(iter.next().unwrap().parse::<f32>().unwrap());
            label.coord3.push(iter.next().unwrap().parse::<f32>().unwrap());
            label.value.push(iter.next().unwrap().parse::<f32>().unwrap());
        }        
    }

    if hdr_num_entries as usize != label.vertex_index.len() {
        Err(NeuroformatsError::InvalidFsLabelFormat)
    } else {
        Ok(label)
    }
}


#[cfg(test)]
mod test { 
    use super::*;

    #[test]
    fn the_demo_surface_label_file_can_be_read() {
        const LABEL_FILE: &str = "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label";
        let label = read_label(LABEL_FILE).unwrap();

        assert_eq!(1085, label.vertex_index.len());
        assert_eq!(1085, label.coord1.len());
        assert_eq!(1085, label.coord2.len());
        assert_eq!(1085, label.coord3.len());
        assert_eq!(1085, label.value.len());
    }
}
