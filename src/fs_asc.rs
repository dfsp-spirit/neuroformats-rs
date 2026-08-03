//! Functions for managing FreeSurfer brain surface meshes in ASCII (.asc) format.
//!
//! ASC files are a text-based variant of the FreeSurfer binary surf format.
//! They store the same triangular mesh data (vertices + faces) but in a
//! human-readable ASCII representation.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::fmt;

use crate::config;
use crate::error::{NeuroformatsError, Result};
use crate::fs_surface::BrainMesh;
use crate::util::validate_finite_vertex_values;

/// Models a FreeSurfer brain surface mesh in ASCII (.asc) format.
///
/// This is simply a [`BrainMesh`] with a comment line.
#[derive(Debug, PartialEq, Clone)]
pub struct FsAsc {
    pub comment: String,
    pub mesh: BrainMesh,
}

impl fmt::Display for FsAsc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FreeSurfer ASCII brain mesh '{}' with {} vertices and {} faces.",
            self.comment.trim(),
            self.mesh.num_vertices(),
            self.mesh.num_faces()
        )
    }
}

/// Read a FreeSurfer brain mesh from an ASCII (.asc) file.
///
/// # Examples
///
/// ```no_run
/// let asc = neuroformats::read_asc("/path/to/surface.asc").unwrap();
/// println!("{}", asc);
/// ```
pub fn read_asc<P: AsRef<Path>>(path: P) -> Result<FsAsc> {
    let reader = BufReader::new(File::open(path)?);
    let mut lines = reader.lines();

    // First line is a comment.
    let comment = lines
        .next()
        .transpose()?
        .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?;

    // Second line: num_vertices num_faces.
    let header_line = lines
        .next()
        .transpose()?
        .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?;
    let mut header_parts = header_line.split_whitespace();
    let num_vertices: usize = header_parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?
        .parse()
        .map_err(|_| NeuroformatsError::InvalidFsSurfaceFormat)?;
    let num_faces: usize = header_parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?
        .parse()
        .map_err(|_| NeuroformatsError::InvalidFsSurfaceFormat)?;

    // Validate against limits.
    if num_vertices > config::max_vertices() {
        return Err(NeuroformatsError::AllocationTooLarge);
    }

    // Read vertex data.
    let mut vertices: Vec<f32> = Vec::with_capacity(num_vertices * 3);
    for _ in 0..num_vertices {
        let vline = lines
            .next()
            .transpose()?
            .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?;
        let mut parts = vline.split_whitespace();
        for _ in 0..3 {
            let val: f32 = parts
                .next()
                .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?
                .parse()
                .map_err(|_| NeuroformatsError::InvalidFsSurfaceFormat)?;
            vertices.push(val);
        }
    }

    validate_finite_vertex_values(&vertices, "ASC vertex")?;

    // Read face data.
    let mut faces: Vec<i32> = Vec::with_capacity(num_faces * 3);
    for _ in 0..num_faces {
        let fline = lines
            .next()
            .transpose()?
            .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?;
        let mut parts = fline.split_whitespace();
        for _ in 0..3 {
            let val: i32 = parts
                .next()
                .ok_or_else(|| NeuroformatsError::InvalidFsSurfaceFormat)?
                .parse()
                .map_err(|_| NeuroformatsError::InvalidFsSurfaceFormat)?;
            faces.push(val);
        }
    }

    Ok(FsAsc {
        comment,
        mesh: BrainMesh { vertices, faces },
    })
}

/// Write a brain mesh to a FreeSurfer ASCII (.asc) surface file.
///
/// # Examples
///
/// ```no_run
/// let surf = neuroformats::read_surf("/path/to/lh.white").unwrap();
/// let asc = neuroformats::FsAsc { comment: "#!ascii version of lh.white".to_string(), mesh: surf.mesh };
/// neuroformats::write_asc("/tmp/test.asc", &asc).unwrap();
/// ```
pub fn write_asc<P: AsRef<Path> + Copy>(path: P, asc: &FsAsc) -> std::io::Result<()> {
    let f = File::create(path)?;
    let writer = BufWriter::new(f);
    let mut writer = writer;

    writeln!(writer, "{}", asc.comment)?;
    writeln!(
        writer,
        "{} {}",
        asc.mesh.num_vertices(),
        asc.mesh.num_faces()
    )?;

    // Write vertices.
    for i in 0..asc.mesh.num_vertices() {
        writeln!(
            writer,
            "{} {} {} 0",
            asc.mesh.vertices[i * 3],
            asc.mesh.vertices[i * 3 + 1],
            asc.mesh.vertices[i * 3 + 2]
        )?;
    }

    // Write faces.
    for i in 0..asc.mesh.num_faces() {
        writeln!(
            writer,
            "{} {} {} 0",
            asc.mesh.faces[i * 3],
            asc.mesh.faces[i * 3 + 1],
            asc.mesh.faces[i * 3 + 2]
        )?;
    }

    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;
    use tempfile::tempdir;

    /// Create a test ASC file by converting from a surf file.
    fn create_test_asc_from_surf(surf_path: &str, asc_path: &str) {
        let surf = crate::read_surf(surf_path).unwrap();
        let asc = FsAsc {
            comment: format!("#!ascii version of {}", surf_path),
            mesh: surf.mesh,
        };
        write_asc(asc_path, &asc).unwrap();
    }

    #[test]
    fn an_asc_file_can_be_written_and_reread() {
        const SURF_FILE: &str = "resources/subjects_dir/subject1/surf/lh.white";

        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("temp-file.asc");
        let tfile_path = tfile_path.to_str().unwrap();

        create_test_asc_from_surf(SURF_FILE, tfile_path);

        let asc = read_asc(tfile_path).unwrap();
        assert_eq!(149244, asc.mesh.num_vertices());
        assert_eq!(298484, asc.mesh.num_faces());

        // Spot-check a few vertices.
        let surf = crate::read_surf(SURF_FILE).unwrap();
        assert_eq!(surf.mesh.num_vertices(), asc.mesh.num_vertices());
        assert_eq!(surf.mesh.num_faces(), asc.mesh.num_faces());

        // Check all vertex data matches.
        for i in 0..surf.mesh.vertices.len() {
            assert_abs_diff_eq!(surf.mesh.vertices[i], asc.mesh.vertices[i], epsilon = 1e-6);
        }
        // Check all face data matches.
        for i in 0..surf.mesh.faces.len() {
            assert_eq!(surf.mesh.faces[i], asc.mesh.faces[i]);
        }
    }

    #[test]
    fn asc_rejects_nan_vertex() {
        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("bad.asc");
        let tfile_path = tfile_path.to_str().unwrap();

        use std::io::Write;
        let mut f = File::create(tfile_path).unwrap();
        writeln!(f, "#!ascii version of test").unwrap();
        writeln!(f, "1 0").unwrap();
        writeln!(f, "NaN 0.0 0.0 0").unwrap();

        let result = read_asc(tfile_path);
        assert!(result.is_err());
    }
}
