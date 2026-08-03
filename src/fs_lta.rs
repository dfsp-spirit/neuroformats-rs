//! Functions for reading FreeSurfer linear transform array (.lta) files.
//!
//! LTA files store a 4×4 affine transformation matrix along with source and
//! destination volume information. They are used for registration results
//! (e.g., Talairach transforms).

use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use std::path::Path;
use std::fmt;

use crate::error::{NeuroformatsError, Result};

/// Volume information stored in an LTA file for source or destination.
#[derive(Debug, Clone, PartialEq)]
pub struct LtaVolumeInfo {
    pub valid: i32,
    pub filename: String,
    pub volume: [usize; 3],
    pub voxelsize: [f32; 3],
    pub xras: [f32; 3],
    pub yras: [f32; 3],
    pub zras: [f32; 3],
    pub cras: [f32; 3],
}

impl Default for LtaVolumeInfo {
    fn default() -> Self {
        LtaVolumeInfo {
            valid: 0,
            filename: String::new(),
            volume: [0; 3],
            voxelsize: [0.0; 3],
            xras: [0.0; 3],
            yras: [0.0; 3],
            zras: [0.0; 3],
            cras: [0.0; 3],
        }
    }
}

/// Models a FreeSurfer linear transform array (.lta) file.
#[derive(Debug, Clone, PartialEq)]
pub struct FsLta {
    pub transform_type: i32,
    pub nxforms: i32,
    pub mean: [f32; 3],
    pub sigma: f32,
    /// The 4×4 affine transformation matrix, stored row-major.
    pub matrix: [[f32; 4]; 4],
    pub src_volume: LtaVolumeInfo,
    pub dst_volume: LtaVolumeInfo,
}

impl fmt::Display for FsLta {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LTA transform type={} from '{}' to '{}'",
            self.transform_type, self.src_volume.filename, self.dst_volume.filename
        )
    }
}

/// Skip comment lines (starting with #).
fn skip_comments(lines: &mut Lines<BufReader<File>>) -> Option<String> {
    loop {
        let line = match lines.next()? {
            Ok(l) => l,
            Err(_) => return None,
        };
        if !line.trim_start().starts_with('#') {
            return Some(line);
        }
    }
}

/// Parse a key = value line, stripping inline comments.
fn parse_key_value(line: &str, key: &str) -> Option<String> {
    let line = line.split('#').next()?.trim();
    if line.starts_with(key) {
        let value = line[key.len()..].trim();
        let value = value.strip_prefix('=').map(|s| s.trim()).unwrap_or(value);
        Some(value.to_string())
    } else {
        None
    }
}

/// Parse space-separated f32 values from a string.
fn parse_f32_triple(s: &str) -> Result<[f32; 3]> {
    let mut parts = s.split_whitespace();
    let a = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<f32>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    let b = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<f32>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    let c = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<f32>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    Ok([a, b, c])
}

/// Parse space-separated usize values from a string.
fn parse_usize_triple(s: &str) -> Result<[usize; 3]> {
    let mut parts = s.split_whitespace();
    let a = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<usize>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    let b = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<usize>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    let c = parts
        .next()
        .ok_or_else(|| NeuroformatsError::InvalidFsMghFormat)?
        .parse::<usize>()
        .map_err(|_| NeuroformatsError::InvalidFsMghFormat)?;
    Ok([a, b, c])
}

/// Parse a single volume info section, stopping at the next "volume info" header or EOF.
/// Returns the parsed info and the next section header line if one was encountered.
fn parse_volume_info_section(
    lines: &mut Lines<BufReader<File>>,
) -> Result<(LtaVolumeInfo, Option<String>)> {
    let mut info = LtaVolumeInfo::default();

    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => return Err(NeuroformatsError::Io(e)),
            None => return Ok((info, None)), // EOF
        };
        let line_trimmed = line.trim();

        if line_trimmed.is_empty() {
            continue;
        }

        // If we hit the next volume info section, return this line to caller.
        if line_trimmed.contains("volume info") && !line_trimmed.starts_with("valid") {
            return Ok((info, Some(line)));
        }

        if let Some(val) = parse_key_value(line_trimmed, "valid") {
            info.valid = val.parse::<i32>().unwrap_or(0);
        } else if let Some(val) = parse_key_value(line_trimmed, "filename") {
            info.filename = val;
        } else if let Some(val) = parse_key_value(line_trimmed, "volume") {
            info.volume = parse_usize_triple(&val)?;
        } else if let Some(val) = parse_key_value(line_trimmed, "voxelsize") {
            info.voxelsize = parse_f32_triple(&val)?;
        } else if let Some(val) = parse_key_value(line_trimmed, "xras") {
            info.xras = parse_f32_triple(&val)?;
        } else if let Some(val) = parse_key_value(line_trimmed, "yras") {
            info.yras = parse_f32_triple(&val)?;
        } else if let Some(val) = parse_key_value(line_trimmed, "zras") {
            info.zras = parse_f32_triple(&val)?;
        } else if let Some(val) = parse_key_value(line_trimmed, "cras") {
            info.cras = parse_f32_triple(&val)?;
        }
    }
}

impl FsLta {
    /// Read an LTA file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsLta> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut lta = FsLta {
            transform_type: 0,
            nxforms: 1,
            mean: [0.0; 3],
            sigma: 0.0,
            matrix: [[0.0; 4]; 4],
            src_volume: LtaVolumeInfo::default(),
            dst_volume: LtaVolumeInfo::default(),
        };

        // Parse header key-value pairs.
        while let Some(line) = skip_comments(&mut lines) {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            if let Some(val) = parse_key_value(&line, "type") {
                lta.transform_type = val.parse::<i32>().unwrap_or(0);
            } else if let Some(val) = parse_key_value(&line, "nxforms") {
                lta.nxforms = val.parse::<i32>().unwrap_or(1);
            } else if let Some(val) = parse_key_value(&line, "mean") {
                lta.mean = parse_f32_triple(&val)?;
            } else if let Some(val) = parse_key_value(&line, "sigma") {
                lta.sigma = val.parse::<f32>().unwrap_or(0.0);
            } else {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[0].parse::<i32>().is_ok() {
                    break;
                }
            }
        }

        // Read the 4x4 matrix: 4 lines of 4 floats.
        for row in 0..4 {
            let matrix_line = match lines.next() {
                Some(Ok(line)) => line,
                _ => return Err(NeuroformatsError::InvalidFsMghFormat),
            };
            let values: Vec<f32> = matrix_line
                .split_whitespace()
                .filter_map(|s| s.parse::<f32>().ok())
                .collect();
            if values.len() < 4 {
                return Err(NeuroformatsError::InvalidFsMghFormat);
            }
            for col in 0..4 {
                lta.matrix[row][col] = values[col];
            }
        }

        // Parse volume info sections. parse_volume_info_section returns
        // the next section header so we can handle it in this loop.
        let mut next_section: Option<String> = None;

        // Skip blank lines and find "src volume info".
        loop {
            let line = match next_section.take() {
                Some(l) => l,
                None => match lines.next() {
                    Some(Ok(l)) => l,
                    Some(Err(e)) => return Err(NeuroformatsError::Io(e)),
                    None => return Ok(lta),
                },
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("src volume info") {
                let (info, next) = parse_volume_info_section(&mut lines)?;
                lta.src_volume = info;
                next_section = next;
                break;
            }
        }

        // Process next section header if we got one.
        loop {
            let line = match next_section.take() {
                Some(l) => l,
                None => match lines.next() {
                    Some(Ok(l)) => l,
                    Some(Err(e)) => return Err(NeuroformatsError::Io(e)),
                    None => return Ok(lta),
                },
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("dst volume info") {
                let (info, _next) = parse_volume_info_section(&mut lines)?;
                lta.dst_volume = info;
                break;
            }
        }

        Ok(lta)
    }
}

/// Read a FreeSurfer linear transform array (.lta) file.
///
/// # Examples
///
/// ```no_run
/// let lta = neuroformats::read_lta("/path/to/talairach.lta").unwrap();
/// println!("{}", lta);
/// ```
pub fn read_lta<P: AsRef<Path>>(path: P) -> Result<FsLta> {
    FsLta::from_file(path)
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;
    use std::io::Write;
    use tempfile::tempdir;

    /// Create a temp LTA file from a text string, read it, return the parsed FsLta.
    fn write_and_read_lta(content: &str) -> FsLta {
        let dir = tempdir().unwrap();
        let tfile_path = dir.path().join("test.lta");
        let tfile_path = tfile_path.to_str().unwrap();
        let mut f = File::create(tfile_path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        drop(f);
        read_lta(tfile_path).unwrap()
    }

    #[test]
    fn lta_file_can_be_read_with_known_data() {
        let lta_text = r#"# test transform
type      = 0 # LINEAR_VOX_TO_VOX
nxforms   = 1
mean      = 10.0 20.0 30.0
sigma     = 5000.0
1 4 4
1.0 0.0 0.0 5.0
0.0 1.0 0.0 10.0
0.0 0.0 1.0 15.0
0.0 0.0 0.0 1.0
src volume info
valid = 1  # volume info valid
filename = source.mgz
volume = 128 128 128
voxelsize = 2.0 2.0 2.0
xras   = -1 0 0
yras   = 0 0 -1
zras   = 0 1 0
cras   = 0 0 0
dst volume info
valid = 1  # volume info valid
filename = target.mgz
volume = 256 256 256
voxelsize = 1.0 1.0 1.0
xras   = -1 0 0
yras   = 0 0 -1
zras   = 0 1 0
cras   = 5.0 10.0 0.0
"#;

        let lta = write_and_read_lta(lta_text);

        assert_eq!(lta.transform_type, 0);
        assert_eq!(lta.nxforms, 1);
        assert_abs_diff_eq!(lta.mean[0], 10.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.mean[1], 20.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.mean[2], 30.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.sigma, 5000.0, epsilon = 1e-6);

        // Check matrix.
        assert_abs_diff_eq!(lta.matrix[0][0], 1.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.matrix[0][3], 5.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.matrix[1][3], 10.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.matrix[2][3], 15.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.matrix[3][0], 0.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.matrix[3][3], 1.0, epsilon = 1e-6);

        // Check src volume info.
        assert_eq!(lta.src_volume.valid, 1);
        assert_eq!(lta.src_volume.filename, "source.mgz");
        assert_eq!(lta.src_volume.volume, [128, 128, 128]);
        assert_abs_diff_eq!(lta.src_volume.voxelsize[0], 2.0, epsilon = 1e-6);

        // Check dst volume info.
        assert_eq!(lta.dst_volume.valid, 1);
        assert_eq!(lta.dst_volume.filename, "target.mgz");
        assert_eq!(lta.dst_volume.volume, [256, 256, 256]);
        assert_abs_diff_eq!(lta.dst_volume.cras[0], 5.0, epsilon = 1e-6);
        assert_abs_diff_eq!(lta.dst_volume.cras[1], 10.0, epsilon = 1e-6);
    }
}
