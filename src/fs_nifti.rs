//! Functions for managing NIfTI-1 files (single-file `.nii` / `.nii.gz`).
//!
//! NIfTI-1 is a common format for storing 3D/4D brain images. This module only
//! supports *standard-conform* single-file NIfTI-1 images (magic `n+1`, i.e.
//! `.nii` / `.nii.gz` files). It does **not** implement the non-conformant
//! "FreeSurfer hack" (storing per-vertex surface data with an overflowing
//! `dim[1]`); such files are rejected with an error.
//!
//! The implementation closely follows the NIfTI-1 support in the `libfs` C++
//! library:
//!
//! * the endianness is auto-detected from `sizeof_hdr` (both little- and
//!   big-endian files are read correctly; files are written in big-endian
//!   byte order),
//! * the `sform` (affine) transform is preferred over the `qform`
//!   (quaternion) transform when both are present,
//! * voxel data is rescaled with `scl_slope` / `scl_inter` on read (and
//!   written with `scl_slope = 1`, `scl_inter = 0`, i.e. already-scaled data),
//! * only the data types `UINT8`, `INT16`, `INT32` and `FLOAT32` are
//!   supported (they map to the FreeSurfer `MRI_UCHAR`, `MRI_SHORT`,
//!   `MRI_INT` and `MRI_FLOAT` types).
//!
//! The NIfTI data part is stored in an [`FsMghData`], exactly like for MGH
//! files, so it is easy to convert between the two formats:
//!
//! * [`nifti_to_mgh`] converts an in-memory NIfTI image to an [`FsMgh`],
//! * [`mgh_to_nifti`] converts an in-memory [`FsMgh`] to a NIfTI image,
//! * [`nifti_to_mgh_file`] and [`mgh_to_nifti_file`] convert files directly.

use byteordered::{ByteOrdered, Endianness};
use flate2::bufread::GzDecoder;
use flate2::Compression;
use ndarray::{Array, Array2, Dim};

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::config;
use crate::error::{NeuroformatsError, Result};
use crate::fs_mgh::{
    read_mgh, write_mgh, FsMgh, FsMghData, FsMghHeader, MRI_FLOAT, MRI_INT, MRI_SHORT, MRI_UCHAR,
};
use crate::util::{checked_mul_dims, validate_finite_f32_slice};

/// The size of a NIfTI-1 header in bytes.
const NIFTI_HEADER_SIZE: usize = 348;

/// Default `vox_offset` for a single-file NIfTI-1 image without extensions
/// (348-byte header + 4-byte extension indicator).
const NIFTI_VOX_OFFSET_NO_EXT: f32 = 352.0;

/// Magic string for single-file NIfTI-1 images (`.nii`): `"n+1\0"`.
const NIFTI_MAGIC_SINGLE_FILE: [u8; 4] = *b"n+1\0";

/// NIfTI-1 `datatype` code for unsigned 8-bit integers (maps to `MRI_UCHAR`).
pub const NIFTI_DT_UINT8: i16 = 2;
/// NIfTI-1 `datatype` code for signed 16-bit integers (maps to `MRI_SHORT`).
pub const NIFTI_DT_INT16: i16 = 4;
/// NIfTI-1 `datatype` code for signed 32-bit integers (maps to `MRI_INT`).
pub const NIFTI_DT_INT32: i16 = 8;
/// NIfTI-1 `datatype` code for 32-bit IEEE-754 single-precision floats (maps to `MRI_FLOAT`).
pub const NIFTI_DT_FLOAT32: i16 = 16;

/// Models the 348-byte header of a NIfTI-1 file.
#[derive(Debug, Clone, PartialEq)]
pub struct FsNiftiHeader {
    pub sizeof_hdr: i32,
    pub dim: [i16; 8],
    pub intent_p1: f32,
    pub intent_p2: f32,
    pub intent_p3: f32,
    pub intent_code: i16,
    pub datatype: i16,
    pub bitpix: i16,
    pub slice_start: i16,
    pub pixdim: [f32; 8],
    pub vox_offset: f32,
    pub scl_slope: f32,
    pub scl_inter: f32,
    pub slice_end: i16,
    pub slice_code: u8,
    pub xyzt_units: u8,
    pub cal_max: f32,
    pub cal_min: f32,
    pub slice_duration: f32,
    pub toffset: f32,
    pub glmax: i32,
    pub glmin: i32,
    pub descrip: String,
    pub qform_code: i16,
    pub sform_code: i16,
    pub quatern_b: f32,
    pub quatern_c: f32,
    pub quatern_d: f32,
    pub qoffset_x: f32,
    pub qoffset_y: f32,
    pub qoffset_z: f32,
    pub srow_x: [f32; 4],
    pub srow_y: [f32; 4],
    pub srow_z: [f32; 4],
    pub magic: [u8; 4],
}

impl Default for FsNiftiHeader {
    fn default() -> FsNiftiHeader {
        FsNiftiHeader {
            sizeof_hdr: NIFTI_HEADER_SIZE as i32,
            dim: [4, 1, 1, 1, 1, 1, 1, 1],
            intent_p1: 0.0,
            intent_p2: 0.0,
            intent_p3: 0.0,
            intent_code: 0,
            datatype: NIFTI_DT_UINT8,
            bitpix: 8,
            slice_start: 0,
            pixdim: [1.0; 8],
            vox_offset: NIFTI_VOX_OFFSET_NO_EXT,
            scl_slope: 1.0,
            scl_inter: 0.0,
            slice_end: 0,
            slice_code: 0,
            xyzt_units: 0,
            cal_max: 0.0,
            cal_min: 0.0,
            slice_duration: 0.0,
            toffset: 0.0,
            glmax: 0,
            glmin: 0,
            descrip: String::new(),
            qform_code: 0,
            sform_code: 0,
            quatern_b: 0.0,
            quatern_c: 0.0,
            quatern_d: 0.0,
            qoffset_x: 0.0,
            qoffset_y: 0.0,
            qoffset_z: 0.0,
            srow_x: [0.0; 4],
            srow_y: [0.0; 4],
            srow_z: [0.0; 4],
            magic: NIFTI_MAGIC_SINGLE_FILE,
        }
    }
}

/// Detect the byte order of a NIfTI-1 file from the `sizeof_hdr` field.
fn detect_endianness(raw: &[u8]) -> Result<Endianness> {
    if raw.len() < 4 {
        return Err(NeuroformatsError::InvalidNiftiFormat(
            "file too small to contain a NIfTI-1 header.".to_string(),
        ));
    }
    let le = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let be = i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
    if le == NIFTI_HEADER_SIZE as i32 {
        Ok(Endianness::Little)
    } else if be == NIFTI_HEADER_SIZE as i32 {
        Ok(Endianness::Big)
    } else {
        Err(NeuroformatsError::InvalidNiftiFormat(format!(
            "invalid sizeof_hdr value {} (expected 348).",
            le
        )))
    }
}

/// Convert a fixed-length NUL-terminated byte string to a `String`.
fn nul_terminated_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Map a NIfTI-1 `datatype` code to the FreeSurfer `MRI_*` data type.
fn nifti_dtype_to_mri(datatype: i16) -> Result<i32> {
    match datatype {
        NIFTI_DT_UINT8 => Ok(MRI_UCHAR),
        NIFTI_DT_INT16 => Ok(MRI_SHORT),
        NIFTI_DT_INT32 => Ok(MRI_INT),
        NIFTI_DT_FLOAT32 => Ok(MRI_FLOAT),
        _ => Err(NeuroformatsError::UnsupportedNiftiDataType(datatype)),
    }
}

/// Map a FreeSurfer `MRI_*` data type to the NIfTI-1 `datatype` code.
fn mri_dtype_to_nifti(mri_dtype: i32) -> Result<i16> {
    match mri_dtype {
        MRI_UCHAR => Ok(NIFTI_DT_UINT8),
        MRI_SHORT => Ok(NIFTI_DT_INT16),
        MRI_INT => Ok(NIFTI_DT_INT32),
        MRI_FLOAT => Ok(NIFTI_DT_FLOAT32),
        _ => Err(NeuroformatsError::UnsupportedMriDataTypeInMgh),
    }
}

/// Determine which voxel array is set in an [`FsMghData`] and return its
/// FreeSurfer `MRI_*` data type.
fn dtype_from_data(data: &FsMghData) -> Result<i32> {
    if data.mri_uchar.is_some() {
        Ok(MRI_UCHAR)
    } else if data.mri_short.is_some() {
        Ok(MRI_SHORT)
    } else if data.mri_int.is_some() {
        Ok(MRI_INT)
    } else if data.mri_float.is_some() {
        Ok(MRI_FLOAT)
    } else {
        Err(NeuroformatsError::InvalidNiftiFormat(
            "the NIfTI data is empty (no voxel array is set).".to_string(),
        ))
    }
}

impl FsNiftiHeader {
    /// Read a NIfTI-1 header from a byte stream.
    ///
    /// It is assumed that the input is currently at the start of the header.
    /// The endianness is auto-detected from the `sizeof_hdr` field.
    pub fn from_reader<S: BufRead>(input: &mut S) -> Result<FsNiftiHeader> {
        let mut raw = [0u8; NIFTI_HEADER_SIZE];
        input.read_exact(&mut raw)?;
        Ok(Self::parse(&raw)?.0)
    }

    /// Parse a NIfTI-1 header from the raw 348 header bytes.
    ///
    /// Returns the header together with the detected file endianness (the
    /// latter is needed to read the voxel data part).
    fn parse(raw: &[u8; NIFTI_HEADER_SIZE]) -> Result<(FsNiftiHeader, Endianness)> {
        let endianness = detect_endianness(raw)?;
        let mut c = ByteOrdered::runtime(Cursor::new(&raw[..]), endianness);

        let sizeof_hdr = c.read_i32()?;
        for _ in 0..10 {
            let _ = c.read_u8()?; // data_type (unused)
        }
        for _ in 0..18 {
            let _ = c.read_u8()?; // db_name (unused)
        }
        let _extents = c.read_i32()?;
        let _session_error = c.read_i16()?;
        let _regular = c.read_u8()?;
        let _dim_info = c.read_u8()?;
        let mut dim = [0i16; 8];
        for i in 0..8 {
            dim[i] = c.read_i16()?;
        }
        let intent_p1 = c.read_f32()?;
        let intent_p2 = c.read_f32()?;
        let intent_p3 = c.read_f32()?;
        let intent_code = c.read_i16()?;
        let datatype = c.read_i16()?;
        let bitpix = c.read_i16()?;
        let slice_start = c.read_i16()?;
        let mut pixdim = [0f32; 8];
        for i in 0..8 {
            pixdim[i] = c.read_f32()?;
        }
        let vox_offset = c.read_f32()?;
        let scl_slope = c.read_f32()?;
        let scl_inter = c.read_f32()?;
        let slice_end = c.read_i16()?;
        let slice_code = c.read_u8()?;
        let xyzt_units = c.read_u8()?;
        let cal_max = c.read_f32()?;
        let cal_min = c.read_f32()?;
        let slice_duration = c.read_f32()?;
        let toffset = c.read_f32()?;
        let glmax = c.read_i32()?;
        let glmin = c.read_i32()?;
        let mut descrip_bytes = [0u8; 80];
        c.read_exact(&mut descrip_bytes)?;
        let descrip = nul_terminated_string(&descrip_bytes);
        for _ in 0..24 {
            let _ = c.read_u8()?; // aux_file (unused)
        }
        let qform_code = c.read_i16()?;
        let sform_code = c.read_i16()?;
        let quatern_b = c.read_f32()?;
        let quatern_c = c.read_f32()?;
        let quatern_d = c.read_f32()?;
        let qoffset_x = c.read_f32()?;
        let qoffset_y = c.read_f32()?;
        let qoffset_z = c.read_f32()?;
        let mut srow_x = [0f32; 4];
        for i in 0..4 {
            srow_x[i] = c.read_f32()?;
        }
        let mut srow_y = [0f32; 4];
        for i in 0..4 {
            srow_y[i] = c.read_f32()?;
        }
        let mut srow_z = [0f32; 4];
        for i in 0..4 {
            srow_z[i] = c.read_f32()?;
        }
        for _ in 0..16 {
            let _ = c.read_u8()?; // intent_name (unused)
        }
        let mut magic = [0u8; 4];
        c.read_exact(&mut magic)?;

        if sizeof_hdr != NIFTI_HEADER_SIZE as i32 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "invalid sizeof_hdr value {} (expected 348).",
                sizeof_hdr
            )));
        }
        if magic != NIFTI_MAGIC_SINGLE_FILE {
            return Err(NeuroformatsError::InvalidNiftiFormat(
                "invalid magic string; only single-file NIfTI-1 images (.nii, magic 'n+1') are supported."
                    .to_string(),
            ));
        }

        let hdr = FsNiftiHeader {
            sizeof_hdr,
            dim,
            intent_p1,
            intent_p2,
            intent_p3,
            intent_code,
            datatype,
            bitpix,
            slice_start,
            pixdim,
            vox_offset,
            scl_slope,
            scl_inter,
            slice_end,
            slice_code,
            xyzt_units,
            cal_max,
            cal_min,
            slice_duration,
            toffset,
            glmax,
            glmin,
            descrip,
            qform_code,
            sform_code,
            quatern_b,
            quatern_c,
            quatern_d,
            qoffset_x,
            qoffset_y,
            qoffset_z,
            srow_x,
            srow_y,
            srow_z,
            magic,
        };
        Ok((hdr, endianness))
    }

    /// Get the dimensions of the NIfTI data.
    ///
    /// Dimensions with values `<= 0` are treated as `1` (except `dim[1]`,
    /// which must be positive for standard-conform files).
    pub fn dim(&self) -> [usize; 4] {
        [
            self.dim[1] as usize,
            (if self.dim[2] > 0 { self.dim[2] } else { 1 }) as usize,
            (if self.dim[3] > 0 { self.dim[3] } else { 1 }) as usize,
            (if self.dim[4] > 0 { self.dim[4] } else { 1 }) as usize,
        ]
    }

    /// Compute the 4x4 `vox2ras` matrix from the header, if available.
    ///
    /// The `sform` (affine) transform is preferred over the `qform`
    /// (quaternion) transform, matching the behaviour of libfs and FreeSurfer.
    /// The returned matrix maps voxel coordinates `(i, j, k, 1)` to RAS
    /// coordinates `(x, y, z, 1)`.
    ///
    /// # Errors
    /// * `NoRasInformationInHeader` if neither `sform_code` nor `qform_code` is set.
    pub fn vox2ras(&self) -> Result<Array2<f32>> {
        let (s, t) = self
            .vox2ras_matrix()
            .ok_or(NeuroformatsError::NoRasInformationInHeader)?;
        let mut m: Array2<f32> = Array::zeros((4, 4));
        for i in 0..3 {
            for j in 0..3 {
                m[[i, j]] = s[i][j];
            }
        }
        m[[0, 3]] = t[0];
        m[[1, 3]] = t[1];
        m[[2, 3]] = t[2];
        m[[3, 3]] = 1.0;
        Ok(m)
    }

    /// Return the 3x3 rotation/scaling part and the translation part of the
    /// `vox2ras` transform, using `sform` if available and `qform` otherwise.
    fn vox2ras_matrix(&self) -> Option<([[f32; 3]; 3], [f32; 3])> {
        if self.sform_code > 0 {
            Some((
                [
                    [self.srow_x[0], self.srow_x[1], self.srow_x[2]],
                    [self.srow_y[0], self.srow_y[1], self.srow_y[2]],
                    [self.srow_z[0], self.srow_z[1], self.srow_z[2]],
                ],
                [self.srow_x[3], self.srow_y[3], self.srow_z[3]],
            ))
        } else if self.qform_code > 0 {
            let b = self.quatern_b;
            let c = self.quatern_c;
            let d = self.quatern_d;
            let a_sq = 1.0 - (b * b + c * c + d * d);
            if a_sq < 0.0 {
                return None;
            }
            let a = a_sq.sqrt();
            let qfac = if self.pixdim[0] < 0.0 { -1.0 } else { 1.0 };
            let sx = self.pixdim[1];
            let sy = self.pixdim[2];
            let sz = self.pixdim[3] * qfac;
            let r11 = a * a + b * b - c * c - d * d;
            let r12 = 2.0 * (b * c - a * d);
            let r13 = 2.0 * (b * d + a * c);
            let r21 = 2.0 * (b * c + a * d);
            let r22 = a * a + c * c - b * b - d * d;
            let r23 = 2.0 * (c * d - a * b);
            let r31 = 2.0 * (b * d - a * c);
            let r32 = 2.0 * (c * d + a * b);
            let r33 = a * a + d * d - b * b - c * c;
            Some((
                [
                    [r11 * sx, r12 * sy, r13 * sz],
                    [r21 * sx, r22 * sy, r23 * sz],
                    [r31 * sx, r32 * sy, r33 * sz],
                ],
                [self.qoffset_x, self.qoffset_y, self.qoffset_z],
            ))
        } else {
            None
        }
    }
}

/// Models a NIfTI-1 file: header plus voxel data.
///
/// The data is stored in an [`FsMghData`], the same structure used for MGH
/// files, which makes converting between the two formats trivial.
#[derive(Debug, Clone, PartialEq)]
pub struct FsNifti {
    pub header: FsNiftiHeader,
    pub data: FsMghData,
}

impl FsNifti {
    /// Read a NIfTI-1 file (`.nii` or `.nii.gz`).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<FsNifti> {
        let gz = is_nii_gz_file(path);
        let mut file = BufReader::new(File::open(path)?);

        if gz {
            let mut buf: Vec<u8> = Vec::new();
            GzDecoder::new(file).read_to_end(&mut buf)?;
            let mut cur = Cursor::new(buf);
            FsNifti::from_buf(&mut cur)
        } else {
            FsNifti::from_buf(&mut file)
        }
    }

    /// Read a NIfTI-1 file from a seekable byte stream.
    fn from_buf<S: BufRead + Seek>(input: &mut S) -> Result<FsNifti> {
        let mut raw = [0u8; NIFTI_HEADER_SIZE];
        input.read_exact(&mut raw)?;
        let (hdr, endianness) = FsNiftiHeader::parse(&raw)?;
        let data = FsNifti::data_from_reader_with_endianness(input, &hdr, endianness)?;
        Ok(FsNifti {
            header: hdr,
            data,
        })
    }

    /// Read the voxel data part of a NIfTI-1 file.
    ///
    /// It is assumed that the input is positioned at the start of the file
    /// (the endianness is detected from the first bytes of the header).
    pub fn data_from_reader<S>(file: &mut S, hdr: &FsNiftiHeader) -> Result<FsMghData>
    where
        S: BufRead + Seek,
    {
        file.seek(SeekFrom::Start(0))?;
        let mut first_bytes = [0u8; 4];
        file.read_exact(&mut first_bytes)?;
        let endianness = detect_endianness(&first_bytes)?;
        FsNifti::data_from_reader_with_endianness(file, hdr, endianness)
    }

    /// Read the voxel data part of a NIfTI-1 file with known endianness.
    fn data_from_reader_with_endianness<S>(
        file: &mut S,
        hdr: &FsNiftiHeader,
        endianness: Endianness,
    ) -> Result<FsMghData>
    where
        S: BufRead + Seek,
    {
        // Validate dimensions. Standard-conform NIfTI-1 requires dim[1] > 0;
        // the non-conformant FreeSurfer hack (negative dim[1]) is rejected.
        let dim1 = hdr.dim[1];
        if dim1 <= 0 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "dim[1] is {}; expected a positive value (non-conformant file or FreeSurfer hack).",
                dim1
            )));
        }
        let dim2 = if hdr.dim[2] > 0 { hdr.dim[2] } else { 1 };
        let dim3 = if hdr.dim[3] > 0 { hdr.dim[3] } else { 1 };
        let dim4 = if hdr.dim[4] > 0 { hdr.dim[4] } else { 1 };

        let total_elements =
            checked_mul_dims(&[dim1 as i32, dim2 as i32, dim3 as i32, dim4 as i32])?;

        let mri_dtype = nifti_dtype_to_mri(hdr.datatype)?;
        let bytes_per_element: usize = match mri_dtype {
            MRI_UCHAR => 1,
            MRI_SHORT => 2,
            MRI_INT => 4,
            MRI_FLOAT => 4,
            _ => unreachable!(),
        };
        let total_bytes = total_elements
            .checked_mul(bytes_per_element)
            .ok_or(NeuroformatsError::IntegerOverflow)?;
        if total_bytes > config::max_bytes_per_file() {
            return Err(NeuroformatsError::AllocationTooLarge);
        }

        // Validate vox_offset and available payload size.
        let file_size = file.seek(SeekFrom::End(0))?;
        let vox_offset = hdr.vox_offset;
        if !vox_offset.is_finite() || vox_offset < NIFTI_HEADER_SIZE as f32 || (vox_offset as u64) >= file_size {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "invalid vox_offset {}.",
                vox_offset
            )));
        }
        let available_bytes = file_size - vox_offset as u64;
        if (total_bytes as u64) > available_bytes {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "dimensions require {} bytes of voxel data but only {} are available.",
                total_bytes, available_bytes
            )));
        }

        file.seek(SeekFrom::Start(vox_offset as u64))?;

        let vol_dim = Dim([dim1 as usize, dim2 as usize, dim3 as usize, dim4 as usize]);
        let mut file = ByteOrdered::runtime(file, endianness);

        let slope = if hdr.scl_slope != 0.0 { hdr.scl_slope } else { 1.0 };
        let inter = hdr.scl_inter;

        let mut data_mri_uchar = None;
        let mut data_mri_short = None;
        let mut data_mri_int = None;
        let mut data_mri_float = None;

        if mri_dtype == MRI_UCHAR {
            let mut values: Vec<u8> = Vec::with_capacity(total_elements);
            for _ in 0..total_elements {
                let raw = file.read_u8()?;
                values.push((raw as f32 * slope + inter).round().clamp(0.0, 255.0) as u8);
            }
            data_mri_uchar = Some(Array::from_shape_vec(vol_dim, values).map_err(shape_err)?);
        } else if mri_dtype == MRI_SHORT {
            let mut values: Vec<i16> = Vec::with_capacity(total_elements);
            for _ in 0..total_elements {
                let raw = file.read_i16()?;
                values.push((raw as f32 * slope + inter).round() as i16);
            }
            data_mri_short = Some(Array::from_shape_vec(vol_dim, values).map_err(shape_err)?);
        } else if mri_dtype == MRI_INT {
            let mut values: Vec<i32> = Vec::with_capacity(total_elements);
            for _ in 0..total_elements {
                let raw = file.read_i32()?;
                values.push((raw as f32 * slope + inter).round() as i32);
            }
            data_mri_int = Some(Array::from_shape_vec(vol_dim, values).map_err(shape_err)?);
        } else if mri_dtype == MRI_FLOAT {
            let mut values: Vec<f32> = Vec::with_capacity(total_elements);
            for _ in 0..total_elements {
                let raw = file.read_f32()?;
                values.push(raw * slope + inter);
            }
            data_mri_float = Some(Array::from_shape_vec(vol_dim, values).map_err(shape_err)?);
        } else {
            return Err(NeuroformatsError::UnsupportedNiftiDataType(hdr.datatype));
        }

        Ok(FsMghData {
            mri_uchar: data_mri_uchar,
            mri_int: data_mri_int,
            mri_float: data_mri_float,
            mri_short: data_mri_short,
        })
    }

    /// Get the dimensions of the NIfTI data.
    pub fn dim(&self) -> [usize; 4] {
        self.header.dim()
    }

    /// Compute the `vox2ras` matrix from the header information, if available.
    ///
    /// Forwarded to [`FsNiftiHeader::vox2ras`], see there for details.
    pub fn vox2ras(&self) -> Result<Array2<f32>> {
        self.header.vox2ras()
    }
}

fn shape_err(e: ndarray::ShapeError) -> NeuroformatsError {
    NeuroformatsError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Check whether the file extension ends with ".nii.gz".
pub fn is_nii_gz_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .file_name()
        .map(|a| a.to_string_lossy().to_lowercase().ends_with(".nii.gz"))
        .unwrap_or(false)
}

/// Read a NIfTI-1 file (`.nii` or `.nii.gz`).
///
/// The NIfTI format stores images with up to 4 dimensions and is commonly
/// used for 3D magnetic resonance images (MRI) or related data. Only
/// standard-conform single-file NIfTI-1 images are supported (the FreeSurfer
/// hack for storing per-vertex data is not implemented).
///
/// The NIfTI-1 `datatype` determines where in the returned [`FsMghData`] the
/// data can be found:
///
/// * `NIFTI_DT_UINT8` (code `2`, maps to `MRI_UCHAR` / Rust `u8`)
/// * `NIFTI_DT_INT16` (code `4`, maps to `MRI_SHORT` / Rust `i16`)
/// * `NIFTI_DT_INT32` (code `8`, maps to `MRI_INT` / Rust `i32`)
/// * `NIFTI_DT_FLOAT32` (code `16`, maps to `MRI_FLOAT` / Rust `f32`)
///
/// Voxel data is rescaled with the `scl_slope` / `scl_inter` header fields on
/// read, like in libfs and FreeSurfer.
///
/// # Examples
///
/// ```no_run
/// let nifti = neuroformats::read_nifti("/path/to/brain.nii").unwrap();
/// assert_eq!(nifti.header.dim[1], 256);
/// let voxels = nifti.data.mri_uchar.unwrap();
/// ```
pub fn read_nifti<P: AsRef<Path> + Copy>(path: P) -> Result<FsNifti> {
    FsNifti::from_file(path)
}

/// Write a [`FsNifti`] struct to a file in NIfTI-1 format.
///
/// Whether plain `.nii` or gzip-compressed `.nii.gz` format is used is
/// determined from the file extension.
///
/// The file is written in big-endian byte order (the NIfTI-1 convention).
/// The data is written with `scl_slope = 1` and `scl_inter = 0`, i.e. in
/// already-rescaled form, so a file written here reads back with unchanged
/// values. The FreeSurfer hack is never produced.
pub fn write_nifti<P: AsRef<Path> + Copy>(path: P, nifti: &FsNifti) -> Result<()> {
    if is_nii_gz_file(path) {
        let mut buf: Vec<u8> = Vec::new();
        write_nifti_to(&mut buf, nifti)?;
        let file = File::create(path)?;
        let mut encoder = flate2::write::GzEncoder::new(file, Compression::default());
        encoder.write_all(&buf)?;
        encoder.finish()?;
        Ok(())
    } else {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        write_nifti_to(&mut writer, nifti)?;
        writer.flush()?;
        Ok(())
    }
}

/// Build the header that is written to disk for the given NIfTI image.
///
/// The stored header is preserved where possible (pixdim, sform/qform, ...),
/// while the fields that must be self-consistent for a standard-conform file
/// (`sizeof_hdr`, `magic`, `vox_offset`, `scl_slope`, `scl_inter`, `datatype`,
/// `bitpix`, `dim`) are (re)computed.
fn build_header_for_write(nifti: &FsNifti) -> Result<FsNiftiHeader> {
    let mut hdr = nifti.header.clone();
    hdr.sizeof_hdr = NIFTI_HEADER_SIZE as i32;
    hdr.magic = NIFTI_MAGIC_SINGLE_FILE;
    hdr.vox_offset = NIFTI_VOX_OFFSET_NO_EXT;
    hdr.scl_slope = 1.0;
    hdr.scl_inter = 0.0;

    let mri_dtype = dtype_from_data(&nifti.data)?;
    hdr.datatype = mri_dtype_to_nifti(mri_dtype)?;
    hdr.bitpix = match mri_dtype {
        MRI_UCHAR => 8,
        MRI_SHORT => 16,
        MRI_INT => 32,
        MRI_FLOAT => 32,
        _ => unreachable!(),
    };

    // Derive the dimensions from the actual voxel data array.
    let shape = match mri_dtype {
        MRI_UCHAR => nifti.data.mri_uchar.as_ref().unwrap().shape().to_vec(),
        MRI_SHORT => nifti.data.mri_short.as_ref().unwrap().shape().to_vec(),
        MRI_INT => nifti.data.mri_int.as_ref().unwrap().shape().to_vec(),
        MRI_FLOAT => nifti.data.mri_float.as_ref().unwrap().shape().to_vec(),
        _ => unreachable!(),
    };
    let mut dim = [1i16; 8];
    dim[0] = 4;
    for (i, &s) in shape.iter().enumerate().take(4) {
        if s > 32767 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "data dimension {} exceeds the NIfTI-1 int16 limit of 32767.",
                s
            )));
        }
        dim[i + 1] = s as i16;
    }
    hdr.dim = dim;

    // Fix up invalid voxel sizes.
    for i in 1..4 {
        if !hdr.pixdim[i].is_finite() || hdr.pixdim[i] <= 0.0 {
            hdr.pixdim[i] = 1.0;
        }
    }

    Ok(hdr)
}

/// Write a [`FsNifti`] struct to a byte stream in NIfTI-1 format.
fn write_nifti_to<W: Write>(f: W, nifti: &FsNifti) -> Result<()> {
    let hdr = build_header_for_write(nifti)?;
    let mri_dtype = dtype_from_data(&nifti.data)?;
    let mut f = ByteOrdered::be(f);

    f.write_i32(hdr.sizeof_hdr)?;
    for _ in 0..10 {
        f.write_u8(0)?; // data_type (unused)
    }
    for _ in 0..18 {
        f.write_u8(0)?; // db_name (unused)
    }
    f.write_i32(0)?; // extents
    f.write_i16(0)?; // session_error
    f.write_u8(0)?; // regular
    f.write_u8(0)?; // dim_info
    for i in 0..8 {
        f.write_i16(hdr.dim[i])?;
    }
    f.write_f32(hdr.intent_p1)?;
    f.write_f32(hdr.intent_p2)?;
    f.write_f32(hdr.intent_p3)?;
    f.write_i16(hdr.intent_code)?;
    f.write_i16(hdr.datatype)?;
    f.write_i16(hdr.bitpix)?;
    f.write_i16(hdr.slice_start)?;
    for i in 0..8 {
        f.write_f32(hdr.pixdim[i])?;
    }
    f.write_f32(hdr.vox_offset)?;
    f.write_f32(hdr.scl_slope)?;
    f.write_f32(hdr.scl_inter)?;
    f.write_i16(hdr.slice_end)?;
    f.write_u8(hdr.slice_code)?;
    f.write_u8(hdr.xyzt_units)?;
    f.write_f32(hdr.cal_max)?;
    f.write_f32(hdr.cal_min)?;
    f.write_f32(hdr.slice_duration)?;
    f.write_f32(hdr.toffset)?;
    f.write_i32(hdr.glmax)?;
    f.write_i32(hdr.glmin)?;
    let mut descrip = [0u8; 80];
    let db = hdr.descrip.as_bytes();
    let n = db.len().min(80);
    descrip[..n].copy_from_slice(&db[..n]);
    f.write_all(&descrip)?;
    for _ in 0..24 {
        f.write_u8(0)?; // aux_file (unused)
    }
    f.write_i16(hdr.qform_code)?;
    f.write_i16(hdr.sform_code)?;
    f.write_f32(hdr.quatern_b)?;
    f.write_f32(hdr.quatern_c)?;
    f.write_f32(hdr.quatern_d)?;
    f.write_f32(hdr.qoffset_x)?;
    f.write_f32(hdr.qoffset_y)?;
    f.write_f32(hdr.qoffset_z)?;
    for i in 0..4 {
        f.write_f32(hdr.srow_x[i])?;
    }
    for i in 0..4 {
        f.write_f32(hdr.srow_y[i])?;
    }
    for i in 0..4 {
        f.write_f32(hdr.srow_z[i])?;
    }
    for _ in 0..16 {
        f.write_u8(0)?; // intent_name (unused)
    }
    f.write_all(&hdr.magic)?;

    // 4-byte extension indicator (0 = no extensions).
    f.write_i32(0)?;

    match mri_dtype {
        MRI_UCHAR => {
            for v in nifti.data.mri_uchar.as_ref().unwrap().iter() {
                f.write_u8(*v)?;
            }
        }
        MRI_SHORT => {
            for v in nifti.data.mri_short.as_ref().unwrap().iter() {
                f.write_i16(*v)?;
            }
        }
        MRI_INT => {
            for v in nifti.data.mri_int.as_ref().unwrap().iter() {
                f.write_i32(*v)?;
            }
        }
        MRI_FLOAT => {
            for v in nifti.data.mri_float.as_ref().unwrap().iter() {
                f.write_f32(*v)?;
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

/// Convert an in-memory NIfTI-1 image to a FreeSurfer MGH image.
///
/// The voxel data is moved (not copied). The spatial transform (vox2ras) is
/// taken from the `sform` (or `qform`) header fields and converted to the MGH
/// header representation (`delta`, `mdc_raw`, `p_xyz_c`).
pub fn nifti_to_mgh(nifti: FsNifti) -> Result<FsMgh> {
    let hdr = nifti_header_to_mgh(&nifti.header)?;
    Ok(FsMgh {
        header: hdr,
        data: nifti.data,
    })
}

/// Convert a NIfTI-1 header to an MGH header.
fn nifti_header_to_mgh(h: &FsNiftiHeader) -> Result<FsMghHeader> {
    let dim1 = h.dim[1];
    if dim1 <= 0 {
        return Err(NeuroformatsError::InvalidNiftiFormat(format!(
            "dim[1] is {}; expected a positive value.",
            dim1
        )));
    }
    let dim2 = if h.dim[2] > 0 { h.dim[2] } else { 1 };
    let dim3 = if h.dim[3] > 0 { h.dim[3] } else { 1 };
    let dim4 = if h.dim[4] > 0 { h.dim[4] } else { 1 };
    let dtype = nifti_dtype_to_mri(h.datatype)?;

    let mut mgh = FsMghHeader::default();
    mgh.mgh_format_version = 1;
    mgh.dim1len = dim1 as i32;
    mgh.dim2len = dim2 as i32;
    mgh.dim3len = dim3 as i32;
    mgh.dim4len = dim4 as i32;
    mgh.dtype = dtype;
    mgh.dof = 0;

    let delta = [
        if h.pixdim[1] > 0.0 && h.pixdim[1].is_finite() {
            h.pixdim[1]
        } else {
            1.0
        },
        if h.pixdim[2] > 0.0 && h.pixdim[2].is_finite() {
            h.pixdim[2]
        } else {
            1.0
        },
        if h.pixdim[3] > 0.0 && h.pixdim[3].is_finite() {
            h.pixdim[3]
        } else {
            1.0
        },
    ];

    if let Some((s, t)) = h.vox2ras_matrix() {
        mgh.is_ras_good = 1;
        mgh.delta = delta;
        // MGH stores mdc such that vox2ras_3x3 = mdc^T * diag(delta).
        for i in 0..3 {
            for j in 0..3 {
                mgh.mdc_raw[i * 3 + j] = s[j][i] / delta[i];
            }
        }
        // p_xyz_c is the RAS coordinate of the central voxel.
        let cx = (dim1 / 2) as f32;
        let cy = (dim2 / 2) as f32;
        let cz = (dim3 / 2) as f32;
        mgh.p_xyz_c = [
            s[0][0] * cx + s[0][1] * cy + s[0][2] * cz + t[0],
            s[1][0] * cx + s[1][1] * cy + s[1][2] * cz + t[1],
            s[2][0] * cx + s[2][1] * cy + s[2][2] * cz + t[2],
        ];
        validate_finite_f32_slice(&mgh.delta, "delta")?;
        validate_finite_f32_slice(&mgh.mdc_raw, "mdc_raw")?;
        validate_finite_f32_slice(&mgh.p_xyz_c, "p_xyz_c")?;
    } else {
        mgh.is_ras_good = 0;
        mgh.delta = delta;
    }

    Ok(mgh)
}

/// Convert an in-memory FreeSurfer MGH image to a NIfTI-1 image.
///
/// The voxel data is moved (not copied). The `vox2ras` transform computed
/// from the MGH header is stored both as `sform` and as `qform` (with a
/// matching quaternion and `qfac`), like the files produced by FreeSurfer's
/// `mri_convert`.
pub fn mgh_to_nifti(mgh: FsMgh) -> Result<FsNifti> {
    let h = mgh.header;

    let mut nih = FsNiftiHeader::default();
    nih.sizeof_hdr = NIFTI_HEADER_SIZE as i32;
    nih.magic = NIFTI_MAGIC_SINGLE_FILE;
    nih.vox_offset = NIFTI_VOX_OFFSET_NO_EXT;
    nih.scl_slope = 1.0;
    nih.scl_inter = 0.0;

    let dims = [h.dim1len, h.dim2len, h.dim3len, h.dim4len];
    for &d in dims.iter() {
        if d > 32767 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "MGH dimension {} exceeds the NIfTI-1 int16 limit of 32767.",
                d
            )));
        }
    }
    nih.dim = [4, dims[0] as i16, dims[1] as i16, dims[2] as i16, dims[3] as i16, 1, 1, 1];
    nih.datatype = mri_dtype_to_nifti(h.dtype)?;
    nih.bitpix = match h.dtype {
        MRI_UCHAR => 8,
        MRI_SHORT => 16,
        MRI_INT => 32,
        MRI_FLOAT => 32,
        _ => unreachable!(),
    };

    nih.pixdim = [1.0; 8];
    for i in 0..3 {
        nih.pixdim[i + 1] = if h.delta[i] > 0.0 && h.delta[i].is_finite() {
            h.delta[i]
        } else {
            1.0
        };
    }

    if h.is_ras_good == 1 {
        let v2r = h.vox2ras()?;
        nih.sform_code = 1;
        for i in 0..4 {
            nih.srow_x[i] = v2r[[0, i]];
            nih.srow_y[i] = v2r[[1, i]];
            nih.srow_z[i] = v2r[[2, i]];
        }

        let m = [
            [v2r[[0, 0]], v2r[[0, 1]], v2r[[0, 2]]],
            [v2r[[1, 0]], v2r[[1, 1]], v2r[[1, 2]]],
            [v2r[[2, 0]], v2r[[2, 1]], v2r[[2, 2]]],
        ];
        let t = [v2r[[0, 3]], v2r[[1, 3]], v2r[[2, 3]]];
        match mat33_to_quatern(m) {
            Some((qb, qc, qd, qfac, dx, dy, dz)) => {
                nih.qform_code = 1;
                nih.quatern_b = qb;
                nih.quatern_c = qc;
                nih.quatern_d = qd;
                nih.qoffset_x = t[0];
                nih.qoffset_y = t[1];
                nih.qoffset_z = t[2];
                nih.pixdim[0] = qfac;
                nih.pixdim[1] = dx;
                nih.pixdim[2] = dy;
                nih.pixdim[3] = dz;
            }
            None => {
                nih.qform_code = 0;
            }
        }
    } else {
        nih.sform_code = 0;
        nih.qform_code = 0;
    }

    Ok(FsNifti {
        header: nih,
        data: mgh.data,
    })
}

/// Decompose a 3x3 `vox2ras` matrix into the NIfTI-1 quaternion parameters.
///
/// Returns `(quatern_b, quatern_c, quatern_d, qfac, xsize, ysize, zsize)`.
/// `qfac` is `1.0` or `-1.0` and is stored in `pixdim[0]`; `xsize`/`ysize`/
/// `zsize` are the voxel sizes derived from the column lengths. Returns `None`
/// if the matrix is degenerate (a column length is zero).
fn mat33_to_quatern(m: [[f32; 3]; 3]) -> Option<(f32, f32, f32, f32, f32, f32, f32)> {
    let xd = (m[0][0] * m[0][0] + m[1][0] * m[1][0] + m[2][0] * m[2][0]).sqrt();
    let yd = (m[0][1] * m[0][1] + m[1][1] * m[1][1] + m[2][1] * m[2][1]).sqrt();
    let zd = (m[0][2] * m[0][2] + m[1][2] * m[1][2] + m[2][2] * m[2][2]).sqrt();
    if xd == 0.0 || yd == 0.0 || zd == 0.0 {
        return None;
    }

    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let zscale = if det < 0.0 { -zd } else { zd };

    // Normalize the columns to get the pure rotation matrix.
    let r = [
        [m[0][0] / xd, m[0][1] / yd, m[0][2] / zscale],
        [m[1][0] / xd, m[1][1] / yd, m[1][2] / zscale],
        [m[2][0] / xd, m[2][1] / yd, m[2][2] / zscale],
    ];
    let (_a, b, c, d) = quaternion_from_rotation(r);
    let qfac = if det < 0.0 { -1.0 } else { 1.0 };
    Some((b, c, d, qfac, xd, yd, zd))
}

/// Extract the unit quaternion `(a, b, c, d)` from a pure 3x3 rotation matrix.
///
/// Uses Shepperd's method, which is numerically robust for all rotations.
fn quaternion_from_rotation(r: [[f32; 3]; 3]) -> (f32, f32, f32, f32) {
    let trace = r[0][0] + r[1][1] + r[2][2];
    if trace > 0.0 {
        let s = 2.0 * (trace + 1.0).sqrt();
        let a = 0.25 * s;
        let b = (r[2][1] - r[1][2]) / s;
        let c = (r[0][2] - r[2][0]) / s;
        let d = (r[1][0] - r[0][1]) / s;
        (a, b, c, d)
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = 2.0 * (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt();
        let a = (r[2][1] - r[1][2]) / s;
        let b = 0.25 * s;
        let c = (r[0][1] + r[1][0]) / s;
        let d = (r[0][2] + r[2][0]) / s;
        (a, b, c, d)
    } else if r[1][1] > r[2][2] {
        let s = 2.0 * (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt();
        let a = (r[0][2] - r[2][0]) / s;
        let b = (r[0][1] + r[1][0]) / s;
        let c = 0.25 * s;
        let d = (r[1][2] + r[2][1]) / s;
        (a, b, c, d)
    } else {
        let s = 2.0 * (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt();
        let a = (r[1][0] - r[0][1]) / s;
        let b = (r[0][2] + r[2][0]) / s;
        let c = (r[1][2] + r[2][1]) / s;
        let d = 0.25 * s;
        (a, b, c, d)
    }
}

/// Convert a NIfTI-1 file to an MGH/MGZ file, like libfs' `nifti_to_mgh`.
///
/// Reads the NIfTI-1 file (`.nii` or `.nii.gz`) and writes it as an MGH or
/// MGZ file (the output format is determined by the `.mgh` / `.mgz` file
/// extension of `mgh_path`).
///
/// # Examples
///
/// ```no_run
/// neuroformats::nifti_to_mgh_file("/path/to/brain.nii", "/path/to/brain.mgh").unwrap();
/// ```
pub fn nifti_to_mgh_file<P1, P2>(nifti_path: P1, mgh_path: P2) -> Result<()>
where
    P1: AsRef<Path> + Copy,
    P2: AsRef<Path> + Copy,
{
    let nifti = read_nifti(nifti_path)?;
    let mgh = nifti_to_mgh(nifti)?;
    write_mgh(mgh_path, &mgh)?;
    Ok(())
}

/// Convert an MGH/MGZ file to a NIfTI-1 file, the inverse of
/// [`nifti_to_mgh_file`].
///
/// Reads the MGH/MGZ file and writes it as a NIfTI-1 file (`.nii` or
/// `.nii.gz`, determined by the extension of `nifti_path`).
///
/// # Examples
///
/// ```no_run
/// neuroformats::mgh_to_nifti_file("/path/to/brain.mgz", "/path/to/brain.nii").unwrap();
/// ```
pub fn mgh_to_nifti_file<P1, P2>(mgh_path: P1, nifti_path: P2) -> Result<()>
where
    P1: AsRef<Path> + Copy,
    P2: AsRef<Path> + Copy,
{
    let mgh = read_mgh(mgh_path)?;
    let nifti = mgh_to_nifti(mgh)?;
    write_nifti(nifti_path, &nifti)?;
    Ok(())
}

impl fmt::Display for FsNifti {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "NIfTI-1 volume data with dim {}, {}, {}, {} and datatype {}.",
            self.header.dim[1],
            self.header.dim[2],
            self.header.dim[3],
            self.header.dim[4],
            self.header.datatype
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::{assert_abs_diff_eq, AbsDiffEq};
    use tempfile::tempdir;

    const NII_FILE: &str = "resources/subjects_dir/subject1/mri/brain.nii";
    const MGZ_FILE: &str = "resources/subjects_dir/subject1/mri/brain.mgz";

    #[test]
    fn the_brain_nii_file_can_be_read() {
        let nifti = read_nifti(NII_FILE).unwrap();

        assert_eq!(nifti.header.sizeof_hdr, 348);
        assert_eq!(nifti.header.dim[0], 3);
        assert_eq!(nifti.header.dim[1], 256);
        assert_eq!(nifti.header.dim[2], 256);
        assert_eq!(nifti.header.dim[3], 256);
        assert_eq!(nifti.header.datatype, NIFTI_DT_UINT8);
        assert_eq!(nifti.header.bitpix, 8);
        assert_eq!(nifti.header.sform_code, 1);
        assert_eq!(nifti.header.qform_code, 1);

        let data = nifti.data.mri_uchar.unwrap();
        assert_eq!(data.ndim(), 4);
        assert_eq!(data[[99, 99, 99, 0]], 77);
        assert_eq!(data[[109, 109, 109, 0]], 71);
        assert_eq!(data[[0, 0, 0, 0]], 0);
        assert_eq!(data.mapv(|a| a as i32).sum(), 121035479);
    }

    #[test]
    fn the_brain_nii_vox2ras_matches_the_mgz() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let nii_v2r = nifti.vox2ras().unwrap();

        let mgh = read_mgh(MGZ_FILE).unwrap();
        let mgh_v2r = mgh.vox2ras().unwrap();

        assert_eq!(nii_v2r.len(), 16);
        assert_abs_diff_eq!(nii_v2r, mgh_v2r, epsilon = 1e-2);
    }

    #[test]
    fn the_brain_nii_can_be_converted_to_mgh_matching_the_mgz() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let mgh_from_nii = nifti_to_mgh(nifti).unwrap();

        let mgh = read_mgh(MGZ_FILE).unwrap();
        assert_eq!(mgh_from_nii.header.dim1len, mgh.header.dim1len);
        assert_eq!(mgh_from_nii.header.dim2len, mgh.header.dim2len);
        assert_eq!(mgh_from_nii.header.dim3len, mgh.header.dim3len);
        assert_eq!(mgh_from_nii.header.dim4len, mgh.header.dim4len);
        assert_eq!(mgh_from_nii.header.dtype, mgh.header.dtype);
        assert_eq!(mgh_from_nii.header.is_ras_good, mgh.header.is_ras_good);
        assert!(mgh_from_nii.header.delta.abs_diff_eq(&mgh.header.delta, 1e-4));
        assert!(mgh_from_nii.header.mdc_raw.abs_diff_eq(&mgh.header.mdc_raw, 1e-4));
        assert!(mgh_from_nii.header.p_xyz_c.abs_diff_eq(&mgh.header.p_xyz_c, 1e-4));

        // The voxel data is identical to the original MGH file.
        assert_eq!(
            mgh_from_nii.data.mri_uchar.unwrap(),
            mgh.data.mri_uchar.unwrap()
        );
    }

    #[test]
    fn a_nifti_file_can_be_written_and_reread() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let dir = tempdir().unwrap();
        let tfile = dir.path().join("temp-file.nii");
        write_nifti(&tfile, &nifti).unwrap();

        let nifti_re = read_nifti(&tfile).unwrap();
        assert_eq!(nifti_re.header.dim[0], 4);
        assert_eq!(nifti_re.header.dim[1], 256);
        assert_eq!(nifti_re.header.dim[2], 256);
        assert_eq!(nifti_re.header.dim[3], 256);
        assert_eq!(nifti_re.header.dim[4], 1);
        assert_eq!(nifti_re.header.datatype, NIFTI_DT_UINT8);

        let data = nifti_re.data.mri_uchar.unwrap();
        assert_eq!(data[[99, 99, 99, 0]], 77);
        assert_eq!(data[[109, 109, 109, 0]], 71);
        assert_eq!(data.mapv(|a| a as i32).sum(), 121035479);
    }

    #[test]
    fn a_nifti_file_can_be_written_and_reread_as_nii_gz() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let dir = tempdir().unwrap();
        let tfile = dir.path().join("temp-file.nii.gz");
        write_nifti(&tfile, &nifti).unwrap();

        let nifti_re = read_nifti(&tfile).unwrap();
        assert_eq!(nifti_re.header.datatype, NIFTI_DT_UINT8);
        assert_eq!(
            nifti_re.data.mri_uchar.unwrap().mapv(|a| a as i32).sum(),
            121035479
        );
    }

    #[test]
    fn an_mgh_file_can_be_converted_to_nifti_and_back() {
        let mgh_orig = read_mgh(MGZ_FILE).unwrap();
        let nifti = mgh_to_nifti(mgh_orig).unwrap();
        let dir = tempdir().unwrap();
        let tfile = dir.path().join("temp-file.nii");
        write_nifti(&tfile, &nifti).unwrap();

        let nifti_re = read_nifti(&tfile).unwrap();
        let mgh_re = nifti_to_mgh(nifti_re).unwrap();

        let mgh = read_mgh(MGZ_FILE).unwrap();
        assert_eq!(mgh_re.header.dim1len, 256);
        assert_eq!(mgh_re.header.is_ras_good, 1);
        assert!(mgh_re.header.delta.abs_diff_eq(&mgh.header.delta, 1e-3));
        assert!(mgh_re.header.mdc_raw.abs_diff_eq(&mgh.header.mdc_raw, 1e-3));
        assert!(mgh_re.header.p_xyz_c.abs_diff_eq(&mgh.header.p_xyz_c, 1e-3));
        assert_eq!(
            mgh_re.data.mri_uchar.unwrap(),
            mgh.data.mri_uchar.unwrap()
        );
    }

    #[test]
    fn nifti_file_can_be_converted_to_mgh_file() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("converted.mgh");
        nifti_to_mgh_file(NII_FILE, &out).unwrap();

        let mgh = read_mgh(&out).unwrap();
        assert_eq!(mgh.header.dim1len, 256);
        assert_eq!(mgh.header.is_ras_good, 1);
        assert_eq!(
            mgh.data.mri_uchar.unwrap().mapv(|a| a as i32).sum(),
            121035479
        );
    }

    #[test]
    fn mgh_file_can_be_converted_to_nifti_file() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("converted.nii");
        mgh_to_nifti_file(MGZ_FILE, &out).unwrap();

        let nifti = read_nifti(&out).unwrap();
        assert_eq!(nifti.header.dim[1], 256);
        assert_eq!(nifti.header.sform_code, 1);
        assert_eq!(
            nifti.data.mri_uchar.unwrap().mapv(|a| a as i32).sum(),
            121035479
        );
    }

    #[test]
    fn nifti_header_rejects_bad_sizeof_hdr() {
        use std::io::Cursor;
        let mut buf = vec![0u8; 348];
        buf[0] = 0x01; // sizeof_hdr = 1 (invalid)
        let mut c = Cursor::new(buf);
        assert!(FsNiftiHeader::from_reader(&mut c).is_err());
    }

    #[test]
    fn nifti_header_rejects_bad_magic() {
        use std::io::Cursor;
        let mut buf = Vec::new();
        buf.extend_from_slice(&348i32.to_le_bytes());
        buf.resize(348, 0);
        buf[344] = b'x';
        buf[345] = b'x';
        buf[346] = b'x';
        let mut c = Cursor::new(buf);
        assert!(FsNiftiHeader::from_reader(&mut c).is_err());
    }

    #[test]
    fn nifti_rejects_negative_dim1_free_surfer_hack() {
        use std::io::Cursor;
        let mut buf = vec![0u8; 348];
        buf[0..4].copy_from_slice(&348i32.to_le_bytes()); // sizeof_hdr
        buf[40..42].copy_from_slice(&3i16.to_le_bytes()); // dim[0] = 3
        buf[42..44].copy_from_slice(&(-1i16).to_le_bytes()); // dim[1] = -1 (FS hack)
        buf[44..46].copy_from_slice(&1i16.to_le_bytes()); // dim[2]
        buf[46..48].copy_from_slice(&1i16.to_le_bytes()); // dim[3]
        buf[48..50].copy_from_slice(&1i16.to_le_bytes()); // dim[4]
        buf[70..72].copy_from_slice(&2i16.to_le_bytes()); // datatype = UINT8
        buf[72..74].copy_from_slice(&8i16.to_le_bytes()); // bitpix
        buf[108..112].copy_from_slice(&352f32.to_le_bytes()); // vox_offset
        buf[344..348].copy_from_slice(b"n+1\0"); // magic
        buf.extend_from_slice(&[0u8; 64]); // some data bytes

        let mut c = Cursor::new(buf);
        assert!(FsNifti::from_buf(&mut c).is_err());
    }
}
