//! Functions for managing FreeSurfer per-vertex data in binary 'curv' files.
//!
//! These files store 1 scalar value (typically a morphological descriptor, like cortical thickness at that point)
//! for each vertex of the respective brain surface mesh.


use byteordered::{ByteOrdered};
use flate2::bufread::GzDecoder;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path};

use crate::util::is_gz_file;
use crate::error::{NeuroformatsError, Result};


pub const CURV_MAGIC_CODE_U8: u8 = 255;

#[derive(Debug, Clone, PartialEq)]
pub struct CurvHeader {
    pub curv_magic: [u8; 3],
    pub num_vertices: i32,
    pub num_faces: i32,
    pub num_values_per_vertex: i32,
}


impl Default for CurvHeader {
    fn default() -> CurvHeader {
        CurvHeader {
            curv_magic: [255; 3],
            num_vertices: 0,
            num_faces: 0,
            num_values_per_vertex: 1,
        }
    }
}

impl CurvHeader {
    
    /// Read a Curv header from a file.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Curv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<CurvHeader> {
        let gz = is_gz_file(&path);
        let file = BufReader::new(File::open(path)?);
        if gz {
            CurvHeader::from_reader(GzDecoder::new(file))
        } else {
            CurvHeader::from_reader(file)
        }
    }


    /// Read a Curv header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// Curv header.
    pub fn from_reader<S>(input: S) -> Result<CurvHeader>
    where
        S: Read,
    {
        let mut hdr = CurvHeader::default();
    
        let mut input = ByteOrdered::be(input);

        for v in &mut hdr.curv_magic {
            *v = input.read_u8()?;
        }
    
        hdr.num_vertices = input.read_i32()?;
        hdr.num_faces = input.read_i32()?;
        hdr.num_values_per_vertex = input.read_i32()?;

        if !(hdr.curv_magic[0] == CURV_MAGIC_CODE_U8 && hdr.curv_magic[1] == CURV_MAGIC_CODE_U8 && hdr.curv_magic[2] == CURV_MAGIC_CODE_U8) {
            Err(NeuroformatsError::InvalidCurvFormat)
        } else {
            Ok(hdr)
        }
    }

}


/// An FsCurv object, models a FreeSurfer per-vertex data file in curv format.
#[derive(Debug, PartialEq, Clone)]
pub struct FsCurv {
    pub header: CurvHeader,
    pub data: Vec<f32>, 
}

/// Read per-vertex data from a FreeSurfer curv file.
///
/// A curv file assigns a single scalar value to each vertex of a brain mesh. These values can represent
/// anything, but the files are typically used to store morphological descriptors like the cortical thickness
/// at each point of the brain surface, or statistical results like t value maps. See [`neuroformats::read_surf`] to load
/// the corresponding mesh file. 
///
/// # Examples
///
/// ```no_run
/// let curv = neuroformats::read_curv("/path/to/subjects_dir/subject1/surf/lh.thickness");
/// ```
pub fn read_curv<P: AsRef<Path> + Copy>(path: P) -> Result<FsCurv> {
    FsCurv::from_file(path)
}


impl FsCurv {
    /// Read a Curvfile.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Curv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsCurv> {
        let gz = is_gz_file(&path);

        let hdr = CurvHeader::from_file(path).unwrap();

        let file = BufReader::new(File::open(path)?);


        let data: Vec<f32> = if gz { FsCurv::curv_data_from_reader(GzDecoder::new(file), &hdr) } else  { FsCurv::curv_data_from_reader(file, &hdr) };

        let curv = FsCurv { 
            header : hdr,
            data: data,
        };

        Ok(curv)
    }


    pub fn curv_data_from_reader<S>(input: S, hdr: &CurvHeader) -> Vec<f32>
    where
        S: Read,
    {
    
        let mut input = ByteOrdered::be(input);

        let hdr_size = 15;
        
        // This is only read because we cannot seek in a GZ stream.
        let mut hdr_data : Vec<u8> = Vec::with_capacity(hdr_size as usize);
        for _ in 1..=hdr_size {
            hdr_data.push(input.read_u8().unwrap());
        }

        let mut data : Vec<f32> = Vec::with_capacity(hdr.num_vertices as usize);
        for _ in 1..=hdr.num_vertices {
            data.push(input.read_f32().unwrap());
        }
        data
    }
}


#[cfg(test)]
mod test { 
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn the_demo_curv_file_can_be_read() {
        const CURV_FILE: &str = "resources/subjects_dir/subject1/surf/lh.thickness";
        let curv = read_curv(CURV_FILE).unwrap();

        assert_eq!(149244, curv.header.num_vertices);
        assert_eq!(298484, curv.header.num_faces);
        assert_eq!(1, curv.header.num_values_per_vertex);
        assert_eq!(149244, curv.data.len());        

        let mut curv_data_sorted = curv.data.to_vec();
        curv_data_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min: f32 = curv_data_sorted[0];
        let max: f32 = curv_data_sorted[curv_data_sorted.len() - 1];
        assert_abs_diff_eq!(0.0, min, epsilon = 1e-10);
        assert_abs_diff_eq!(5.0, max, epsilon = 1e-10);
    }
}
