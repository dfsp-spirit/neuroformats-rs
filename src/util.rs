//! Utility functions used in all other neuroformats modules.

use std::{path::Path};
use std::io::{Read, Seek, SeekFrom};

use crate::error::{Result};

use byteordered::byteorder::ReadBytesExt;

/// Check whether the file extension ends with ".gz".
pub fn is_gz_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    path.as_ref()
        .file_name()
        .map(|a| a.to_string_lossy().ends_with(".gz"))
        .unwrap_or(false)
}


/// Read a variable length zero-terminated byte string from the input, until a '\0' is hit. The trailing '\0' is not consumed.
pub fn read_variable_length_string<S>(input: &mut S) -> Result<String>
    where
        S: Read + Seek,
    {
        let mut cur_char = input.read_u8()? as char;
        let mut info_line = String::from(cur_char);
        while cur_char != '\0' {
            cur_char = input.read_u8()? as char;
            info_line.push(cur_char);            
        }
        input.seek(SeekFrom::Current(-1))?;
        Ok(info_line)
    }


/// Read a fixed length zero-terminated byte string of the given length from the input. Embedded '\0' chars are allowed, but not added to the returned String.
pub fn read_fixed_length_string<S>(input: &mut S, len: usize) -> Result<String>
where
    S: Read,
{
    let mut info_line = String::with_capacity(len);
    for _  in 0..len   {
        let cur_char = input.read_u8()? as char;
        if cur_char != '\0'  {
            info_line.push(cur_char);
        }
    }
    Ok(info_line)
}


/// Determine the minimum and maximum value of an `f32` vector.
///
/// There most likely is some standard way to do this in
/// Rust which I have not yet discovered. Please file an issue
/// if you know it and read this. ;)
///
/// # Panics
///
/// If the `data` input vector is empty or contains nan values.
///
/// # Return value
///
/// A tuple of length 2, the first value is the minimum, the second the maximum.
pub fn vec32minmax(data : &Vec<f32>, remove_nan: bool) -> (f32, f32) {
    if (*data).is_empty() {
        panic!("Input data must not be empty.");
    }

    let mut curv_data_sorted : Vec<f32> = Vec::with_capacity(data.len()); // May slightly over-allocate if NaNs present.

    let mut has_nan : bool = false;
    if remove_nan {
        for v in data {
            if !v.is_nan() {
                curv_data_sorted.push(*v);
            } else {
                has_nan = true;
            }
        }
    }

    if ! remove_nan {
        if has_nan {
            panic!("NaN values not allowed in input.");
        } else {
            curv_data_sorted = data.to_vec();
        }  
    }
    
    // Sort   
    curv_data_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min: f32 = curv_data_sorted[0];
    let max: f32 = curv_data_sorted[curv_data_sorted.len() - 1];
    (min, max)
}



#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn the_min_and_max_of_an_f32_vector_without_nan_values_can_be_computed() {

        let v : Vec<f32> = vec![0.4, 0.5, 0.9, 0.01];
        let (min, max) = vec32minmax(&v, true);
        assert_abs_diff_eq!(min, 0.01, epsilon = 1e-8);
        assert_abs_diff_eq!(max, 0.9, epsilon = 1e-8);
    }

    #[test]
    fn the_min_and_max_of_an_f32_vector_with_nan_values_can_be_computed() {

        let v : Vec<f32> = vec![0.4, 0.5, 0.9, std::f32::NAN, 0.01];
        let (min, max) = vec32minmax(&v, true);
        assert_abs_diff_eq!(min, 0.01, epsilon = 1e-8);
        assert_abs_diff_eq!(max, 0.9, epsilon = 1e-8);
    }
}
