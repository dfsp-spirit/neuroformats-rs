//! Functions for managing FreeSurfer brain volumes or other 3D or 4D data in binary 'MGH' files.

use flate2::bufread::GzDecoder;
use byteordered::{ByteOrdered};
use ndarray::{Array, Array1, Array2, Array4, Dim, array};


use std::{fs::File};
use std::io::{BufReader, Read};
use std::path::{Path};

use crate::error::{NeuroformatsError, Result};

pub const MGH_VERSION_CODE: i32 = 1;

pub const MGH_DATATYPE_NAMES : [&str; 4] = ["MRI_UCHAR", "MRI_INT", "MRI_FLOAT", "MRI_SHORT"];
pub const MGH_DATATYPE_CODES : [i32; 4] = [0, 1, 3, 4];
pub const MRI_UCHAR : i32 = 0;
pub const MRI_INT : i32 = 1;
pub const MRI_FLOAT : i32 = 3;
pub const MRI_SHORT : i32 = 4;

pub const MGH_DATA_START : i32 = 284; // The index in bytes where the data part starts in an MGH file.

/// Models the header of a FreeSurfer MGH file containing a brain volume.
#[derive(Debug, Clone, PartialEq)]
pub struct FsMghHeader {
    pub mgh_format_version: i32,
    pub dim1len: i32,
    pub dim2len: i32,
    pub dim3len: i32,
    pub dim4len: i32,  // aka "num_frames", this typically is the time dimension.
    pub dtype: i32,
    pub dof: i32,
    pub is_ras_good: i16,
    pub delta: [f32; 3],
    pub mdc_raw: [f32; 9],
    pub p_xyz_c: [f32; 3],
}

/// Models the data part of a FreeSurfer MGH file.
#[derive(Debug, Clone, PartialEq)]
pub struct FsMghData {
    pub data_mri_uchar: Option<Array4<u8>>,
    pub data_mri_float: Option<Array4<f32>>,
    pub data_mri_int: Option<Array4<i32>>,
    pub data_mri_short: Option<Array4<i16>>,
}

/// Models a FreeSurfer MGH file.
#[derive(Debug, Clone, PartialEq)]
pub struct FsMgh {
    pub header: FsMghHeader,
    pub data: FsMghData
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
        let gz = is_mgz_file(&path);
        let mut file = BufReader::new(File::open(path)?);

        if gz {
            FsMghHeader::from_reader(&mut GzDecoder::new(file))
        } else {
            FsMghHeader::from_reader(&mut file)
        }
        
    }


    /// Read an MGH header from the given byte stream.
    /// It is assumed that the input is currently at the start of the
    /// header.
    pub fn from_reader<S>(input: &mut S) -> Result<FsMghHeader> where S: Read,
    {
        let mut hdr = FsMghHeader::default();
    
        let mut input = ByteOrdered::be(input);

        hdr.mgh_format_version = input.read_i32()?;

        if hdr.mgh_format_version != MGH_VERSION_CODE {
            return Err(NeuroformatsError::InvalidFsMghFormat);
        }

        hdr.dim1len = input.read_i32()?;
        hdr.dim2len = input.read_i32()?;
        hdr.dim3len = input.read_i32()?;
        hdr.dim4len = input.read_i32()?;

        hdr.dtype = input.read_i32()?;
        hdr.dof = input.read_i32()?;

        hdr.is_ras_good = input.read_i16()?;

        hdr.delta = [f32::NAN; 3];
        hdr.mdc_raw = [f32::NAN; 9];
        hdr.p_xyz_c = [f32::NAN; 3];

        if hdr.is_ras_good == 1 as i16 {            
            for idx in 0..3 { hdr.delta[idx] = input.read_f32()?; }
            for idx in 0..9 { hdr.mdc_raw[idx] = input.read_f32()?; }
            for idx in 0..3 { hdr.p_xyz_c[idx] = input.read_f32()?; }
        }        
        Ok(hdr)
    }

    /// Get dimensions of the MGH data.
    pub fn dim(&self) -> [usize; 4] {
        [self.dim1len as usize, self.dim2len as usize, self.dim3len as usize, self.dim4len as usize]
    }


    /// Compute the vox2ras matrix from the RAS data in the header, if available.
    ///
    /// The vox2ras matrix is a 4x4 f32 matrix. You can use it to find the RAS coordinates of a voxel
    /// using matrix multiplication.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ndarray::{Array1, array};
    /// let mgh = neuroformats::read_mgh("/path/to/subjects_dir/subject1/mri/brian.mgz").unwrap();
    /// let vox2ras = mgh.vox2ras().unwrap();
    /// let my_voxel_ijk : Array1<f32> = array![32.0, 32.0, 32.0];
    /// let my_voxel_ras = vox2ras.dot(&my_voxel_ijk);
    /// ```
    pub fn vox2ras(&self) -> Result<Array2<f32>> {
        if self.is_ras_good != 1 as i16 {
            return Err(NeuroformatsError::NoRasInformationInHeader);
        }

        // Create zero-matrix with voxel sizes along diagonal for scaling
        let mut d : Array2<f32> = Array::zeros((3, 3));
        d[[0, 0]] = self.delta[0]; // delta holds the voxel size in mm along the 3 dimensions (xsize, ysize, zsize)
        d[[1, 1]] = self.delta[1];
        d[[2, 2]] = self.delta[2];

        // println!("d: {}", d);  // okay

        let mdc_mat = Array2::from_shape_vec((3, 3), self.mdc_raw.to_vec()).unwrap();

        //println!("mdc_mat: {}", mdc_mat); 
        let mdc_scaled : Array2<f32> = mdc_mat.dot(&d);          // Scaled by the voxel dimensions (xsize, ysize, zsize): okay

        // The IJK voxel indices of the center.
        let p_crs_c : Array1<f32> = array![(self.dim1len/2) as f32, (self.dim2len/2) as f32, (self.dim3len/2) as f32]; // CRS indices of the center voxel

        //println!("p_crs_c: {} {} {}", p_crs_c[0], p_crs_c[1], p_crs_c[2]); // 128, 128, 128: okay

        // The RAS coordinates (aka x,y,z) of the center.
        let p_xyz_c : Array1<f32> = array![self.p_xyz_c[0], self.p_xyz_c[1], self.p_xyz_c[2]];

        println!("mdc_scaled: {}", mdc_scaled);

        println!("mdc_scaled.dot(&p_crs_c) = {}", mdc_scaled.t().dot(&p_crs_c)); // -128, -128, -128 BUT should be -128, 128, -128 => mdc_sclaed is wrong

        let p_xyz_0 : Array1<f32> = p_xyz_c - (mdc_scaled.t().dot(&p_crs_c)); // The x,y,z location at CRS=0,0,0 (also known as P0 RAS or 'first voxel RAS').

        // Plug everything together into the 4x4 vox2ras matrix:
        let mut m : Array2<f32> = Array::zeros((4, 4));

        // Set upper left 3x3 matrix to mdc_scaled.
        for i in 0..3 {
            for j in 0..3 {
                m[[i, j]] = mdc_scaled[[i, j]];
            }
        }
        m[[3, 0]] = p_xyz_0[0];  // Set last column to p_xyz_0
        m[[3, 1]] = p_xyz_0[1];
        m[[3, 2]] = p_xyz_0[2];
        m[[3, 3]] = 1.;          // Set last row to affine 0, 0, 0, 1. (only the last 1 needs manipulation)

        Ok(m)
    }
}


impl FsMgh {

    /// Read an MGH or MGZ file.
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsMgh> {

        let hdr : FsMghHeader = FsMghHeader::from_file(path)?;

        let gz = is_mgz_file(&path);
        let mut file = BufReader::new(File::open(path)?);

        let data = 
        if gz {
            FsMgh::data_from_reader(&mut GzDecoder::new(file), &hdr)?
        } else {
            FsMgh::data_from_reader(&mut file, &hdr)?
        };

        let mgh = FsMgh {
            header : hdr,
            data : data,
        };
        Ok(mgh)
    }


    /// Read MGH data from a reader. It is assumed that position is before the header.
    pub fn data_from_reader<S>(file: &mut S, hdr: &FsMghHeader) -> Result<FsMghData> where S: Read, {

        let vol_dim = Dim([hdr.dim1len as usize, hdr.dim2len as usize, hdr.dim3len as usize, hdr.dim4len as usize]);

        let mut file = ByteOrdered::be(file);

        // Skip or read to end of header.
        for _ in 1..=MGH_DATA_START {
            let _discarded = file.read_u8()?;
        }

        let mut data_mri_uchar = None;
        let mut data_mri_int = None;
        let mut data_mri_float = None;
        let mut data_mri_short = None;

        let num_voxels : usize = (hdr.dim1len * hdr.dim2len * hdr.dim3len * hdr.dim4len) as usize; 

        if hdr.dtype == MRI_UCHAR {
            let mut mgh_data : Vec<u8> = Vec::with_capacity(num_voxels);
            for _ in 1..=num_voxels {
                mgh_data.push(file.read_u8()?);
            }
            data_mri_uchar = Some(Array::from_shape_vec(vol_dim, mgh_data).unwrap());
        } else if hdr.dtype == MRI_INT {
            let mut mgh_data : Vec<i32> = Vec::with_capacity(num_voxels);
            for _ in 1..=num_voxels {
                mgh_data.push(file.read_i32()?);
            }
            data_mri_int = Some(Array::from_shape_vec(vol_dim, mgh_data).unwrap());
        } else if hdr.dtype == MRI_FLOAT {
            let mut mgh_data : Vec<f32> = Vec::with_capacity(num_voxels);
            for _ in 1..=num_voxels {
                mgh_data.push(file.read_f32()?);
            }
            data_mri_float = Some(Array::from_shape_vec(vol_dim, mgh_data).unwrap());
        } else if hdr.dtype == MRI_SHORT {
            let mut mgh_data : Vec<i16> = Vec::with_capacity(num_voxels);
            for _ in 1..=num_voxels {
                mgh_data.push(file.read_i16()?);
            }
            data_mri_short = Some(Array::from_shape_vec(vol_dim, mgh_data).unwrap());
        } else {
            return Err(NeuroformatsError::UnsupportedMriDataTypeInMgh);
        }

        let mgh_data = FsMghData {
            data_mri_uchar : data_mri_uchar,
            data_mri_int : data_mri_int,
            data_mri_float : data_mri_float,
            data_mri_short : data_mri_short,
        };
        Ok(mgh_data)
    }

    /// Get dimensions of the MGH data.
    pub fn dim(&self) -> [usize; 4] {
        self.header.dim()
    }


    /// Compute the vox2ras matrix from the header information, if available.
    ///
    /// Forwarded to [`FsMghHeader::vox2ras`], see there for details.
    pub fn vox2ras(&self) -> Result<Array2<f32>> {
        self.header.vox2ras()
    }
}


/// Check whether the file extension ends with ".mgz".
pub fn is_mgz_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    path.as_ref()
        .file_name()
        .map(|a| a.to_string_lossy().ends_with(".mgz"))
        .unwrap_or(false)
}


/// Read an MGH or MGZ file.
///
/// The MGH format stores images with up to 4 dimensions. It is typically used to
/// store voxels of 3D magnetic resonance images (MRI) or related data like results from statistical
/// analyses of these images. By convention, the 4th dimension is the time dimension, and it is often unused.
/// The format can also be used to store 1D per-vertex data from surface-based analyses, either for a
/// single subject (in which case only the 1st dimension is used), or for a group (in which case the first and
/// second dimensions are used).
///
/// # Examples
///
/// ```no_run
/// let mgh = neuroformats::read_mgh("/path/to/subjects_dir/subject1/mri/brian.mgz").unwrap();
/// ```
pub fn read_mgh<P: AsRef<Path> + Copy>(path: P) -> Result<FsMgh> {
    FsMgh::from_file(path)
}


#[cfg(test)]
mod test { 
    use super::*;

    #[test]
    fn the_brain_mgz_file_can_be_read() {
        const MGZ_FILE: &str = "resources/subjects_dir/subject1/mri/brain.mgz";
        let mgh = read_mgh(MGZ_FILE).unwrap();

        // Test MGH header.
        assert_eq!(mgh.header.dim1len, 256);
        assert_eq!(mgh.header.dim2len, 256);
        assert_eq!(mgh.header.dim3len, 256);
        assert_eq!(mgh.header.dim4len, 1);
        assert_eq!(mgh.header.dtype, MRI_UCHAR);
        assert_eq!(mgh.header.is_ras_good, 1);

        let expected_delta : Array1<f32> = array![1.0, 1.0, 1.0];
        let expected_mdc : Array2<f32> = Array2::from_shape_vec((3, 3), [-1., 0., 0., 0., 0., -1., 0., 1., 0.].to_vec()).unwrap();
        let expected_p_xyz_c : Array1<f32> = array![-0.49995422, 29.372742, -48.90473];

        let delta : Array1<f32> = Array1::from_vec(mgh.header.delta.to_vec());
        let mdc : Array2<f32> = Array2::from_shape_vec((3, 3), mgh.header.mdc_raw.to_vec()).unwrap();
        let p_xyz_c : Array1<f32> = Array1::from_vec(mgh.header.p_xyz_c.to_vec());

        assert!(delta.all_close(&expected_delta, 1e-5));
        assert!(mdc.all_close(&expected_mdc, 1e-5));
        assert!(p_xyz_c.all_close(&expected_p_xyz_c, 1e-5));

        // Test MGH data.
        let data = mgh.data.data_mri_uchar.unwrap();
        assert_eq!(data.ndim(), 4);
        assert_eq!(data[[99, 99, 99, 0]], 77);   // try on command line: mri_info --voxel 99 99 99 resources/subjects_dir/subject1/mri/brain.mgz
        assert_eq!(data[[109, 109, 109, 0]], 71);
        assert_eq!(data[[0, 0, 0, 0]], 0);

        assert_eq!(data.mapv(|a| a as i32).sum(), 121035479);
    }

    #[test]
    fn the_vox2ras_matrix_can_be_computed() {
        const MGZ_FILE: &str = "resources/subjects_dir/subject1/mri/brain.mgz";
        let mgh = read_mgh(MGZ_FILE).unwrap();

        // Test vox2ras computation
        let vox2ras = mgh.header.vox2ras().unwrap();
        assert_eq!(vox2ras.len(), 16);

        let expected_vox2ras_ar : Vec<f32> = [-1., 0., 0., 0., 0., 0., -1. ,0. ,0., 1., 0., 0., 127.5, -98.6273, 79.0953, 1.].to_vec();
        let expected_vox2ras = Array2::from_shape_vec((4, 4), expected_vox2ras_ar).unwrap();

        println!("expected v2r: {}", expected_vox2ras);
        println!("found v2r: {}", vox2ras);

        assert!(vox2ras.all_close(&expected_vox2ras, 1e-2));
    }

    #[test]
    fn the_demo_mgh_file_can_be_read() {
        const MGH_FILE: &str = "resources/mgh/tiny.mgh";
        let mgh = read_mgh(MGH_FILE).unwrap();

        assert_eq!(mgh.header.dim1len, 3);
        assert_eq!(mgh.header.dim2len, 3);
        assert_eq!(mgh.header.dim3len, 3);
        assert_eq!(mgh.header.dim4len, 1);

        assert_eq!(mgh.dim(), [3 as usize, 3 as usize, 3 as usize, 1 as usize]);
        assert_eq!(mgh.header.dim(), [3 as usize, 3 as usize, 3 as usize, 1 as usize]);

        assert_eq!(mgh.header.is_ras_good, -1);
    }
}
