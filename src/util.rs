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

