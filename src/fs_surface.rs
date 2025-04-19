//! Functions for managing FreeSurfer brain surface meshes in binary 'surf' files.
//!
//! Surf files store a triangular mesh, where each vertex is defined by its x,y,z coords and
//! each face is defined by 3 vertices, stored as 3 row-indices into the vertices matrix.
//! These vertex indices are zero-based.

use byteordered::{ByteOrdered, Endianness};

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::error::{NeuroformatsError, Result};
use crate::read_curv;
use crate::util::read_fs_variable_length_string;
use crate::util::values_to_colors;
use crate::util::vec32minmax;

use base64::{engine::general_purpose, Engine as _}; // WTF?! this is required for the absurd general_purpose::STANDARD_NO_PAD.encode() below, see https://www.reddit.com/r/programmingcirclejerk/comments/16zkmnl/base64s_rust_create_maintainer_bravely_defends/?rdt=55288

use serde_json::json;

use ndarray::{array, s, Array2};
use ndarray_stats::QuantileExt;

pub const TRIS_MAGIC_FILE_TYPE_NUMBER: i32 = 16777214;

/// Models the header of a FreeSurfer surf file containing a brain mesh. Note that the `info_line` must contain only ASCII chars and end with two Unix EOLs, `\n\n`.
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
            surf_magic: [255, 255, 254],
            info_line: String::from("A brain surface.\n\n"),
            num_vertices: 0,
            num_faces: 0,
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
        S: BufRead,
    {
        let mut hdr = FsSurfaceHeader::default();

        let mut input = ByteOrdered::be(input);

        hdr.surf_magic[0] = input.read_u8()?;
        hdr.surf_magic[1] = input.read_u8()?;
        hdr.surf_magic[2] = input.read_u8()?;
        hdr.info_line = read_fs_variable_length_string(&mut input)?;
        hdr.num_vertices = input.read_i32()?;
        hdr.num_faces = input.read_i32()?;

        let magic: i32 =
            interpret_fs_int24(hdr.surf_magic[0], hdr.surf_magic[1], hdr.surf_magic[2]);

        if !(magic == TRIS_MAGIC_FILE_TYPE_NUMBER) {
            Err(NeuroformatsError::InvalidFsSurfaceFormat)
        } else {
            Ok(hdr)
        }
    }
}

/// Compute the min and max coordinates for the x, y, and z axes.
///
/// # Panics
///
/// If the min and max coordinates for the axes cannot be computed. E.g., when the coordinate vector is empty, has a length that is not a multiple of 3, or contains invalid vertex coordinates like `NAN` values.
///
/// # Return value
///
/// The 6 values in the returned tuple are, in the following order: (min_x, max_x, min_y, max_y, min_z, max_z).
///
/// Examples
///
/// ```no_run
/// let coords: Vec<f32> = vec![0.0, 0.1, 0.2, 0.3, 0.3, 0.3, 1.0, 2.0, 4.0];
/// let (minx, maxx, miny, maxy, minz, maxz) = neuroformats::fs_surface::coord_extrema(&coords).unwrap();
/// assert_eq!(0.0, minx);
/// assert_eq!(0.1, miny);
/// assert_eq!(0.2, minz);
/// assert_eq!(1.0, maxx);
/// assert_eq!(2.0, maxy);
/// assert_eq!(4.0, maxz);
/// ```
pub fn coord_extrema(coords: &Vec<f32>) -> Result<(f32, f32, f32, f32, f32, f32)> {
    let all_coords =
        Array2::from_shape_vec((coords.len() / 3 as usize, 3 as usize), coords.clone()).unwrap();
    let x_coords = all_coords.slice(s![.., 0]);
    let y_coords = all_coords.slice(s![.., 1]);
    let z_coords = all_coords.slice(s![.., 2]);

    let min_x = x_coords.min().unwrap().clone(); // min() on type ndarray::ArrayBase is available from ndarray-stats Quantile trait
    let max_x = x_coords.max().unwrap().clone();

    let min_y = y_coords.min().unwrap().clone();
    let max_y = y_coords.max().unwrap().clone();

    let min_z = z_coords.min().unwrap().clone();
    let max_z = z_coords.max().unwrap().clone();

    Ok((min_x, max_x, min_y, max_y, min_z, max_z))
}

/// Compute the center of the given coordinates.
///
/// The center is simply the mean of the min and max values for the x, y and z axes. So this is NOT the center of mass.
///
/// # Panics
///
/// If the `mean` of the min and max coordinates cannot be computed. E.g., when coords vector is empty, has a length that is not a multiple of 3, or contains invalid vertex coordinates like `NAN` values.
///
/// # Return value
///
/// The 3 values in the returned tuple are the x, y and z coordinates of the center, in that order.
///
/// # Examples
///
/// ```no_run
/// let coords: Vec<f32> = vec![0.0, 0.0, 0.0, 0.1, 0.1, 0.1, 1.0, 2.0, 4.0];
/// let (cx, cy, cz) = neuroformats::fs_surface::coord_center(&coords).unwrap();
/// assert_eq!(0.5, cx);
/// assert_eq!(1.0, cy);
/// assert_eq!(2.0, cz);
/// ```
pub fn coord_center(coords: &Vec<f32>) -> Result<(f32, f32, f32)> {
    let (min_x, max_x, min_y, max_y, min_z, max_z) = coord_extrema(coords)?;
    let cx = array![min_x, max_x]
        .mean()
        .expect("Could not compute mean for x coords.");
    let cy = array![min_y, max_y]
        .mean()
        .expect("Could not compute mean for y coords.");
    let cz = array![min_z, max_z]
        .mean()
        .expect("Could not compute mean for z coords.");
    Ok((cx, cy, cz))
}

/// Interpret three bytes as a single 24 bit integer, FreeSurfer style.
pub fn interpret_fs_int24(b1: u8, b2: u8, b3: u8) -> i32 {
    let c1 = (b1 as u32).checked_shl(16).unwrap_or(0);
    let c2 = (b2 as u32).checked_shl(8).unwrap_or(0);
    let c3 = b3 as i32;

    let fs_int24: i32 = c1 as i32 + c2 as i32 + c3;
    fs_int24
}

/// Write an FsSurface struct to a file in FreeSurfer surf format.
pub fn write_surf<P: AsRef<Path> + Copy>(path: P, surf: &FsSurface) -> std::io::Result<()> {
    let f = File::create(path)?;
    let f = BufWriter::new(f);
    let mut f = ByteOrdered::runtime(f, Endianness::Big);
    f.write_u8(surf.header.surf_magic[0])?;
    f.write_u8(surf.header.surf_magic[1])?;
    f.write_u8(surf.header.surf_magic[2])?;

    // Write the info line. It is a byte string that ends with 2 Unix linefeeds '\n' or '\x0A' (decimal 10). There is NOT any string terminator (no NUL byte).
    f.write(surf.header.info_line.as_bytes())?;
    f.write_i32(surf.header.num_vertices)?;
    f.write_i32(surf.header.num_faces)?;

    for v in surf.mesh.vertices.iter() {
        f.write_f32(*v)?;
    }
    for v in surf.mesh.faces.iter() {
        f.write_i32(*v)?;
    }

    Ok(())
}

/// An FsSurface object, models the contents (header and data) of a FreeSurfer surf file.
#[derive(Debug, PartialEq, Clone)]
pub struct FsSurface {
    pub header: FsSurfaceHeader,
    pub mesh: BrainMesh,
}

/// A brain mesh, or any other triangular mesh. Vertices are stored as a vector of x,y,z coordinates, where triplets of coordinates represent a vertex. The triangular faces are stored in the same way as a vector of vertex indices.
#[derive(Debug, PartialEq, Clone)]
pub struct BrainMesh {
    pub vertices: Vec<f32>,
    pub faces: Vec<i32>,
}

impl BrainMesh {
    /// Export a brain mesh to a Wavefront Object (OBJ) format string.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let surf = neuroformats::read_surf("/path/to/subjects_dir/subject1/surf/lh.white").unwrap();
    /// let obj_repr = surf.mesh.to_obj();
    /// std::fs::write("/tmp/lhwhite.obj", obj_repr).expect("Unable to write OBJ mesh file");
    /// ```
    pub fn to_obj(&self) -> String {
        let mut obj_repr = Vec::<String>::new();

        let vertices = Array2::from_shape_vec(
            (self.vertices.len() / 3 as usize, 3 as usize),
            self.vertices.clone(),
        )
        .unwrap();
        let faces = Array2::from_shape_vec(
            (self.faces.len() / 3 as usize, 3 as usize),
            self.faces.clone(),
        )
        .unwrap();

        for vrow in vertices.rows() {
            obj_repr.push(format!("v {} {} {}\n", vrow[0], vrow[1], vrow[2]));
        }

        for frow in faces.rows() {
            obj_repr.push(format!(
                "f {} {} {}\n",
                frow[0] + 1,
                frow[1] + 1,
                frow[2] + 1
            ));
        }

        let obj_repr = obj_repr.join("");
        obj_repr
    }

    /// Export a brain mesh to PLY (Polygon File Format) format string.
    ///
    /// # Arguments
    /// * `vertex_colors` - Optional vertex colors as RGB values in [r,g,b, r,g,b, ...] format.
    ///                    Must be exactly 3 times the number of vertices if provided.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let surf = neuroformats::read_surf("/path/to/subject/surf/lh.white").unwrap();
    /// let colors = vec![255; surf.mesh.vertices.len()]; // White colors for all vertices
    /// let ply_repr = surf.mesh.to_ply(Some(&colors));
    /// std::fs::write("/tmp/lhwhite.ply", ply_repr).expect("Unable to write PLY mesh file");
    /// ```
    pub fn to_ply(&self, vertex_colors: Option<&[u8]>) -> String {
        let vertex_count: usize = self.vertices.len() / 3;
        let face_count: usize = self.faces.len() / 3;

        // Validate vertex colors if provided
        if let Some(colors) = vertex_colors {
            assert_eq!(
                colors.len(),
                vertex_count * 3,
                "Vertex colors array must have exactly 3 values per vertex"
            );
        }

        let mut ply_lines: Vec<String> = Vec::new();

        // Header
        ply_lines.push("ply".to_string());
        ply_lines.push("format ascii 1.0".to_string());
        ply_lines.push(format!("element vertex {}", vertex_count));
        ply_lines.push("property float x".to_string());
        ply_lines.push("property float y".to_string());
        ply_lines.push("property float z".to_string());

        if vertex_colors.is_some() {
            ply_lines.push("property uchar red".to_string());
            ply_lines.push("property uchar green".to_string());
            ply_lines.push("property uchar blue".to_string());
        }

        ply_lines.push(format!("element face {}", face_count));
        ply_lines.push("property list uchar int vertex_indices".to_string());
        ply_lines.push("end_header".to_string());

        // Vertex data
        for i in 0..vertex_count {
            let x: f32 = self.vertices[i * 3];
            let y: f32 = self.vertices[i * 3 + 1];
            let z: f32 = self.vertices[i * 3 + 2];

            let mut vertex_line: String = format!("{} {} {}", x, y, z);

            if let Some(colors) = vertex_colors {
                let r = colors[i * 3];
                let g = colors[i * 3 + 1];
                let b = colors[i * 3 + 2];
                vertex_line.push_str(&format!(" {} {} {}", r, g, b));
            }

            ply_lines.push(vertex_line);
        }

        // Face data
        for i in 0..face_count {
            let a = self.faces[i * 3];
            let b = self.faces[i * 3 + 1];
            let c = self.faces[i * 3 + 2];
            ply_lines.push(format!("3 {} {} {}", a, b, c));
        }

        ply_lines.join("\n") + "\n"
    }

    pub fn to_gltf(&self, vertex_colors: Option<&[u8]>) -> String {
        let vertex_count = self.vertices.len() / 3;

        // Validate all indices are within bounds
        if let Some(invalid_idx) = self
            .faces
            .iter()
            .find(|&&i| i < 0 || i as usize >= vertex_count)
        {
            panic!(
                "Invalid face index {} (vertex count: {})",
                invalid_idx, vertex_count
            );
        }

        // Convert to u32 indices (glTF requirement)
        let face_indices: Vec<u32> = self.faces.iter().map(|&i| i as u32).collect();

        // Constants
        const GLTF_TYPE_FLOAT32: i32 = 5126;
        const GLTF_TYPE_UINT32: i32 = 5125;
        const GLTF_TYPE_UBYTE: i32 = 5121;
        const GLTF_BUFFERTYPE_ARRAY_BUFFER: i32 = 34962;
        const GLTF_BUFFERTYPE_ELEMENT_ARRAY_BUFFER: i32 = 34963;

        // Create vertex color buffer (default to white if not provided)
        // Handle vertex colors - create owned vec if not provided
        let default_colors: Vec<u8>;
        let colors = match vertex_colors {
            Some(colors) => {
                assert_eq!(
                    colors.len(),
                    vertex_count * 3,
                    "Vertex colors must have exactly 3 components (RGB) per vertex"
                );
                colors
            }
            None => {
                default_colors = vec![0; vertex_count * 3]; // TODO: change 0=black back to 255=white
                &default_colors
            }
        };

        let mut rgba_buffer = Vec::with_capacity(colors.len() / 3 * 4);
        for chunk in colors.chunks_exact(3) {
            rgba_buffer.extend(chunk); // R, G, B
            rgba_buffer.push(255); // A=255 (fully opaque)
        }

        let cb = rgba_buffer.clone();

        // Create a normalized float32 version of the color buffer
        /*let color_buffer: Vec<u8> = color_buffer
            .chunks_exact(3)
            .flat_map(|chunk| {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;
                vec![r.to_le_bytes(), g.to_le_bytes(), b.to_le_bytes()].concat()
            })
            .collect();
        */

        // Create other buffers
        let vertex_buffer: Vec<u8> = self.vertices.iter().flat_map(|v| v.to_le_bytes()).collect();

        let index_buffer: Vec<u8> = face_indices.iter().flat_map(|i| i.to_le_bytes()).collect();

        // Calculate buffer sizes
        let vertex_buffer_len = vertex_buffer.len() as u32;
        let index_buffer_len = index_buffer.len() as u32;
        let color_buffer_len = rgba_buffer.len() as u32;

        // Combine buffers in correct order
        let mut binary_data = index_buffer;
        binary_data.extend(vertex_buffer);
        binary_data.extend(rgba_buffer);

        // Base64 encode
        let buffer_uri = format!(
            "data:application/octet-stream;base64,{}",
            general_purpose::STANDARD_NO_PAD.encode(&binary_data) // This API, WTF?! this should be base64::encode() without imports, but see https://www.reddit.com/r/programmingcirclejerk/comments/16zkmnl/base64s_rust_create_maintainer_bravely_defends/?rdt=55288
        );

        // Calculate bounds
        let (min_pos, max_pos) = {
            let mut min = [f32::MAX; 3];
            let mut max = [f32::MIN; 3];
            for chunk in self.vertices.chunks_exact(3) {
                for i in 0..3 {
                    min[i] = min[i].min(chunk[i]);
                    max[i] = max[i].max(chunk[i]);
                }
            }
            (
                min.iter().map(|&v| v as f64).collect::<Vec<_>>(),
                max.iter().map(|&v| v as f64).collect::<Vec<_>>(),
            )
        };

        // Calculate min and max for color buffer for r, g, b, a. They should be between 0 and 255 and returned as Vec<u8>
        let (min_color, max_color) = {
            let mut min = [u8::MAX; 4]; // Start with highest possible u8 value
            let mut max = [u8::MIN; 4]; // Start with lowest possible u8 value

            for chunk in cb.chunks_exact(4) {
                for i in 0..4 {
                    min[i] = min[i].min(chunk[i]);
                    max[i] = max[i].max(chunk[i]);
                }
            }

            // Convert to Vec<u8> (though arrays would work fine too)
            (min.to_vec(), max.to_vec())
        };

        // Build JSON structure
        let mut buffer_views = vec![
            json!({
                "buffer": 0,
                "byteOffset": 0,
                "byteLength": index_buffer_len,
                "target": GLTF_BUFFERTYPE_ELEMENT_ARRAY_BUFFER
            }),
            json!({
                "buffer": 0,
                "byteOffset": index_buffer_len,
                "byteLength": vertex_buffer_len,
                "target": GLTF_BUFFERTYPE_ARRAY_BUFFER
            }),
        ];

        let mut accessors = vec![
            json!({
                "bufferView": 0,
                "byteOffset": 0,
                "componentType": GLTF_TYPE_UINT32,
                "count": face_indices.len() as u32,
                "type": "SCALAR",
                "max": [*face_indices.iter().max().unwrap_or(&0) as f64],
                "min": [*face_indices.iter().min().unwrap_or(&0) as f64]
            }),
            json!({
                "bufferView": 1,
                "byteOffset": 0,
                "componentType": GLTF_TYPE_FLOAT32,
                "count": vertex_count as u32,
                "type": "VEC3",
                "max": max_pos,
                "min": min_pos
            }),
        ];

        // Add color buffer view and accessor if colors exist
        let mut attributes = json!({ "POSITION": 1 });

        if vertex_colors.is_some() {
            buffer_views.push(json!({
                "buffer": 0,
                "byteOffset": index_buffer_len + vertex_buffer_len,
                "byteLength": color_buffer_len,
                "target": GLTF_BUFFERTYPE_ARRAY_BUFFER
            }));

            accessors.push(json!({
                "bufferView": 2,
                "byteOffset": 0,
                "componentType": GLTF_TYPE_UBYTE,
                "count": vertex_count as u32,
                "type": "VEC4",
                "normalized": true,
                "min": min_color,
                "max": max_color
            }));

            attributes["COLOR_0"] = 2.into();
        }

        let gltf = json!({
            "asset": { "version": "2.0", "generator": "BrainMesh" },
            "scenes": [{ "nodes": [0] }],
            "nodes": [{ "mesh": 0 }],
            "meshes": [{
                "primitives": [{
                    "attributes": attributes,
                    "indices": 0,
                    "mode": 4
                }]
            }],
            "buffers": [{
                "uri": buffer_uri,
                "byteLength": index_buffer_len + vertex_buffer_len + color_buffer_len
            }],
            "bufferViews": buffer_views,
            "accessors": accessors
        });

        serde_json::to_string_pretty(&gltf).expect("Failed to serialize glTF JSON")
    }

    /// Get the number of vertices for this mesh.
    pub fn num_vertices(&self) -> usize {
        self.vertices.len() / 3
    }

    /// Get the number of faces (or polygons) for this mesh.
    pub fn num_faces(&self) -> usize {
        self.faces.len() / 3
    }

    /// Read a brain mesh from a Wavefront object format (.obj) mesh file.
    ///
    /// # Examples
    /// ```no_run
    /// let mesh = neuroformats::BrainMesh::from_obj_file("resources/mesh/cube.obj").unwrap();
    /// assert_eq!(24, mesh.vertices.len());
    /// ```
    pub fn from_obj_file<P: AsRef<Path>>(path: P) -> Result<BrainMesh> {
        let reader: BufReader<File> = BufReader::new(File::open(path)?);

        let mut vertex_data: Vec<f32> = Vec::new();
        let mut face_data: Vec<i32> = Vec::new();

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
                continue; // Ignore comment lines.
            } else {
                return Err(NeuroformatsError::InvalidWavefrontObjectFormat);
            }
        }

        if num_vertices < 1 || num_faces < 1 {
            return Err(NeuroformatsError::EmptyWavefrontObjectFile);
        }

        let mesh = BrainMesh {
            vertices: vertex_data,
            faces: face_data,
        };
        Ok(mesh)
    }

    /// Compute the min and max coordinates for the x, y, and z axes of the mesh.
    ///
    /// # Panics
    ///
    /// If the min and max coordinates for the axes cannot be computed. E.g., when the mesh contains no vertices or invalid vertex coordinates like NAN values.
    ///
    /// # Return value
    ///
    /// The 6 values in the returned tuple are, in the following order: (min_x, max_x, min_y, max_y, min_z, max_z).
    pub fn axes_min_max_coords(&self) -> Result<(f32, f32, f32, f32, f32, f32)> {
        coord_extrema(&self.vertices)
    }

    /// Compute the center of the mesh.
    ///
    /// The center is simply the mean of the min and max values for the x, y and z axes. So this is NOT the center of mass.
    ///
    /// # Panics
    ///
    /// If the `mean` of the min and max coordinates cannot be computed. E.g., when the mesh contains no vertices or invalid vertex coordinates like NAN values.
    ///
    /// # Return value
    ///
    /// The 3 values in the returned tuple are the x, y and z coordinates of the center, in that order.
    pub fn center(&self) -> Result<(f32, f32, f32)> {
        coord_center(&self.vertices)
    }
}

impl fmt::Display for BrainMesh {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Brain trimesh with {} vertices and {} faces.",
            self.vertices.len() / 3,
            self.faces.len() / 3
        )
    }
}

/// Read an FsSurface instance from a file.
///
/// Surf files store a triangular mesh, where each vertex is defined by its x,y,z coords and
/// each face is defined by 3 vertices, stored as 3 row-indices into the vertices matrix.
/// These vertex indices are zero-based. The mesh typically represents a single brain hemisphere.
///
/// See [`crate::read_curv`] to read per-vertex data for the mesh and [`crate::read_annot`] to
/// read atlas-based parcellations.
///
/// # Examples
///
/// ```no_run
/// let surf = neuroformats::read_surf("/path/to/subjects_dir/subject1/surf/lh.white").unwrap();
/// let num_verts = surf.mesh.vertices.len();
/// ```
pub fn read_surf<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
    FsSurface::from_file(path)
}

impl FsSurface {
    /// Read an FsSurface instance from a file in FreeSurfer surf format.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsSurface> {
        let mut file = BufReader::new(File::open(path)?);

        let hdr = FsSurfaceHeader::from_reader(&mut file).unwrap();

        let mesh: BrainMesh = FsSurface::mesh_from_reader(&mut file, &hdr);

        let surf = FsSurface {
            header: hdr,
            mesh: mesh,
        };

        Ok(surf)
    }

    /// Generate vertex colors for this mesh from the per-vertex values in a FreeSurfer curv file.
    /// This is a convenience function that reads the curv file and generates a color vector for the mesh.
    /// It also checks that the number of colors matches the number of vertices in the mesh.
    /// Arguments:
    /// * `path` - The path to the curv file.
    /// Returns a vector of colors in [r,g,b, r,g,b, ...] format.
    pub fn colors_from_curv_file<P: AsRef<Path> + Copy>(&self, path: P) -> Result<Vec<u8>> {
        let curv = read_curv(path)?;
        let (min, max) = vec32minmax(curv.data.clone().into_iter(), true);
        let colors: Vec<u8> = values_to_colors(&curv.data.clone(), min, max);

        // verify that the number of colors * 3 matches the number of vertices (R,G,B for each vertex)
        if (colors.len() / 3) != self.mesh.num_vertices() {
            Err(NeuroformatsError::VertexColorCountMismatch)
        } else {
            Ok(colors)
        }
    }

    /// Read a brain mesh, i.e., the data part of an FsSurface instance, from a reader.
    pub fn mesh_from_reader<S>(input: &mut S, hdr: &FsSurfaceHeader) -> BrainMesh
    where
        S: BufRead,
    {
        let mut input = ByteOrdered::be(input);

        let num_vert_coords: i32 = hdr.num_vertices * 3;
        let mut vertex_data: Vec<f32> = Vec::with_capacity(num_vert_coords as usize);
        for _ in 1..=hdr.num_vertices * 3 {
            vertex_data.push(input.read_f32().unwrap());
        }

        //let vertices = Array::from_shape_vec((hdr.num_vertices as usize, 3 as usize), vertex_data).unwrap();

        let mut face_data: Vec<i32> = Vec::with_capacity((hdr.num_faces * 3) as usize);
        for _ in 1..=hdr.num_faces * 3 {
            face_data.push(input.read_i32().unwrap());
        }

        //let faces = Array::from_shape_vec((hdr.num_faces as usize, 3 as usize), face_data).unwrap();

        let mesh = BrainMesh {
            vertices: vertex_data,
            faces: face_data,
        };

        mesh
    }
}

impl fmt::Display for FsSurface {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FreeSurfer Brain trimesh with {} vertices and {} faces.",
            self.mesh.vertices.len() / 3,
            self.mesh.faces.len() / 3
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;
    use tempfile::tempdir;

    #[test]
    fn the_demo_surf_file_can_be_read() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        assert_eq!(255 as u8, surf.header.surf_magic[0]);
        assert_eq!(255 as u8, surf.header.surf_magic[1]);
        assert_eq!(254 as u8, surf.header.surf_magic[2]);

        assert_eq!(149244, surf.header.num_vertices);
        assert_eq!(298484, surf.header.num_faces);

        assert_eq!(149244 * 3, surf.mesh.vertices.len());
        assert_eq!(298484 * 3, surf.mesh.faces.len());
    }

    #[test]
    fn the_center_and_min_max_coords_of_a_brainmesh_can_be_computed() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        let expected_min_max: (f32, f32, f32, f32, f32, f32) = (
            -60.6363, 5.589893, -108.62039, 58.73302, -8.280799, 106.17429,
        );

        assert_abs_diff_eq!(
            expected_min_max.0,
            surf.mesh.axes_min_max_coords().unwrap().0,
            epsilon = 1e-8
        );
        assert_abs_diff_eq!(
            expected_min_max.1,
            surf.mesh.axes_min_max_coords().unwrap().1,
            epsilon = 1e-8
        );
        assert_abs_diff_eq!(
            expected_min_max.2,
            surf.mesh.axes_min_max_coords().unwrap().2,
            epsilon = 1e-8
        );
        assert_abs_diff_eq!(
            expected_min_max.3,
            surf.mesh.axes_min_max_coords().unwrap().3,
            epsilon = 1e-8
        );
        assert_abs_diff_eq!(
            expected_min_max.4,
            surf.mesh.axes_min_max_coords().unwrap().4,
            epsilon = 1e-8
        );
        assert_abs_diff_eq!(
            expected_min_max.5,
            surf.mesh.axes_min_max_coords().unwrap().5,
            epsilon = 1e-8
        );

        let expected_center: (f32, f32, f32) = (-27.523203, -24.943686, 48.946747);
        let (cx, cy, cz) = surf.mesh.center().unwrap();
        assert_abs_diff_eq!(expected_center.0, cx, epsilon = 1e-8);
        assert_abs_diff_eq!(expected_center.1, cy, epsilon = 1e-8);
        assert_abs_diff_eq!(expected_center.2, cz, epsilon = 1e-8);
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
    }

    #[test]
    fn the_tiny_demo_surf_file_can_be_exported_to_ply_format_without_colors() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.tinysurface";
        let surf = read_surf(SURF_FILE).unwrap();

        assert_eq!(5, surf.header.num_vertices);
        assert_eq!(3, surf.header.num_faces);

        assert_eq!(5 * 3, surf.mesh.vertices.len());
        assert_eq!(3 * 3, surf.mesh.faces.len());

        let ply_repr: String = surf.mesh.to_ply(None);
        assert_eq!(String::from("ply\nformat ascii 1.0\nelement vertex 5\nproperty float x\nproperty float y\nproperty float z\nelement face 3\nproperty list uchar int vertex_indices\nend_header\n0.3 0.3 0.3\n0.3 0.3 0.3\n0.3 0.3 0.3\n0.3 0.3 0.3\n0.3 0.3 0.3\n3 0 1 3\n3 1 3 4\n3 2 2 2\n"), ply_repr);
    }

    #[test]
    fn the_tiny_demo_surf_file_can_be_exported_to_ply_format_with_colors() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.tinysurface";
        let surf = read_surf(SURF_FILE).unwrap();

        assert_eq!(5, surf.header.num_vertices);
        assert_eq!(3, surf.header.num_faces);

        assert_eq!(5 * 3, surf.mesh.vertices.len());
        assert_eq!(3 * 3, surf.mesh.faces.len());

        let colors = vec![
            255, 0, 0, // Red for vertex 0
            0, 255, 0, // Green for vertex 1
            0, 0, 255, // Blue for vertex 2
            255, 255, 0, // Yellow for vertex 3
            255, 0, 255, // Magenta for vertex 4
        ];

        let ply_repr: String = surf.mesh.to_ply(Some(&colors));
        assert_eq!(String::from("ply\nformat ascii 1.0\nelement vertex 5\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 3\nproperty list uchar int vertex_indices\nend_header\n0.3 0.3 0.3 255 0 0\n0.3 0.3 0.3 0 255 0\n0.3 0.3 0.3 0 0 255\n0.3 0.3 0.3 255 255 0\n0.3 0.3 0.3 255 0 255\n3 0 1 3\n3 1 3 4\n3 2 2 2\n"), ply_repr);
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

    #[test]
    fn the_coord_center_can_be_computed() {
        let coords: Vec<f32> = vec![
            0.0, 0.0, 0.0, 0.1, 0.1, 0.1, 0.5, 0.5, 0.5, 0.9, 0.9, 0.9, 0.95, 0.95, 0.95, 1.0, 2.0,
            4.0,
        ];
        let (cx, cy, cz) = crate::fs_surface::coord_center(&coords).unwrap();
        assert_eq!(0.5, cx);
        assert_eq!(1.0, cy);
        assert_eq!(2.0, cz);
    }

    #[test]
    fn the_coord_extrema_can_be_computed() {
        let coords: Vec<f32> = vec![
            0.0, 0.1, 0.2, 0.3, 0.3, 0.3, 0.5, 0.5, 0.5, 0.9, 0.9, 0.9, 0.95, 0.95, 0.95, 1.0, 2.0,
            4.0,
        ];
        let (minx, maxx, miny, maxy, minz, maxz) =
            crate::fs_surface::coord_extrema(&coords).unwrap();
        assert_eq!(0.0, minx);
        assert_eq!(0.1, miny);
        assert_eq!(0.2, minz);
        assert_eq!(1.0, maxx);
        assert_eq!(2.0, maxy);
        assert_eq!(4.0, maxz);
    }

    #[test]
    fn a_surface_file_can_be_written_and_reread() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        let dir = tempdir().unwrap();

        const EXPORT_FILE: &str = "tempfile_lhwhite.ply";
        let tfile_path = dir.path().join(EXPORT_FILE);
        let tfile_path = tfile_path.to_str().unwrap();
        write_surf(tfile_path, &surf).unwrap();

        let surf_re = read_surf(tfile_path).unwrap();

        assert_eq!(149244, surf_re.header.num_vertices);
        assert_eq!(298484, surf_re.header.num_faces);

        assert_eq!(149244, surf_re.mesh.num_vertices());
        assert_eq!(298484, surf_re.mesh.num_faces());
    }

    #[test]
    fn a_surface_file_can_be_exported_with_vertex_colors_in_ply_format() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        let colors: Vec<u8> = surf
            .colors_from_curv_file("resources/subjects_dir/subject1/surf/lh.thickness")
            .unwrap();

        let dir = tempdir().unwrap();

        // get path of current directory as &path::Path
        //let current_dir = std::env::current_dir().unwrap();
        const EXPORT_FILE: &str = "lh_mesh_thickness_viridis.ply";
        let tfile_path = dir.path().join(EXPORT_FILE);
        //let tfile_path = current_dir.join(EXPORT_FILE);

        let tfile_path = tfile_path.to_str().unwrap();

        let ply_repr = surf.mesh.to_ply(Some(&colors));
        std::fs::write(tfile_path, ply_repr).expect("Unable to write vertex-colored PLY mesh file");

        let ply_repr = std::fs::read_to_string(tfile_path).unwrap();
        assert!(ply_repr.contains("ply")); // Check the file with a mesh viewer like MeshLab. Under Ubuntu 24: ```sudo apt install meshlab```, then ```XDG_SESSION_TYPE="" meshlab temp-file.ply```
    }

    #[test]
    fn a_surface_file_can_be_exported_in_gltf_format_without_vertex_colors() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        let dir = tempdir().unwrap();
        const EXPORT_FILE: &str = "lh_mesh_white.gltf";
        // get path of current directory as &path::Path

        let tfile_path = dir.path().join(EXPORT_FILE);

        //let current_dir = std::env::current_dir().unwrap();
        //let tfile_path = current_dir.join(EXPORT_FILE);

        let tfile_path = tfile_path.to_str().unwrap();

        let gltf_repr = surf.mesh.to_gltf(None);
        std::fs::write(tfile_path, gltf_repr).expect("Unable to write glTF mesh file");

        let gltf_repr_reread = std::fs::read_to_string(tfile_path).unwrap();
        assert!(gltf_repr_reread.contains("bufferViews")); // Check the file with a mesh viewer like MeshLab. You will need at least v2023.12 for glTF support, which is not in Ubuntu 24 via apt. Get it via flatpak.
    }

    #[test]
    fn a_surface_file_can_be_exported_in_gltf_format_with_vertex_colors() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";
        let surf = read_surf(SURF_FILE).unwrap();

        let colors: Vec<u8> = surf
            .colors_from_curv_file("resources/subjects_dir/subject1/surf/lh.sulc")
            .unwrap();

        let dir = tempdir().unwrap();
        const EXPORT_FILE: &str = "lh_mesh_sulc_viridis.gltf";

        // get path of current directory as &path::Path
        //let current_dir = std::env::current_dir().unwrap();
        //let tfile_path = current_dir.join(EXPORT_FILE);

        let tfile_path = dir.path().join(EXPORT_FILE);

        let tfile_path = tfile_path.to_str().unwrap();

        let gltf_repr = surf.mesh.to_gltf(Some(&colors));
        std::fs::write(tfile_path, gltf_repr)
            .expect("Unable to write vertex-colored glTF mesh file");

        let gltf_repr_reread = std::fs::read_to_string(tfile_path).unwrap();
        assert!(gltf_repr_reread.contains("bufferViews")); // Check the file with a mesh viewer. WARNING: MeshLab 2023.12 does not support them (see issue https://github.com/cnr-isti-vclab/meshlab/issues/1464), best to use https://sandbox.babylonjs.com/ or Blender, but in Blender you need to manually activate them to be displayed.
    }
}
