//! Functions for managing FreeSurfer brain surface meshes in binary 'surf' files.
//!
//! These files store a triangular mesh, where each vertex is defined by its x,y,z coords and 
//! each face is defined by 3 vertices, stored as 3 row-indices into the vertices matrix.
//! These vertex indices are zero-based.


use byteordered::{ByteOrdered};

use std::{fs::File};
use std::io::{BufReader, BufRead, Read, Seek};
use std::path::{Path};

use crate::util::read_variable_length_string;
use crate::error::{NeuroformatsError, Result};


use ndarray::{Array, Array2};

pub const TRIS_MAGIC_FILE_TYPE_NUMBER: i32 = 16777214;
pub const TRIS_MAGIC_FILE_TYPE_NUMBER_ALTERNATIVE: i32 = 16777215;

/// Models the header of a FreeSurfer surf file containing a brain mesh.
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

/// The header of a FreeSurfer brain mesh file in surf format.
impl FsSurfaceHeader {
    
    /// Read an FsSurface header from a file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsSurfaceHeader> {
        let mut file = BufReader::new(File::open(path)?);
        FsSurfaceHeader::from_reader(&mut file)
    }


    /// Read an FsSurface header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// FsSurface header.
    pub fn from_reader<S>(input: &mut S) -> Result<FsSurfaceHeader>
    where
        S: Read + Seek,
    {
        let mut hdr = FsSurfaceHeader::default();
    
        let mut input = ByteOrdered::be(input);

        hdr.info_line = read_variable_length_string(&mut input)?;
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


/// An FsSurface object, models the contents (header and data) of a FreeSurfer surf file.
#[derive(Debug, PartialEq, Clone)]
pub struct FsSurface {
    pub header: FsSurfaceHeader,
    pub mesh: BrainMesh, 
}

/// A brain mesh, or any other triangular mesh. Vertices are stored as an `nx3` matrix of x,y,z coordinates. The triangular faces are stored as an `mx3` matrix of vertex indices.
#[derive(Debug, PartialEq, Clone)]
pub struct BrainMesh {
    pub vertices: Array2<f32>,
    pub faces: Array2<i32>, 
}


impl BrainMesh {
    /// Export a brain mesh to a Wavefront Object (OBJ) format string.
    pub fn to_obj(&self) -> String {
        let mut obj_repr = Vec::<String>::new();

        for vrow in self.vertices.genrows() {
            obj_repr.push(format!("v {} {} {}\n", vrow[0], vrow[1], vrow[2]));
        }

        for frow in self.faces.genrows() {
            obj_repr.push(format!("f {} {} {}\n", frow[0]+1, frow[1]+1, frow[2]+1));
        }
        
        let obj_repr = obj_repr.join("");
        obj_repr
    }

    /// Read a brain mesh from a Wavefront object format (.obj) mesh file.
    ///
    /// # Examples
    /// ```no_run
    /// let mesh = neuroformats::BrainMesh::from_obj_file("resources/mesh/cube.obj").unwrap();
    /// assert_eq!(24, mesh.vertices.len());
    /// ```
    pub fn from_obj_file<P: AsRef<Path>>(path: P) -> Result<BrainMesh> {
    
        let reader = BufReader::new(File::open(path)?);

        let mut vertex_data : Vec<f32> = Vec::new();
        let mut face_data : Vec<i32> = Vec::new();

        let mut num_vertices: i32 = 0;
        let mut num_faces: i32 = 0;

        // Read the file line by line using the lines() iterator from std::io::BufRead.
        for (_index, line) in reader.lines().enumerate() {

            let line = line?;
            let mut iter = line.split_whitespace();
    
            
            let entry_type = iter.next().unwrap().trim();
            if entry_type == "v" {
                num_vertices += 1;
                vertex_data.push(iter.next().unwrap().parse::<f32>().unwrap());
                vertex_data.push(iter.next().unwrap().parse::<f32>().unwrap());
                vertex_data.push(iter.next().unwrap().parse::<f32>().unwrap());
            } else if entry_type == "f" {
                num_faces += 1;
                face_data.push(iter.next().unwrap().parse::<i32>().unwrap());
                face_data.push(iter.next().unwrap().parse::<i32>().unwrap());
                face_data.push(iter.next().unwrap().parse::<i32>().unwrap());
            } else if entry_type == "#" {
                continue; // a comment line.
            } else {
                return Err(NeuroformatsError::InvalidWavefrontObjectFormat);
            }                
        }

        if num_vertices < 1 || num_faces < 1 {
            return Err(NeuroformatsError::EmptyWavefrontObjectFile);
        }

        let vertices = Array::from_shape_vec((num_vertices as usize, 3 as usize), vertex_data).unwrap();
        let faces = Array::from_shape_vec((num_faces as usize, 3 as usize), face_data).unwrap();

        let mesh = BrainMesh {
            vertices : vertices,
            faces : faces
        };
        Ok(mesh)
    }


}


/// Read an FsSurface instance from a file.
///
/// See [`crate::read_curv`] to read per-vertex data for the mesh.
///
/// # Examples
///
/// ```no_run
/// let surf = neuroformats::read_surf("/path/to/subjects_dir/subject1/surf/lh.white");
/// ```
pub fn read_surf<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
    FsSurface::from_file(path)
}


impl FsSurface {
    /// Read an FsSurface instance from a file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {

        let mut file = BufReader::new(File::open(path)?);

        let hdr = FsSurfaceHeader::from_reader(&mut file).unwrap();


        let mesh: BrainMesh = FsSurface::mesh_from_reader(&mut file, &hdr);

        let surf = FsSurface { 
            header : hdr,
            mesh: mesh,
        };

        Ok(surf)
    }


    /// Read a brain mesh, i.e., the data part of an FsSurface instance, from a reader.
    pub fn mesh_from_reader<S>(input: &mut S, hdr: &FsSurfaceHeader) -> BrainMesh
    where
        S: Read,
    {
    
        let mut input = ByteOrdered::be(input);

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

    #[test]
    fn the_tiny_demo_surf_file_can_be_exported_to_obj_format() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.tinysurface";
        let surf = read_surf(SURF_FILE).unwrap();

        assert_eq!(5, surf.header.num_vertices);
        assert_eq!(3, surf.header.num_faces);
    
        assert_eq!(5 * 3, surf.mesh.vertices.len());
        assert_eq!(3 * 3, surf.mesh.faces.len());

        let obj_repr: String = surf.mesh.to_obj();
        assert_eq!(String::from("v 0.3 0.3 0.3\nv 0.3 0.3 0.3\nv 0.3 0.3 0.3\nv 0.3 0.3 0.3\nv 0.3 0.3 0.3\nf 1 2 4\nf 2 4 5\nf 3 3 3\n"), obj_repr);

        // One could write the OBJ string to a file: 
        //     use std::fs;
        //     fs::write("/tmp/tinysurface.obj", obj_repr).expect("Unable to write file");
    }

    #[test]
    fn an_obj_file_can_be_parsed_into_a_brainmesh() {
        const OBJ_FILE: &str = "resources/mesh/cube.obj";
        let mesh = BrainMesh::from_obj_file(OBJ_FILE).unwrap();

        let known_vertex_count = 8;
        let known_face_count = 12;

        assert_eq!(known_vertex_count * 3, mesh.vertices.len());
        assert_eq!(known_face_count * 3, mesh.faces.len());
    }
}


