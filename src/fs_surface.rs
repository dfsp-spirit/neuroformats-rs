// Functions for managing FreeSurfer brain surface meshes in binary 'surf' files.
// These files store a triangular mesh, where each vertex if defined by its x,y,z coord and 
// each face is defined by 3 vertices, stored as 3 indices into the vertices.


use byteordered::{ByteOrdered};
use flate2::bufread::GzDecoder;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path};

use crate::util::is_gz_file;
use crate::error::{NeuroformatsError, Result};

pub const TRIS_MAGIC_FILE_TYPE_NUMBER: i32 = 16777214;

#[derive(Debug, Clone, PartialEq)]
pub struct FsSurfaceHeader {
    pub surf_magic: [u8; 3],
    pub info_line: String,
    pub num_vertices: i32,
    pub num_faces: i32,
}


impl Default for FsSurfaceHeader {
    fn default() -> FsSurfaceHeader {
        FsSurfaceHeader {
            surf_magic: [255; 3],
            info_line: String::from(""),
            num_vertices: 0,
            num_faces: 0
        }
    }
}

impl FsSurfaceHeader {
    
    /// Read an FsSurface header from a file.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Surv files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsSurfaceHeader> {
        let gz = is_gz_file(&path);
        let file = BufReader::new(File::open(path)?);
        if gz {
            FsSurfaceHeader::from_reader(GzDecoder::new(file))
        } else {
            FsSurfaceHeader::from_reader(file)
        }
    }


    /// Read an FsSurface header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// FsSurface header.
    pub fn from_reader<S>(input: S) -> Result<FsSurfaceHeader>
    where
        S: Read,
    {
        let mut hdr = FsSurfaceHeader::default();
    
        let mut input = ByteOrdered::be(input);

        let mut cur_char = input.read_u8()? as char;
        let mut info_line = String::from(cur_char);
        while cur_char != '\0' {
            cur_char = input.read_u8()? as char;
            info_line.push(cur_char);
        }
    
        hdr.info_line = info_line;
        hdr.num_vertices = input.read_i32()?;
        hdr.num_faces = input.read_i32()?;
        
        let magic: i32 = interpret_fs_int24(hdr.surf_magic[0], hdr.surf_magic[1], hdr.surf_magic[2]);

        if magic != TRIS_MAGIC_FILE_TYPE_NUMBER {
            Err(NeuroformatsError::InvalidFsSurfaceFormat)
        } else {
            Ok(hdr)
        }
    }
}


/// Interpret three bytes as a single 24 bit integer, FreeSurfer style.
pub fn interpret_fs_int24(b1: u8, b2:u8, b3:u8) -> i32 {
    let c1 = (b1 as u32).checked_shl(16).unwrap_or(0);
    let c2 = (b2 as u32).checked_shl(8).unwrap_or(0);
    let c3 = b3 as i32;

    let fs_int24: i32 = c1 as i32 + c2 as i32 + c3;
    fs_int24
}


// An FsSurface object
#[derive(Debug, PartialEq, Clone)]
pub struct FsSurface {
    pub header: FsSurfaceHeader,
    pub mesh: BrainMesh, 
}

// A Brain Mesh
#[derive(Debug, PartialEq, Clone)]
pub struct BrainMesh {
    pub vertices: Vec<f32>,
    pub faces: Vec<i32>, 
}


pub fn read_surf<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
    FsSurface::from_file(path)
}


impl FsSurface {
    /// Read an FsSurface instance from a file.
    /// If the file's name ends with ".gz", the file is assumed to need GZip decoding. This is not typically the case
    /// for FreeSurfer Surface files, but very handy (and it helps us to reduce the size of our test data).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
        let gz = is_gz_file(&path);

        let hdr = FsSurfaceHeader::from_file(path).unwrap();

        let file = BufReader::new(File::open(path)?);


        let mesh: BrainMesh = if gz { FsSurface::mesh_from_reader(GzDecoder::new(file), &hdr) } else  { FsSurface::mesh_from_reader(file, &hdr) };

        let surf = FsSurface { 
            header : hdr,
            mesh: mesh,
        };

        Ok(surf)
    }

    pub fn mesh_from_reader<S>(input: S, hdr: &FsSurfaceHeader) -> BrainMesh
    where
        S: Read,
    {
    
        let input = ByteOrdered::be(input);

        let hdr_size = 3 + hdr.info_line.len() + 4 + 4;
        

        let mut input = ByteOrdered::be(input);

        // This is only read because we cannot seek in a GZ stream.
        let mut hdr_data : Vec<u8> = Vec::with_capacity(hdr_size as usize);
        for _ in 1..=hdr_size {
            hdr_data.push(input.read_u8().unwrap());
        }

        let mut vertex_data : Vec<f32> = Vec::with_capacity((hdr.num_vertices * 3) as usize);
        for _ in 1..=hdr.num_vertices * 3 {
            vertex_data.push(input.read_f32().unwrap());
        }

        let mut face_data : Vec<i32> = Vec::with_capacity((hdr.num_faces * 3) as usize);
        for _ in 1..=hdr.num_faces * 3 {
            face_data.push(input.read_i32().unwrap());
        }

        let mesh = BrainMesh {
            vertices : vertex_data,
            faces : face_data
        };

        mesh
    }
}


#[cfg(test)]
mod test { 
    use super::*;

    #[test]
    fn the_demo_surf_file_can_be_read() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        assert_eq!(149244, surf.header.num_vertices);
        assert_eq!(298484, surf.header.num_faces);
    
        assert_eq!(149244 * 3, surf.mesh.vertices.len());
        assert_eq!(298484 * 3, surf.mesh.faces.len());
    }
}


