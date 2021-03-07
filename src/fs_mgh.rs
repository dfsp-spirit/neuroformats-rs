//! Functions for managing FreeSurfer brain volumes in binary 'MGH' files.


use byteordered::{ByteOrdered};
use ndarray::{Array, Array4};


use std::{fs::File};
use std::io::{BufReader, Read};
use std::path::{Path};

use crate::error::{NeuroformatsError, Result};

pub const MGH_VERSION: i32 = 1;

pub const MGH_DATATYPE_NAMES : [&str; 4] = ["MRI_UCHAR", "MRI_INT", "MRI_FLOAT", "MRI_SHORT"];
pub const MGH_DATATYPE_CODES : [i32; 4] = [0, 1, 3, 4];
pub const MGH_DATA_START : i32 = 284; // The index in bytes where the data part starts in an MGH file.

/// Models the header of a FreeSurfer MGH file containing a brain volume.
#[derive(Debug, Clone, PartialEq)]
pub struct FsMghHeader {
    pub mgh_format_version: i32,
    pub dim1len: i32,
    pub dim2len: i32,
    pub dim3len: i32,
    pub dim4len: i32,  // aka "num_frames"
    pub dtype: i32,
    pub dof: i32,
    pub is_ras_good: i16,
    pub delta: [f32; 3],
    pub mdc_raw: [f32; 9],
    pub p_xyz_c: [f32; 3],
}


/// Models a FreeSurfer MGH file.
#[derive(Debug, Clone, PartialEq)]
pub struct FsMgh {
    pub header: FsMghHeader,
    pub data_mri_uchar: Option<Array4<u8>>,
    pub data_mri_float: Option<Array4<f32>>,
    pub data_mri_int: Option<Array4<i32>>,
    pub data_mri_short: Option<Array4<i16>>,
}


impl Default for FsMghHeader {
    fn default() -> FsMghHeader {
        FsMghHeader {
            mgh_format_version: 1 as i32,
            dim1len: 0 as i32,
            dim2len: 0 as i32,
            dim3len: 0 as i32,
            dim4len: 0 as i32,
            dtype: 1 as i32,
            dof: 0 as i32,
            is_ras_good: 0 as i16,
            delta: [0.; 3],
            mdc_raw: [0.; 9],
            p_xyz_c: [0.; 3],
        }
    }
}

/// The header of an MGH/MGZ file.
impl FsMghHeader {
    
    /// Read an MGH header from a file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<FsMghHeader> {
        let mut file = BufReader::new(File::open(path)?);
        FsMghHeader::from_reader(&mut file)
    }


    /// Read an MGH header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// header.
    pub fn from_reader<S>(input: &mut S) -> Result<FsMghHeader>
    where
        S: Read,
    {
        let mut hdr = FsMghHeader::default();
    
        let mut input = ByteOrdered::be(input);

        hdr.mgh_format_version = input.read_i32()?;

        if hdr.mgh_format_version != MGH_VERSION {
            return Err(NeuroformatsError::InvalidFsMghFormat);
        }

        hdr.dim1len = input.read_i32()?;
        hdr.dim2len = input.read_i32()?;
        hdr.dim3len = input.read_i32()?;
        hdr.dim4len = input.read_i32()?;

        hdr.dtype = input.read_i32()?;
        hdr.dof = input.read_i32()?;

        hdr.is_ras_good = input.read_i16()?;

        hdr.delta = [0.; 3];
        hdr.mdc_raw = [0.; 9];
        hdr.p_xyz_c = [0.; 3];

        if hdr.is_ras_good == 1 as i16 {            
            for idx in 0..2 { hdr.delta[idx] = input.read_f32()?; }
            for idx in 0..8 { hdr.mdc_raw[idx] = input.read_f32()?; }
            for idx in 0..2 { hdr.p_xyz_c[idx] = input.read_f32()?; }
        }        
        Ok(hdr)
    }
}


impl FsMgh {

    /// Read an MGH or MGZ file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P, hdr : &FsMghHeader) -> Result<FsMgh> {

        let file = BufReader::new(File::open(path)?);
        let mut file = ByteOrdered::be(file);

        // TODO: skip or read to end of header or re-use existing reader.

        if hdr.dtype == 0 { // MRI_UCHAR

        }
        // TODO: read data from file.
        let mgh_data = Array::from_shape_vec((1 as usize, 1 as usize, 1 as usize, 1 as usize), vec![1.0 as f32]).unwrap();

        let mgh = FsMgh {
            header : FsMghHeader::default(),
            data_mri_uchar : None,
            data_mri_int : None,
            data_mri_float : Some(mgh_data),
            data_mri_short : None,
        };
        Ok(mgh)
    }
}


/// Read an MGH or MGZ file.
pub fn read_mgh<P: AsRef<Path> + Copy>(path: P) -> Result<FsMgh> {
    FsMgh::from_file(path)
}

