//! Functions for managing FreeSurfer per-vertex data in weight (.w) files.
//!
//! Weight files are a simple legacy format that stores one scalar value per vertex.
//! They are simpler than curv files (no magic bytes, no face count, no values-per-vertex field).
//! The format is: `num_vertices` (i32, big-endian), followed by that many `f32` values.

use byteordered::{ByteOrdered, Endianness};

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::fmt;

use crate::config;
use crate::error::{NeuroformatsError, Result};
use crate::util::{validate_finite_vertex_values, vec32minmax};

/// Models a FreeSurfer per-vertex data file in weight (.w) format.
#[derive(Debug, PartialEq, Clone)]
pub struct FsWeight {
    pub data: Vec<f32>,
}

impl fmt::Display for FsWeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (min, max) = vec32minmax(self.data.iter().copied(), false);
        write!(
            f,
            "Per-vertex weight data for {} vertices, with values in range {} to {}.",
            self.data.len(),
            min,
            max
        )
    }
}

impl FsWeight {
    /// Read a weight file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsWeight> {
        let file = BufReader::new(File::open(path)?);
        FsWeight::from_reader(file)
    }

    /// Read weight data from a reader.
    pub fn from_reader<S>(input: S) -> Result<FsWeight>
    where
        S: BufRead,
    {
        let mut input = ByteOrdered::be(input);

        let num_vertices: i32 = input.read_i32()?;
        if num_vertices < 0 {
            return Err(NeuroformatsError::InvalidHeaderValue(format!(
                "Negative vertex count in weight file: {}",
                num_vertices
            )));
        }
        let num_vertices = num_vertices as usize;
        if num_vertices > config::max_vertices() {
            return Err(NeuroformatsError::AllocationTooLarge);
        }
        let data_bytes = num_vertices
            .checked_mul(4)
            .ok_or(NeuroformatsError::IntegerOverflow)?;
        if data_bytes > config::max_bytes_per_file() {
            return Err(NeuroformatsError::AllocationTooLarge);
        }

        let mut data: Vec<f32> = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            data.push(input.read_f32()?);
        }

        validate_finite_vertex_values(&data, "weight value")?;

        Ok(FsWeight { data })
    }
}

/// Read per-vertex data from a FreeSurfer weight (.w) file.
///
/// Weight files store a single scalar value for each vertex of a brain mesh,
/// similar to curv files but in a simpler legacy format.
///
/// # Examples
///
/// ```no_run
/// let wfile = neuroformats::read_weight("/path/to/rh.ppa.invivo.w").unwrap();
/// let value_at_vertex_0: f32 = wfile.data[0];
/// ```
pub fn read_weight<P: AsRef<Path> + Copy>(path: P) -> Result<FsWeight> {
    FsWeight::from_file(path)
}

/// Write an FsWeight struct to a file in FreeSurfer weight (.w) format.
///
/// # Examples
///
/// ```no_run
/// let wfile = neuroformats::read_weight("/path/to/rh.ppa.invivo.w").unwrap();
/// neuroformats::write_weight("/tmp/test.w", &wfile).unwrap();
/// ```
pub fn write_weight<P: AsRef<Path> + Copy>(path: P, weight: &FsWeight) -> std::io::Result<()> {
    let f = File::create(path)?;
    let writer = BufWriter::new(f);
    let mut writer = ByteOrdered::runtime(writer, Endianness::Big);

    writer.write_i32(weight.data.len() as i32)?;
    for v in &weight.data {
        writer.write_f32(*v)?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;
    use tempfile::tempdir;

    /// Helper: write known weight data to a temp file, read it back, verify.
    fn write_and_read_weight(data: &[f32]) -> FsWeight {
        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("test.w");
        let tfile_path = tfile_path.to_str().unwrap();

        let wfile = FsWeight {
            data: data.to_vec(),
        };
        write_weight(tfile_path, &wfile).unwrap();
        read_weight(tfile_path).unwrap()
    }

    #[test]
    fn weight_file_can_be_read_with_known_data() {
        let known: Vec<f32> = vec![0.0, 1.5, -2.3, 3.14, 42.0];
        let wfile = write_and_read_weight(&known);

        assert_eq!(known.len(), wfile.data.len());
        for i in 0..known.len() {
            assert_abs_diff_eq!(known[i], wfile.data[i], epsilon = 1e-6);
        }
    }

    #[test]
    fn weight_file_write_reread_preserves_data() {
        let data: Vec<f32> = (0..100).map(|i| (i as f32) * 0.1).collect();
        let wfile = write_and_read_weight(&data);
        assert_eq!(data.len(), wfile.data.len());
        for i in 0..data.len() {
            assert_abs_diff_eq!(data[i], wfile.data[i], epsilon = 1e-6);
        }
    }

    #[test]
    fn weight_file_handles_empty_data() {
        let wfile = write_and_read_weight(&[]);
        assert_eq!(0, wfile.data.len());
    }

    #[test]
    fn weight_file_rejects_negative_vertex_count() {
        use std::io::Cursor;
        use byteordered::byteorder::WriteBytesExt;

        let mut buf = Cursor::new(Vec::new());
        buf.write_i32::<byteordered::byteorder::BigEndian>(-1).unwrap();
        buf.set_position(0);

        let result = FsWeight::from_reader(buf);
        assert!(result.is_err());
    }

    #[test]
    fn weight_file_rejects_nan_values() {
        use std::io::Cursor;
        use byteordered::byteorder::WriteBytesExt;

        let mut buf = Cursor::new(Vec::new());
        buf.write_i32::<byteordered::byteorder::BigEndian>(1).unwrap();
        buf.write_f32::<byteordered::byteorder::BigEndian>(f32::NAN).unwrap();
        buf.set_position(0);

        let result = FsWeight::from_reader(buf);
        assert!(result.is_err());
    }
}
