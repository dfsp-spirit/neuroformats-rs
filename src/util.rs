

use std::{path::Path};
use std::io::{Read, Seek, SeekFrom};
use byteordered::byteorder::ReadBytesExt;

pub fn is_gz_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    path.as_ref()
        .file_name()
        .map(|a| a.to_string_lossy().ends_with(".gz"))
        .unwrap_or(false)
}


/// Read a variable length byte string from the input, until a \0 is hit.
pub fn read_variable_length_string<S>(input: &mut S) -> String
    where
        S: Read + Seek,
    {
        let mut cur_char = input.read_u8().unwrap() as char;
        let mut info_line = String::from(cur_char);
        while cur_char != '\0' {
            cur_char = input.read_u8().unwrap() as char;
            info_line.push(cur_char);            
        }
        input.seek(SeekFrom::Current(-1)).unwrap();
        info_line
    }

