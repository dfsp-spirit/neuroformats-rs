//! Functions for managing FreeSurfer per-vertex data in binary 'curv' files.
//!
//! These files store 1 scalar value (typically a morphological descriptor, like cortical thickness at that point)
//! for each vertex of the respective brain surface mesh.


use byteordered::{ByteOrdered, Endianness};
use flate2::bufread::GzDecoder;

use std::fs::File;
use std::io::{BufReader, BufRead, BufWriter};
use std::path::{Path};
use std::fmt;

use crate::util::{is_gz_file, vec32minmax};
use crate::error::{NeuroformatsError, Result};


pub const CURV_MAGIC_CODE_U8: u8 = 255;

#[derive(Debug, Clone, PartialEq)]
pub struct FsCurvHeader {
    pub curv_magic: [u8; 3],
    pub num_vertices: i32,
    pub num_faces: i32,
    pub num_values_per_vertex: i32,
}


impl Default for FsCurvHeader {
    fn default() -> FsCurvHeader {
        FsCurvHeader {
            curv_magic: [255; 3],
            num_vertices: 0,
            num_faces: 0,
            num_values_per_vertex: 1,
        }
    }
}

impl FsCurvHeader {
    
    /// Read a Curv header from a file.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Curv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsCurvHeader> {
        let gz = is_gz_file(&path);
        let file = BufReader::new(File::open(path)?);
        if gz {
            FsCurvHeader::from_reader(BufReader::new(GzDecoder::new(file)))
        } else {
            FsCurvHeader::from_reader(file)
        }
    }


    /// Read a Curv header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// Curv header.
    pub fn from_reader<S>(input: S) -> Result<FsCurvHeader>
    where
        S: BufRead,
    {
        let mut hdr = FsCurvHeader::default();
    
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
    pub header: FsCurvHeader,
    pub data: Vec<f32>, 
}


impl fmt::Display for FsCurv {    
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {        
        write!(f, "Per-vertex data for {} vertices, with values in range {} to {}.", self.data.len(), vec32minmax(&self.data, false).0, vec32minmax(&self.data, false).1)
    }
}


/// Read per-vertex data from a FreeSurfer curv file.
///
/// A curv file assigns a single scalar value to each vertex of a brain mesh. These values can represent
/// anything, but the files are typically used to store morphological descriptors like the cortical thickness
/// at each point of the brain surface, or statistical results like t value maps. See [`crate::read_surf`] to load
/// the corresponding mesh file. 
///
/// # Examples
///
/// ```no_run
/// let curv = neuroformats::read_curv("/path/to/subjects_dir/subject1/surf/lh.thickness").unwrap();
/// let thickness_at_vertex_0 : f32 = curv.data[0];
/// ```
pub fn read_curv<P: AsRef<Path> + Copy>(path: P) -> Result<FsCurv> {
    FsCurv::from_file(path)
}

/// Write an FsCurv struct to a file in FreeSurfer curv format.
pub fn write_curv<P: AsRef<Path> + Copy>(path: P, curv : &FsCurv) {
    let f = File::create(path).expect("Unable to create curv file");
    let f = BufWriter::new(f);  
    let mut f  =  ByteOrdered::runtime(f, Endianness::Big); 
    f.write_u8(CURV_MAGIC_CODE_U8).unwrap();
    f.write_u8(CURV_MAGIC_CODE_U8).unwrap();
    f.write_u8(CURV_MAGIC_CODE_U8).unwrap();
    f.write_i32(curv.header.num_vertices).unwrap();
    f.write_i32(curv.header.num_faces).unwrap();
    f.write_i32(curv.header.num_values_per_vertex).unwrap();

    for v in &curv.data {
        f.write_f32(*v).unwrap();
    }
}


impl FsCurv {
    /// Read a Curvfile.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Curv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsCurv> {
        let gz = is_gz_file(&path);

        let hdr = FsCurvHeader::from_file(path)?;

        let file = BufReader::new(File::open(path)?);

        let data: Vec<f32> = if gz {
            FsCurv::curv_data_from_reader(BufReader::new(GzDecoder::new(file)), &hdr)
        } else {
            FsCurv::curv_data_from_reader(file, &hdr)
        };

        let curv = FsCurv { 
            header : hdr,
            data: data,
        };

        Ok(curv)
    }


    pub fn curv_data_from_reader<S>(input: S, hdr: &FsCurvHeader) -> Vec<f32>
    where
        S: BufRead,
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
    use tempfile::{tempdir};

    #[test]
    fn the_demo_curv_file_can_be_read() {
        const CURV_FILE: &str = "resources/subjects_dir/subject1/surf/lh.thickness";
        let curv = read_curv(CURV_FILE).unwrap();

        assert_eq!(149244, curv.header.num_vertices);
        assert_eq!(298484, curv.header.num_faces);
        assert_eq!(1, curv.header.num_values_per_vertex);
        assert_eq!(149244, curv.data.len());        

        use crate::util::vec32minmax;
        let (min, max) = vec32minmax(&curv.data, false);
        assert_abs_diff_eq!(0.0, min, epsilon = 1e-10);
        assert_abs_diff_eq!(5.0, max, epsilon = 1e-10);
    }

    #[test]
    fn a_curv_file_can_be_written_and_reread() {
        const CURV_FILE: &str = "resources/subjects_dir/subject1/surf/lh.thickness";
        let curv = read_curv(CURV_FILE).unwrap();

        let dir = tempdir().unwrap();

        let tfile_path = dir.path().join("temp-curv-file.curv");
        let tfile_path = tfile_path.to_str().unwrap();
        write_curv(tfile_path, &curv);

        let curv_re = read_curv(tfile_path).unwrap();

        assert_eq!(149244, curv_re.header.num_vertices);
        assert_eq!(298484, curv_re.header.num_faces);
        assert_eq!(1, curv_re.header.num_values_per_vertex);
        assert_eq!(149244, curv_re.data.len());        

        use crate::util::vec32minmax;
        let (min, max) = vec32minmax(&curv_re.data, false);
        assert_abs_diff_eq!(0.0, min, epsilon = 1e-10);
        assert_abs_diff_eq!(5.0, max, epsilon = 1e-10);
    }
}
