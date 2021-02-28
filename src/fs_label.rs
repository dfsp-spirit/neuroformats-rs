/// Functions for reading FreeSurfer label files.


use csv::{ReaderBuilder};

use std::fs::File;
use std::io::{BufReader};
use std::path::{Path};

use std::error::Error;


#[derive(Debug, Clone, PartialEq)]
pub struct FsLabel {
    pub vertex_index: Vec<i32>,
    pub coord1: Vec<f32>,
    pub coord2: Vec<f32>,
    pub coord3: Vec<f32>,
    pub value: Vec<f32>,
}



pub fn read_label<P: AsRef<Path>>(path: P) -> Result<FsLabel, Box<dyn Error>> {

    let file = BufReader::new(File::open(path)?);
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        //.delimiter(b' ')
        .flexible(true)
        .comment(Some(b'#'))
        .from_reader(file);

    for result in rdr.records() {
        // The iterator yields Result<StringRecord, Error>, so we check the
        // error here.
        let record = result?;
        println!("{:?}", record);
    }

    let label = FsLabel {
        vertex_index : Vec::new(),
        coord1 : Vec::new(),
        coord2 : Vec::new(),
        coord3 : Vec::new(),
        value : Vec::new(),
    };

    Ok(label)
}


#[cfg(test)]
mod test { 
    use super::*;

    #[test]
    fn the_demo_surface_label_file_can_be_read() {
        const LABEL_FILE: &str = "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label";
        let label = read_label(LABEL_FILE).unwrap();

        assert_eq!(1085, label.vertex_index.len());
    }
}
