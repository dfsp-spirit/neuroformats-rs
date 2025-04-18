//! Utility functions used in all other neuroformats modules.

use std::io::BufRead;
use std::path::Path;

use crate::error::Result;

use byteordered::byteorder::ReadBytesExt;

use colorgrad::Gradient;

/// Convert a slice of f32 values to a vector of RGB colors using the Viridis colormap.
///
/// This function takes a slice of f32 values and maps them to RGB colors using the Viridis colormap.
/// The values are normalized to the range [0, 1] based on the provided minimum and maximum values.
/// The resulting colors are returned as a vector of u8 values, where each color is represented by three consecutive u8 values (R, G, B).
/// # Arguments
/// * `values` - A slice of f32 values to be converted to colors.
/// * `min_val` - The minimum value for normalization. If the values argument contains values less than this, they will be clamped to this value.
/// * `max_val` - The maximum value for normalization. If the values argument contains values greater than this, they will be clamped to this value.
/// # Returns
/// * A vector of u8 values representing the RGB colors.
/// # Example
/// ```
/// use neuroformats::util::values_to_colors;
/// let values = vec![0.0, 0.5, 1.1];
/// let min_val = 0.0;
/// let max_val = 1.0;
/// let colors = values_to_colors(&values, min_val, max_val);
/// assert_eq!(colors, vec![68, 1, 84, 38, 130, 142, 254, 232, 37]);
/// ```
/// # Note
/// The input values should be in the range [min_val, max_val]. Values outside this range will be clamped.
/// The resulting colors are in the RGB format, where each color is represented by three consecutive u8 values (R, G, B).
/// The colors are generated using the Viridis colormap, which is perceptually uniform and colorblind-friendly.
pub fn values_to_colors(values: &[f32], min_val: f32, max_val: f32) -> Vec<u8> {
    // Create Viridis colormap
    let grad = colorgrad::preset::viridis();

    // Normalize values to [0, 1] range and map to colors
    let mut colors = Vec::with_capacity(values.len() * 3);

    for &value in values {
        // Normalize to [0, 1] range
        let t = (value - min_val) / (max_val - min_val);
        let t = t.clamp(0.0, 1.0); // Ensure within bounds

        // Get color from gradient
        let color = grad.at(t as f32);

        // Convert to RGB u8 and add to output
        colors.push((color.r * 255.0) as u8);
        colors.push((color.g * 255.0) as u8);
        colors.push((color.b * 255.0) as u8);
    }

    colors
}

/// Check whether the file extension ends with ".gz".
/// This is a simple check and does not guarantee that the file is actually gzipped.
/// # Example
/// ```
/// use std::path::Path;
/// use neuroformats::util::is_gz_file;
/// assert_eq!(is_gz_file("example.gz"), true);
/// assert_eq!(is_gz_file("example.txt"), false);
/// ```
/// # Arguments
/// * `path` - A path to the file to check.
/// # Returns
/// * `true` if the file name ends with ".gz", `false` otherwise.
/// # Note
/// This function does not check the actual content of the file.
pub fn is_gz_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    path.as_ref()
        .file_name()
        .map(|a| a.to_string_lossy().ends_with(".gz"))
        .unwrap_or(false)
}

/// Read a variable length Freesurfer-style byte string from the input.
///
/// A FreeSurfer-style variable length string is a string terminated by two `\x0A`, or 'Unix line feed' ASCII characters.
///
/// # Warnings
///
/// * Terrible things will happen if the input does not contain a sequence of two consecutive `\x0A` chars.
pub fn read_fs_variable_length_string<S>(input: &mut S) -> Result<String>
where
    S: BufRead,
{
    let mut last_char;
    let mut cur_char: char = '0';
    let mut info_line = String::new();
    loop {
        last_char = cur_char;
        cur_char = input.read_u8()? as char;
        info_line.push(cur_char);
        if last_char == '\x0A' && cur_char == '\x0A' {
            break;
        }
    }
    Ok(info_line)
}

/// Read a fixed length NUL-terminated string.
///
/// Read a fixed length zero-terminated byte string of the given length from the input. The `len` value must include the trailing NUL byte position, if any. Embedded '\0' chars are allowed, and the trailing one (if any) is read but not added to the returned String (all others are).
pub fn read_fixed_length_string<S>(input: &mut S, len: usize) -> Result<String>
where
    S: BufRead,
{
    let mut info_line = String::with_capacity(len);
    for char_idx in 0..len {
        let cur_char = input.read_u8()? as char;
        if char_idx == (len - 1) {
            if cur_char != '\0' {
                info_line.push(cur_char);
            }
        } else {
            info_line.push(cur_char);
        }
    }
    Ok(info_line)
}

/// Determine the minimum and maximum value of an `f32` sequence.
///
/// # Panics
///
/// If the `data` input vector is empty or contains nan values.
///
/// # Return value
///
/// A tuple of length 2, the first value is the minimum, the second the maximum.
///
/// Example:
/// ```
/// use neuroformats::util::vec32minmax;
/// let v: Vec<f32> = vec![0.4, 0.5, 0.9, 0.01];
/// let (min, max) = vec32minmax(v.into_iter(), true);
/// assert_eq!(min, 0.01);
/// assert_eq!(max, 0.9);
/// ```
/// # Arguments
/// * `data` - An iterator over `f32` values.
/// * `remove_nan` - If set to true, NaN values will be filtered out. If set to false, the function will panic if NaN values are found.
/// # Note
/// The function will panic if the input iterator is empty or contains NaN values and `remove_nan` is set to false.
/// The function will also panic if the input iterator is empty.
/// The function will filter out NaN values if `remove_nan` is set to true.
/// The function will return a tuple containing the minimum and maximum values found in the input iterator.
pub fn vec32minmax<I>(data: I, remove_nan: bool) -> (f32, f32)
where
    I: Iterator<Item = f32>,
{
    // NOTE: the data variable is a iterator, it will be consumed by the for loop bellow
    let mut data = data.filter(|v| match (remove_nan, v.is_nan()) {
        // if is just a regular f32, just let is pass
        (_, false) => true,
        // remove_nan is set, if is a NaN, filter it out
        (true, true) => false,
        // remove_nan is not set, panic if is NaN
        (false, true) => panic!("NaN values not allowed in input."),
    });

    let first = data.next().expect("Input data must not be empty.");
    let mut min = first;
    let mut max = first;
    for value in data {
        if value < min {
            min = value;
        } else if value > max {
            max = value;
        }
    }
    (min, max)
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn the_min_and_max_of_an_f32_vector_without_nan_values_can_be_computed() {
        let v: Vec<f32> = vec![0.4, 0.5, 0.9, 0.01];
        let (min, max) = vec32minmax(v.into_iter(), true);
        assert_abs_diff_eq!(min, 0.01, epsilon = 1e-8);
        assert_abs_diff_eq!(max, 0.9, epsilon = 1e-8);
    }

    #[test]
    fn the_min_and_max_of_an_f32_vector_with_nan_values_can_be_computed() {
        let v: Vec<f32> = vec![0.4, 0.5, 0.9, std::f32::NAN, 0.01];
        let (min, max) = vec32minmax(v.into_iter(), true);
        assert_abs_diff_eq!(min, 0.01, epsilon = 1e-8);
        assert_abs_diff_eq!(max, 0.9, epsilon = 1e-8);
    }

    #[test]
    fn a_variable_length_fs_string_can_be_read() {
        use std::io::{Cursor, Read, Seek, SeekFrom, Write};

        // Create our "file".
        let mut c = Cursor::new(Vec::<u8>::new());
        c.write(b"test\x0A\x0A").unwrap();
        c.write(&[166 as u8]).unwrap();

        // Seek to start
        c.seek(SeekFrom::Start(0)).unwrap();

        // Re-read the data.
        let s = read_fs_variable_length_string(&mut c).unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();

        assert_eq!(s, "test\n\n");
        assert_eq!(out, &[166]);
        assert_eq!(7, c.position());
    }

    #[test]
    fn a_fixed_length_nul_terminated_string_can_be_read() {
        use std::io::{Cursor, Read, Seek, SeekFrom, Write};

        // Create our "file".
        let mut c = Cursor::new(Vec::<u8>::new());
        c.write(b"test\x0A\x0Atest\x00").unwrap();

        // Seek to start
        c.seek(SeekFrom::Start(0)).unwrap();

        // Re-read the data.
        let s = read_fixed_length_string(&mut c, 11 as usize).unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();

        assert_eq!(s, "test\n\ntest");
        assert_eq!(out, &[]);
        assert_eq!(11, c.position());
    }

    #[test]
    fn a_fixed_length_without_termination_char_can_be_read() {
        use std::io::{Cursor, Read, Seek, SeekFrom, Write};

        // Create our "file".
        let mut c = Cursor::new(Vec::<u8>::new());
        c.write(b"test\x0A\x0Atestdonotreadthis").unwrap();

        // Seek to start
        c.seek(SeekFrom::Start(0)).unwrap();

        // Re-read the data.
        let s = read_fixed_length_string(&mut c, 10 as usize).unwrap();

        assert_eq!(s, "test\n\ntest");
        assert_eq!(10, c.position());

        let mut out: Vec<u8> = Vec::new();
        c.read_to_end(&mut out).unwrap();
        assert_eq!(23, c.position());
    }

    #[test]
    fn float_per_vertex_data_can_be_converted_to_rgb_uint8_colors() {
        let values: Vec<f32> = vec![0.0, 0.5, 1.0];
        let min_val: f32 = 0.0;
        let max_val: f32 = 1.0;
        let colors: Vec<u8> = values_to_colors(&values, min_val, max_val);
        assert_eq!(colors, vec![68, 1, 84, 38, 130, 142, 254, 232, 37]);
    }
}
