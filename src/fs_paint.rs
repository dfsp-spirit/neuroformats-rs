//! Functions for reading FreeSurfer paint (.paint) files.
//!
//! Paint files assign each vertex of a brain surface mesh to a region using integer
//! labels. They are similar to annot files but without a colortable — the mapping
//! from paint values to region names must be obtained separately.

use byteordered::{ByteOrdered, Endianness};

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::fmt;

use crate::config;
use crate::error::{NeuroformatsError, Result};

/// Models a FreeSurfer paint file, assigning an integer label to each vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct FsPaint {
    pub vertex_indices: Vec<i32>,
    pub vertex_labels: Vec<i32>,
}

impl fmt::Display for FsPaint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Paint data for {} vertices.",
            self.vertex_indices.len()
        )
    }
}

impl FsPaint {
    /// Read a paint file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsPaint> {
        let file = BufReader::new(File::open(path)?);
        let mut file = ByteOrdered::be(file);

        let num_vertices: i32 = file.read_i32()?;

        if num_vertices < 0 {
            return Err(NeuroformatsError::InvalidHeaderValue(format!(
                "Negative vertex count in paint file: {}",
                num_vertices
            )));
        }
        let num_vertices_usize = num_vertices as usize;
        if num_vertices_usize > config::max_vertices() {
            return Err(NeuroformatsError::AllocationTooLarge);
        }

        let mut vertex_indices: Vec<i32> = Vec::with_capacity(num_vertices_usize);
        let mut vertex_labels: Vec<i32> = Vec::with_capacity(num_vertices_usize);
        for _ in 0..num_vertices_usize {
            vertex_indices.push(file.read_i32()?);
            vertex_labels.push(file.read_i32()?);
        }

        Ok(FsPaint {
            vertex_indices,
            vertex_labels,
        })
    }
}

/// Read a FreeSurfer paint (.paint) file.
///
/// Paint files assign each vertex of a brain surface to an integer region label.
/// Unlike annot files, they do not contain a colortable.
///
/// # Examples
///
/// ```no_run
/// let paint = neuroformats::read_paint("/path/to/lh.cortex.paint").unwrap();
/// println!("{}", paint);
/// ```
pub fn read_paint<P: AsRef<Path> + Copy>(path: P) -> Result<FsPaint> {
    FsPaint::from_file(path)
}

/// Write an FsPaint struct to a FreeSurfer paint (.paint) file.
pub fn write_paint<P: AsRef<Path> + Copy>(path: P, paint: &FsPaint) -> std::io::Result<()> {
    let f = File::create(path)?;
    let writer = BufWriter::new(f);
    let mut writer = ByteOrdered::runtime(writer, Endianness::Big);

    writer.write_i32(paint.vertex_indices.len() as i32)?;
    for i in 0..paint.vertex_indices.len() {
        writer.write_i32(paint.vertex_indices[i])?;
        writer.write_i32(paint.vertex_labels[i])?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn a_paint_file_can_be_written_and_reread() {
        const ANNOT_FILE: &str = "resources/subjects_dir/subject1/label/lh.aparc.annot";

        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("temp-file.paint");
        let tfile_path = tfile_path.to_str().unwrap();

        // Create paint from annot.
        let annot = crate::read_annot(ANNOT_FILE).unwrap();
        let paint = FsPaint {
            vertex_indices: annot.vertex_indices.clone(),
            vertex_labels: annot.vertex_labels.clone(),
        };
        write_paint(tfile_path, &paint).unwrap();

        let paint_re = read_paint(tfile_path).unwrap();
        assert_eq!(annot.vertex_indices.len(), paint_re.vertex_indices.len());
        assert_eq!(annot.vertex_labels.len(), paint_re.vertex_labels.len());

        for i in 0..annot.vertex_indices.len() {
            assert_eq!(annot.vertex_indices[i], paint_re.vertex_indices[i]);
            assert_eq!(annot.vertex_labels[i], paint_re.vertex_labels[i]);
        }
    }

    #[test]
    fn paint_rejects_negative_vertex_count() {
        // Create a temp file with a negative vertex count header.
        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("bad.paint");
        let tfile_path = tfile_path.to_str().unwrap();

        let f = File::create(tfile_path).unwrap();
        let writer = BufWriter::new(f);
        let mut writer = ByteOrdered::runtime(writer, Endianness::Big);
        writer.write_i32(-1).unwrap();
        writer.flush().unwrap();
        drop(writer);

        let result = read_paint(tfile_path);
        assert!(result.is_err());
    }
}
