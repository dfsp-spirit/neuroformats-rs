// Functions for managing FreeSurfer per-vertex data in binary 'curv' files.
// These files store 1 scalar value (typically a morphological descriptor, like cortical thickness at that point)
// for each vertex of the respective brain surface mesh.


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


// An FsCurv object
#[derive(Debug, PartialEq, Clone)]
pub struct FsCurv {
    header: CurvHeader,
    data: Vec<i32>, 
}


impl FsCurv {
    /// Read a Curv header from a file.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Curv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsCurv> {
        let gz = is_gz_file(&path);

        let hdr = CurvHeader::from_file(path).unwrap();

        let file = BufReader::new(File::open(path)?);


        let data: Vec<i32> = if gz { FsCurv::curv_data_from_reader(GzDecoder::new(file), &hdr) } else  { FsCurv::curv_data_from_reader(file, &hdr) };

        let curv = FsCurv { 
            header : hdr,
            data: data,
        };

        Ok(curv)
    }

    pub fn curv_data_from_reader<S>(input: S, hdr: &CurvHeader) -> Vec<i32>
    where
        S: Read,
    {
    
        let input = ByteOrdered::be(input);

        let hdr_size = 15;
        

        let mut input = ByteOrdered::be(input);

        let mut hdr_data : Vec<u8> = Vec::with_capacity(hdr_size as usize);
        for _ in 1..=hdr_size {
            hdr_data.push(input.read_u8().unwrap());
        }

        let mut data : Vec<i32> = Vec::with_capacity(hdr.num_vertices as usize);
        for _ in 1..=hdr.num_vertices {
            data.push(input.read_i32().unwrap());
        }
        data
    }
}



