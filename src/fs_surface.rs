// Functions for managing FreeSurfer brain surface meshes in binary 'surf' files.
// These files store a triangular mesh, where each vertex if defined by its x,y,z coord and 
// each face is defined by 3 vertices, stored as 3 indices into the vertices.


use byteordered::{ByteOrdered};
use flate2::bufread::GzDecoder;

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path};

use crate::util::is_gz_file;
use crate::error::{NeuroformatsError, Result};


use ndarray::{prelude::*};

pub const TRIS_MAGIC_FILE_TYPE_NUMBER: i32 = 16777214;
pub const TRIS_MAGIC_FILE_TYPE_NUMBER_ALTERNATIVE: i32 = 16777215;


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
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsSurfaceHeader> {
        let file = BufReader::new(File::open(path)?);
        FsSurfaceHeader::from_reader(file)
    }


    /// Read an FsSurface header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// FsSurface header.
    pub fn from_reader<S>(input: S) -> Result<FsSurfaceHeader>
    where
        S: Read + Seek,
    {
        let mut hdr = FsSurfaceHeader::default();
    
        let mut input = ByteOrdered::be(input);

        let mut cur_char = input.read_u8()? as char;
        let mut info_line = String::from(cur_char);
        while cur_char != '\0' {
            cur_char = input.read_u8()? as char;
            info_line.push(cur_char);            
        }
        input.seek(SeekFrom::Current(-1))?;
    
        hdr.info_line = info_line;
        hdr.num_vertices = input.read_i32()?;
        hdr.num_faces = input.read_i32()?;
        
        let magic: i32 = interpret_fs_int24(hdr.surf_magic[0], hdr.surf_magic[1], hdr.surf_magic[2]);

        if !(magic == TRIS_MAGIC_FILE_TYPE_NUMBER || magic == TRIS_MAGIC_FILE_TYPE_NUMBER_ALTERNATIVE) {
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
    pub vertices: Array2<f32>,
    pub faces: Array2<i32>, 
}


pub fn read_surf<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
    FsSurface::from_file(path)
}


impl FsSurface {
    /// Read an FsSurface instance from a file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
        let gz = is_gz_file(&path);

        let hdr = FsSurfaceHeader::from_file(path).unwrap();

        println!("Hdr: magic = {}, {}, {}.", hdr.surf_magic[0], hdr.surf_magic[1], hdr.surf_magic[2]);
        println!("Hdr: info_line = {}.", hdr.info_line);
        println!("Hdr: num_v = {}, num_f = {}.", hdr.num_vertices, hdr.num_faces);

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

        let num_vert_coords: i32 = hdr.num_vertices * 3;
        let mut vertex_data : Vec<f32> = Vec::with_capacity(num_vert_coords as usize);
        for _ in 1..=hdr.num_vertices * 3 {
            vertex_data.push(input.read_f32().unwrap());
        }

        let vertices = Array::from_shape_vec((hdr.num_vertices as usize, 3 as usize), vertex_data).unwrap();

        let mut face_data : Vec<i32> = Vec::with_capacity((hdr.num_faces * 3) as usize);
        for _ in 1..=hdr.num_faces * 3 {
            face_data.push(input.read_i32().unwrap());
        }

        let faces = Array::from_shape_vec((hdr.num_faces as usize, 3 as usize), face_data).unwrap();

        let mesh = BrainMesh {
            vertices : vertices,
            faces : faces
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


