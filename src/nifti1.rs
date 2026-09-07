//! Functions for managing brain volumes or other 3D or 4D data in NIfTI-1 format ('.nii' and '.nii.gz' files).
//!
//! Only standard-compliant, single-file NIfTI-1 files are supported:
//!
//! * Single-file `.nii` files (magic string `n+1\0`) and their gzip-compressed variant `.nii.gz`.
//! * Both big- and little-endian files can be read. Files are written in big-endian byte order.
//! * The voxel data is held in the crate's [`FsMgh`] volume model, which makes converting between
//!   NIfTI-1 and the FreeSurfer MGH/MGZ formats straightforward: read a NIfTI file with
//!   [`read_nifti`] and get the volume via [`Nifti1::to_mgh`], or wrap an existing [`FsMgh`] volume
//!   (e.g. one read from an MGH/MGZ file) with [`Nifti1::from_mgh`] and write it out with
//!   [`write_nifti`].
//!
//! The following NIfTI-1 data types are supported, since they map directly onto the data types the
//! MGH volume model supports: `DT_UINT8` (2), `DT_INT16` (4), `DT_INT32` (8) and `DT_FLOAT32` (16).
//!
//! Not supported (an error is returned): two-file `.hdr`/`.img` pairs (magic `ni1\0`), more than 4
//! dimensions, and the FreeSurfer "hack" of storing surface data in NIfTI files (negative `dim[1]`).
//!
//! # Coordinate systems
//!
//! A NIfTI file describes the mapping from voxel indices to world (RAS) coordinates via either an
//! affine transform stored in `srow_x/y/z` (the *s-form*, preferred) or a quaternion stored in
//! `quatern_b/c/d` together with `pixdim` (the *q-form*). On read, the s-form is preferred and the
//! q-form is used as a fallback. The transform is decomposed into the [`FsMghHeader`] fields
//! `delta` (voxel sizes in mm), `mdc_raw` (unit direction cosines of the 3 volume axes) and
//! `p_xyz_c` (RAS coordinates of the center voxel) such that [`FsMghHeader::vox2ras`] reconstructs
//! the exact transform stored in the NIfTI file. On write, both an s-form and a q-form are stored,
//! encoding the same transform.
//!
//! Note that the NIfTI s-form/q-form translation (and the `qoffset_*` fields) refer to the RAS
//! coordinates of voxel (0, 0, 0), which is not necessarily equal to the MGH `p_xyz_c` (the center
//! voxel). The conversion in this module handles this correctly, matching what FreeSurfer's
//! `mri_convert` produces.
//!
//! # Examples
//!
//! Read a NIfTI file and convert it to an MGH volume:
//!
//! ```no_run
//! use neuroformats::{read_nifti, write_mgh};
//!
//! let nifti = read_nifti("/path/to/volume.nii").unwrap();
//! let mgh = nifti.to_mgh();
//! // Now write the same volume as a FreeSurfer MGH/MGZ file:
//! write_mgh("/path/to/volume.mgz", &mgh).unwrap();
//! ```
//!
//! Convert an MGH/MGZ file to NIfTI:
//!
//! ```no_run
//! use neuroformats::{read_mgh, Nifti1, write_nifti};
//!
//! let mgh = read_mgh("/path/to/volume.mgz").unwrap();
//! let nifti = Nifti1::from_mgh(mgh).unwrap();
//! write_nifti("/path/to/volume.nii.gz", &nifti).unwrap();
//! ```

use byteordered::byteorder::{BigEndian, ByteOrder, LittleEndian};
use byteordered::{ByteOrdered, Endianness};
use flate2::bufread::GzDecoder;
use flate2::Compression;
use ndarray::{Array, Dim};

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::config;
use crate::error::{NeuroformatsError, Result};
use crate::fs_mgh::{FsMgh, FsMghData, FsMghHeader, MRI_FLOAT, MRI_INT, MRI_SHORT, MRI_UCHAR};
use crate::util::{checked_mul_dims, validate_finite_f32_slice};

/// The size of the NIfTI-1 header, in bytes.
pub const NIFTI_HEADER_SIZE_IN_BYTES: usize = 348;

/// The size of the NIfTI-1 header plus the 4-byte extension indicator, in bytes.
pub const NIFTI_HEADER_PLUS_EXT_SIZE_IN_BYTES: usize = 352;

/// The value of the `vox_offset` field written by this crate (the default offset of the voxel
/// data, right after the header and the 4-byte extension indicator).
pub const NIFTI_DEFAULT_VOX_OFFSET: f32 = 352.0;

/// NIfTI-1 data type code for unsigned 8-bit integer data.
pub const DT_UINT8: i16 = 2;
/// NIfTI-1 data type code for signed 16-bit integer data.
pub const DT_INT16: i16 = 4;
/// NIfTI-1 data type code for signed 32-bit integer data.
pub const DT_INT32: i16 = 8;
/// NIfTI-1 data type code for 32-bit float data.
pub const DT_FLOAT32: i16 = 16;

/// NIfTI-1 xform code meaning "no transform".
pub const XFORM_UNKNOWN: i16 = 0;
/// NIfTI-1 xform code meaning "Scanner Anat".
pub const XFORM_SCANNER_ANAT: i16 = 1;

/// Magic string of a single-file NIfTI-1 volume (`n+1\0`).
const MAGIC_SINGLE_FILE: [u8; 4] = [b'n', b'+', b'1', 0];
/// Magic string of a two-file NIfTI-1 volume (`ni1\0`), i.e. a `.hdr`/`.img` pair.
const MAGIC_TWO_FILE: [u8; 4] = [b'n', b'i', b'1', 0];

/// A scale factor that is added (and multiplied) to every raw voxel value read from a NIfTI file
/// before it is converted to the target data type. A `scl_slope` of 0 means "no scaling" and is
/// interpreted as a slope of 1, as specified by the NIfTI-1 standard.
#[derive(Debug, Clone, Copy)]
struct VolumeScaling {
    slope: f32,
    inter: f32,
}

impl VolumeScaling {
    /// Build the effective scaling from the header fields `scl_slope` and `scl_inter`.
    fn from_header(scl_slope: f32, scl_inter: f32) -> Result<VolumeScaling> {
        validate_finite_f32_slice(&[scl_slope, scl_inter], "scl_slope/scl_inter")?;
        let slope = if scl_slope == 0.0 { 1.0 } else { scl_slope };
        Ok(VolumeScaling {
            slope,
            inter: scl_inter,
        })
    }

    /// Whether this scaling is the identity (no transformation of the raw values is needed).
    fn is_identity(&self) -> bool {
        self.slope == 1.0 && self.inter == 0.0
    }
}

/// Models the header of a NIfTI-1 file (the single-file `.nii` variant).
///
/// The header is exactly 348 bytes long and contains, among others, the volume dimensions, the
/// voxel data type, the voxel sizes (`pixdim`), and the spatial transform from voxel to world
/// coordinates, given either as an affine transform (s-form) or as a quaternion (q-form).
///
/// All fields are stored in the same layout as on disk; fields that are unused by this crate (like
/// the `data_type` and `db_name` legacy fields) are kept as raw byte arrays so that no information
/// is lost when a file is read and written back.
#[derive(Debug, Clone, PartialEq)]
pub struct Nifti1Header {
    /// The size of the header, always 348.
    pub sizeof_hdr: i32,
    /// Unused in the NIfTI-1 standard.
    pub data_type: [u8; 10],
    /// Unused in the NIfTI-1 standard.
    pub db_name: [u8; 18],
    /// Unused in the NIfTI-1 standard.
    pub extents: i32,
    /// Unused in the NIfTI-1 standard.
    pub session_error: i16,
    /// Unused in the NIfTI-1 standard.
    pub regular: u8,
    /// Slice ordering information, unused here.
    pub dim_info: u8,
    /// Volume dimensions: `dim[0]` = number of dimensions, `dim[1..4]` = voxel counts per axis.
    pub dim: [i16; 8],
    /// Intent parameters, unused here.
    pub intent_p1: f32,
    /// Intent parameters, unused here.
    pub intent_p2: f32,
    /// Intent parameters, unused here.
    pub intent_p3: f32,
    /// Intent code, unused here.
    pub intent_code: i16,
    /// The NIfTI-1 voxel data type, one of the `DT_*` constants.
    pub datatype: i16,
    /// The number of bits per voxel.
    pub bitpix: i16,
    /// First slice index, unused here.
    pub slice_start: i16,
    /// Voxel sizes: `pixdim[0]` holds the q-form `qfac` (+-1), `pixdim[1..3]` the voxel sizes in mm.
    pub pixdim: [f32; 8],
    /// Byte offset to the start of the voxel data, relative to the start of the file.
    pub vox_offset: f32,
    /// Scaling slope. A value of 0 means no scaling (interpreted as 1).
    pub scl_slope: f32,
    /// Scaling intercept.
    pub scl_inter: f32,
    /// Last slice index, unused here.
    pub slice_end: i16,
    /// Slice timing code, unused here.
    pub slice_code: u8,
    /// Units of the pixdim values, unused here.
    pub xyzt_units: u8,
    /// Calibration maximum, unused here.
    pub cal_max: f32,
    /// Calibration minimum, unused here.
    pub cal_min: f32,
    /// Slice duration, unused here.
    pub slice_duration: f32,
    /// Time offset, unused here.
    pub toffset: f32,
    /// Global maximum, unused here.
    pub glmax: i32,
    /// Global minimum, unused here.
    pub glmin: i32,
    /// Description of the data.
    pub descrip: [u8; 80],
    /// Auxiliary file name, unused here.
    pub aux_file: [u8; 24],
    /// Transform code for the q-form. A value > 0 means the q-form is valid.
    pub qform_code: i16,
    /// Transform code for the s-form. A value > 0 means the s-form is valid.
    pub sform_code: i16,
    /// Quaternion parameter b.
    pub quatern_b: f32,
    /// Quaternion parameter c.
    pub quatern_c: f32,
    /// Quaternion parameter d.
    pub quatern_d: f32,
    /// Quaternion x offset.
    pub qoffset_x: f32,
    /// Quaternion y offset.
    pub qoffset_y: f32,
    /// Quaternion z offset.
    pub qoffset_z: f32,
    /// First row of the affine (s-form) transform, `[x1, x2, x3, tx]`.
    pub srow_x: [f32; 4],
    /// Second row of the affine (s-form) transform, `[y1, y2, y3, ty]`.
    pub srow_y: [f32; 4],
    /// Third row of the affine (s-form) transform, `[z1, z2, z3, tz]`.
    pub srow_z: [f32; 4],
    /// Intent name, unused here.
    pub intent_name: [u8; 16],
    /// The magic string, `n+1\0` for single-file NIfTI volumes.
    pub magic: [u8; 4],
}

impl Default for Nifti1Header {
    fn default() -> Nifti1Header {
        Nifti1Header {
            sizeof_hdr: NIFTI_HEADER_SIZE_IN_BYTES as i32,
            data_type: [0; 10],
            db_name: [0; 18],
            extents: 0,
            session_error: 0,
            regular: b'r',
            dim_info: 0,
            dim: [0; 8],
            intent_p1: 0.0,
            intent_p2: 0.0,
            intent_p3: 0.0,
            intent_code: 0,
            datatype: 0,
            bitpix: 0,
            slice_start: 0,
            pixdim: [0.0; 8],
            vox_offset: NIFTI_DEFAULT_VOX_OFFSET,
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
            descrip: [0; 80],
            aux_file: [0; 24],
            qform_code: XFORM_UNKNOWN,
            sform_code: XFORM_UNKNOWN,
            quatern_b: 0.0,
            quatern_c: 0.0,
            quatern_d: 0.0,
            qoffset_x: 0.0,
            qoffset_y: 0.0,
            qoffset_z: 0.0,
            srow_x: [0.0; 4],
            srow_y: [0.0; 4],
            srow_z: [0.0; 4],
            intent_name: [0; 16],
            magic: MAGIC_SINGLE_FILE,
        }
    }
}

impl Nifti1Header {
    /// Read a [`Nifti1Header`] from a byte buffer containing exactly the first 348 bytes of a
    /// NIfTI file.
    ///
    /// The `little_endian` flag selects the byte order used to decode the multi-byte fields.
    pub fn from_bytes(buf: &[u8; NIFTI_HEADER_SIZE_IN_BYTES], little_endian: bool) -> Nifti1Header {
        if little_endian {
            parse_header::<LittleEndian>(buf)
        } else {
            parse_header::<BigEndian>(buf)
        }
    }

    /// Serialize this header into a byte buffer of exactly 348 bytes, using the given byte order.
    pub fn to_bytes(&self, little_endian: bool) -> [u8; NIFTI_HEADER_SIZE_IN_BYTES] {
        if little_endian {
            serialize_header::<LittleEndian>(self)
        } else {
            serialize_header::<BigEndian>(self)
        }
    }

    /// Check whether the magic string identifies a single-file NIfTI-1 volume (`n+1\0`).
    pub fn is_single_file_magic(&self) -> bool {
        self.magic == MAGIC_SINGLE_FILE
    }

    /// Check whether the magic string identifies a two-file NIfTI-1 volume (`ni1\0`).
    pub fn is_two_file_magic(&self) -> bool {
        self.magic == MAGIC_TWO_FILE
    }

    /// Set the `descrip` field from a string (truncated to 80 bytes, NUL-padded).
    pub fn set_descrip(&mut self, descrip: &str) {
        let bytes = descrip.as_bytes();
        self.descrip = [0; 80];
        let n = bytes.len().min(80);
        self.descrip[..n].copy_from_slice(&bytes[..n]);
    }

    /// Get the `descrip` field as a string, with trailing NUL bytes trimmed.
    pub fn descrip_string(&self) -> String {
        String::from_utf8_lossy(trim_nul_bytes(&self.descrip)).into_owned()
    }
}

impl fmt::Display for Nifti1Header {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "NIfTI-1 header with dim {}, {}, {}, {} and data type {}.",
            self.dim[1], self.dim[2], self.dim[3], self.dim[4], self.datatype
        )
    }
}

/// Trim all trailing NUL bytes from a byte slice.
fn trim_nul_bytes(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    &bytes[..end]
}

/// Parse the fixed-layout NIfTI-1 header from a byte buffer, using the byte order `E`.
fn parse_header<E: ByteOrder>(buf: &[u8]) -> Nifti1Header {
    let mut h = Nifti1Header::default();

    h.sizeof_hdr = E::read_i32(&buf[0..4]);
    h.data_type.copy_from_slice(&buf[4..14]);
    h.db_name.copy_from_slice(&buf[14..32]);
    h.extents = E::read_i32(&buf[32..36]);
    h.session_error = E::read_i16(&buf[36..38]);
    h.regular = buf[38];
    h.dim_info = buf[39];
    for (i, item) in h.dim.iter_mut().enumerate() {
        *item = E::read_i16(&buf[40 + i * 2..42 + i * 2]);
    }
    h.intent_p1 = E::read_f32(&buf[56..60]);
    h.intent_p2 = E::read_f32(&buf[60..64]);
    h.intent_p3 = E::read_f32(&buf[64..68]);
    h.intent_code = E::read_i16(&buf[68..70]);
    h.datatype = E::read_i16(&buf[70..72]);
    h.bitpix = E::read_i16(&buf[72..74]);
    h.slice_start = E::read_i16(&buf[74..76]);
    for (i, item) in h.pixdim.iter_mut().enumerate() {
        *item = E::read_f32(&buf[76 + i * 4..80 + i * 4]);
    }
    h.vox_offset = E::read_f32(&buf[108..112]);
    h.scl_slope = E::read_f32(&buf[112..116]);
    h.scl_inter = E::read_f32(&buf[116..120]);
    h.slice_end = E::read_i16(&buf[120..122]);
    h.slice_code = buf[122];
    h.xyzt_units = buf[123];
    h.cal_max = E::read_f32(&buf[124..128]);
    h.cal_min = E::read_f32(&buf[128..132]);
    h.slice_duration = E::read_f32(&buf[132..136]);
    h.toffset = E::read_f32(&buf[136..140]);
    h.glmax = E::read_i32(&buf[140..144]);
    h.glmin = E::read_i32(&buf[144..148]);
    h.descrip.copy_from_slice(&buf[148..228]);
    h.aux_file.copy_from_slice(&buf[228..252]);
    h.qform_code = E::read_i16(&buf[252..254]);
    h.sform_code = E::read_i16(&buf[254..256]);
    h.quatern_b = E::read_f32(&buf[256..260]);
    h.quatern_c = E::read_f32(&buf[260..264]);
    h.quatern_d = E::read_f32(&buf[264..268]);
    h.qoffset_x = E::read_f32(&buf[268..272]);
    h.qoffset_y = E::read_f32(&buf[272..276]);
    h.qoffset_z = E::read_f32(&buf[276..280]);
    for (i, item) in h.srow_x.iter_mut().enumerate() {
        *item = E::read_f32(&buf[280 + i * 4..284 + i * 4]);
    }
    for (i, item) in h.srow_y.iter_mut().enumerate() {
        *item = E::read_f32(&buf[296 + i * 4..300 + i * 4]);
    }
    for (i, item) in h.srow_z.iter_mut().enumerate() {
        *item = E::read_f32(&buf[312 + i * 4..316 + i * 4]);
    }
    h.intent_name.copy_from_slice(&buf[328..344]);
    h.magic.copy_from_slice(&buf[344..348]);

    h
}

/// Serialize a [`Nifti1Header`] into the fixed-layout 348-byte representation, using byte order `E`.
fn serialize_header<E: ByteOrder>(h: &Nifti1Header) -> [u8; NIFTI_HEADER_SIZE_IN_BYTES] {
    let mut buf = [0u8; NIFTI_HEADER_SIZE_IN_BYTES];

    E::write_i32(&mut buf[0..4], h.sizeof_hdr);
    buf[4..14].copy_from_slice(&h.data_type);
    buf[14..32].copy_from_slice(&h.db_name);
    E::write_i32(&mut buf[32..36], h.extents);
    E::write_i16(&mut buf[36..38], h.session_error);
    buf[38] = h.regular;
    buf[39] = h.dim_info;
    for (i, item) in h.dim.iter().enumerate() {
        E::write_i16(&mut buf[40 + i * 2..42 + i * 2], *item);
    }
    E::write_f32(&mut buf[56..60], h.intent_p1);
    E::write_f32(&mut buf[60..64], h.intent_p2);
    E::write_f32(&mut buf[64..68], h.intent_p3);
    E::write_i16(&mut buf[68..70], h.intent_code);
    E::write_i16(&mut buf[70..72], h.datatype);
    E::write_i16(&mut buf[72..74], h.bitpix);
    E::write_i16(&mut buf[74..76], h.slice_start);
    for (i, item) in h.pixdim.iter().enumerate() {
        E::write_f32(&mut buf[76 + i * 4..80 + i * 4], *item);
    }
    E::write_f32(&mut buf[108..112], h.vox_offset);
    E::write_f32(&mut buf[112..116], h.scl_slope);
    E::write_f32(&mut buf[116..120], h.scl_inter);
    E::write_i16(&mut buf[120..122], h.slice_end);
    buf[122] = h.slice_code;
    buf[123] = h.xyzt_units;
    E::write_f32(&mut buf[124..128], h.cal_max);
    E::write_f32(&mut buf[128..132], h.cal_min);
    E::write_f32(&mut buf[132..136], h.slice_duration);
    E::write_f32(&mut buf[136..140], h.toffset);
    E::write_i32(&mut buf[140..144], h.glmax);
    E::write_i32(&mut buf[144..148], h.glmin);
    buf[148..228].copy_from_slice(&h.descrip);
    buf[228..252].copy_from_slice(&h.aux_file);
    E::write_i16(&mut buf[252..254], h.qform_code);
    E::write_i16(&mut buf[254..256], h.sform_code);
    E::write_f32(&mut buf[256..260], h.quatern_b);
    E::write_f32(&mut buf[260..264], h.quatern_c);
    E::write_f32(&mut buf[264..268], h.quatern_d);
    E::write_f32(&mut buf[268..272], h.qoffset_x);
    E::write_f32(&mut buf[272..276], h.qoffset_y);
    E::write_f32(&mut buf[276..280], h.qoffset_z);
    for (i, item) in h.srow_x.iter().enumerate() {
        E::write_f32(&mut buf[280 + i * 4..284 + i * 4], *item);
    }
    for (i, item) in h.srow_y.iter().enumerate() {
        E::write_f32(&mut buf[296 + i * 4..300 + i * 4], *item);
    }
    for (i, item) in h.srow_z.iter().enumerate() {
        E::write_f32(&mut buf[312 + i * 4..316 + i * 4], *item);
    }
    buf[328..344].copy_from_slice(&h.intent_name);
    buf[344..348].copy_from_slice(&h.magic);

    buf
}

/// Detect the byte order of a NIfTI file from the `sizeof_hdr` field in its first 4 bytes.
///
/// Returns `true` for little-endian, `false` for big-endian. Returns an error if `sizeof_hdr` is
/// not 348 in either byte order.
fn detect_little_endian(buf: &[u8; NIFTI_HEADER_SIZE_IN_BYTES]) -> Result<bool> {
    let le = LittleEndian::read_i32(&buf[0..4]);
    let be = BigEndian::read_i32(&buf[0..4]);
    if le == NIFTI_HEADER_SIZE_IN_BYTES as i32 {
        Ok(true)
    } else if be == NIFTI_HEADER_SIZE_IN_BYTES as i32 {
        Ok(false)
    } else {
        Err(NeuroformatsError::InvalidNiftiFormat(format!(
            "sizeof_hdr is {} (expected {}).",
            le, NIFTI_HEADER_SIZE_IN_BYTES
        )))
    }
}

/// Map a NIfTI-1 data type code to an MGH `MRI_*` data type constant.
fn nifti_dtype_to_mri(nifti_dtype: i16) -> Result<i32> {
    match nifti_dtype {
        DT_UINT8 => Ok(MRI_UCHAR),
        DT_INT16 => Ok(MRI_SHORT),
        DT_INT32 => Ok(MRI_INT),
        DT_FLOAT32 => Ok(MRI_FLOAT),
        other => Err(NeuroformatsError::UnsupportedNiftiDataType(other)),
    }
}

/// Map an MGH `MRI_*` data type constant to a NIfTI-1 data type code and the number of bits per voxel.
fn mri_dtype_to_nifti(mri_type: i32) -> Result<(i16, i16)> {
    match mri_type {
        MRI_UCHAR => Ok((DT_UINT8, 8)),
        MRI_SHORT => Ok((DT_INT16, 16)),
        MRI_INT => Ok((DT_INT32, 32)),
        MRI_FLOAT => Ok((DT_FLOAT32, 32)),
        other => Err(NeuroformatsError::UnsupportedNiftiDataType(other as i16)),
    }
}

/// Get the number of bytes per voxel for an MGH `MRI_*` data type.
fn bytes_per_value(mri_type: i32) -> Result<usize> {
    match mri_type {
        MRI_UCHAR => Ok(1),
        MRI_SHORT => Ok(2),
        MRI_INT => Ok(4),
        MRI_FLOAT => Ok(4),
        other => Err(NeuroformatsError::UnsupportedNiftiDataType(other as i16)),
    }
}

/// Check whether a file name refers to a gzip-compressed NIfTI file (`.nii.gz`).
fn is_nii_gz_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    path.as_ref()
        .file_name()
        .map(|n| n.to_string_lossy().ends_with(".nii.gz"))
        .unwrap_or(false)
}

/// Models a volume stored in a single-file NIfTI-1 file.
///
/// The volume data itself is held in an [`FsMgh`] instance (with an [`FsMghHeader`] and an
/// [`FsMghData`]), which makes converting between NIfTI-1 and the FreeSurfer MGH/MGZ formats
/// straightforward, see the [module documentation](self).
#[derive(Debug, Clone, PartialEq)]
pub struct Nifti1 {
    /// The NIfTI-1 header of the file.
    pub header: Nifti1Header,
    /// The volume data, in the MGH data model (header and 4D data).
    pub volume: FsMgh,
}

impl Nifti1 {
    /// Read a NIfTI-1 file (`.nii` or `.nii.gz`, determined from the file extension).
    pub fn from_file<P: AsRef<Path> + Copy>(path: P) -> Result<Nifti1> {
        let gz = is_nii_gz_file(&path);
        let file = File::open(path)?;
        let buf_reader = BufReader::new(file);
        if gz {
            let mut input = BufReader::new(GzDecoder::new(buf_reader));
            Nifti1::from_reader(&mut input)
        } else {
            let mut input = buf_reader;
            Nifti1::from_reader(&mut input)
        }
    }

    /// Read a NIfTI-1 file from a byte stream. The byte order is detected from the header.
    ///
    /// It is assumed that the input is currently at the start of the file.
    pub fn from_reader<S>(input: &mut S) -> Result<Nifti1>
    where
        S: BufRead,
    {
        let mut hdr_bytes = [0u8; NIFTI_HEADER_SIZE_IN_BYTES];
        input
            .read_exact(&mut hdr_bytes)
            .map_err(|e| NeuroformatsError::Io(e))?;

        let little_endian = detect_little_endian(&hdr_bytes)?;
        let header = Nifti1Header::from_bytes(&hdr_bytes, little_endian);

        // Validate the magic string.
        if header.is_two_file_magic() {
            return Err(NeuroformatsError::InvalidNiftiFormat(
                "two-file NIfTI volumes (.hdr/.img pairs) are not supported, only single-file .nii volumes.".to_string(),
            ));
        }
        if !header.is_single_file_magic() {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "invalid magic string {:?}. Only single-file .nii volumes are supported.",
                &header.magic
            )));
        }

        // Validate dimensions.
        let ndim = header.dim[0];
        if !(1..=7).contains(&ndim) {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "dim[0] = {} is not in the valid range 1..=7.",
                ndim
            )));
        }
        if header.dim[1] <= 0 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "dim[1] = {}. Note that FreeSurfer surface data stored in NIfTI files (the FreeSurfer hack) is not supported.",
                header.dim[1]
            )));
        }
        // We can only represent up to 4 dimensions in the MGH volume model.
        for extra_dim in &header.dim[5..8] {
            if *extra_dim > 1 {
                return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                    "more than 4 dimensions are not supported (dim = {:?}).",
                    header.dim
                )));
            }
        }

        // Data dimensions. The dims 2..4 that are 0 are treated as 1, as some writers use 0 for unused dims.
        let dims: [usize; 4] = [
            header.dim[1] as usize,
            max_1(header.dim[2]) as usize,
            max_1(header.dim[3]) as usize,
            max_1(header.dim[4]) as usize,
        ];

        let mri_type = nifti_dtype_to_mri(header.datatype)?;
        let bytes_per_element = bytes_per_value(mri_type)?;

        // Compute the total number of voxels and check the allocation limit.
        let dims_i32: [i32; 4] = [
            dims[0] as i32,
            dims[1] as i32,
            dims[2] as i32,
            dims[3] as i32,
        ];
        let num_voxels = checked_mul_dims(&dims_i32)?;
        let num_bytes = num_voxels
            .checked_mul(bytes_per_element)
            .ok_or(NeuroformatsError::IntegerOverflow)?;
        if num_bytes > config::max_bytes_per_file() {
            return Err(NeuroformatsError::AllocationTooLarge);
        }

        // Validate the voxel data offset.
        if !header.vox_offset.is_finite() || header.vox_offset < NIFTI_HEADER_SIZE_IN_BYTES as f32 {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "vox_offset = {} is invalid (must be finite and >= {}).",
                header.vox_offset, NIFTI_HEADER_SIZE_IN_BYTES
            )));
        }
        if header.vox_offset > (usize::MAX as f32 / 2.0) {
            return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                "vox_offset = {} is unreasonably large.",
                header.vox_offset
            )));
        }
        let vox_offset = header.vox_offset as usize;
        let to_skip = vox_offset - NIFTI_HEADER_SIZE_IN_BYTES;

        // Skip any data between the header and the voxel data (extensions etc.).
        discard_bytes(input, to_skip)?;

        let scaling = VolumeScaling::from_header(header.scl_slope, header.scl_inter)?;

        // Read the raw voxel data.
        let mut data_bytes = vec![0u8; num_bytes];
        input
            .read_exact(&mut data_bytes)
            .map_err(|e| NeuroformatsError::Io(e))?;

        let data = decode_volume(&data_bytes, mri_type, little_endian, &scaling, &dims)?;

        let mgh_header = mgh_header_from_nifti(&header, &dims)?;
        let volume = FsMgh {
            header: mgh_header,
            data,
        };

        Ok(Nifti1 { header, volume })
    }

    /// Build a [`Nifti1`] from an existing [`FsMgh`] volume, deriving a matching NIfTI-1 header
    /// (dims, data type, and, if the volume carries RAS information, the s-form and q-form).
    pub fn from_mgh(mgh: FsMgh) -> Result<Nifti1> {
        let mh = &mgh.header;

        // NIfTI-1 stores dimensions as int16, so they must fit.
        for dim in [mh.dim1len, mh.dim2len, mh.dim3len, mh.dim4len].iter() {
            if *dim <= 0 || *dim > i16::MAX as i32 {
                return Err(NeuroformatsError::InvalidNiftiFormat(format!(
                    "MGH dimension {} exceeds the NIfTI-1 int16 range (1..=32767). Cannot write as NIfTI.",
                    dim
                )));
            }
        }

        let mut header = Nifti1Header::default();
        header.sizeof_hdr = NIFTI_HEADER_SIZE_IN_BYTES as i32;
        header.dim[0] = if mh.dim4len > 1 { 4 } else { 3 };
        header.dim[1] = mh.dim1len as i16;
        header.dim[2] = mh.dim2len as i16;
        header.dim[3] = mh.dim3len as i16;
        header.dim[4] = mh.dim4len as i16;
        header.dim[5] = 1;
        header.dim[6] = 1;
        header.dim[7] = 1;

        let (datatype, bitpix) = mri_dtype_to_nifti(mh.dtype)?;
        header.datatype = datatype;
        header.bitpix = bitpix;

        header.vox_offset = NIFTI_DEFAULT_VOX_OFFSET;
        header.scl_slope = 1.0;
        header.scl_inter = 0.0;
        header.pixdim[0] = 1.0;
        header.pixdim[1] = 1.0;
        header.pixdim[2] = 1.0;
        header.pixdim[3] = 1.0;
        header.pixdim[4] = 1.0;
        header.pixdim[5] = 1.0;
        header.pixdim[6] = 1.0;
        header.pixdim[7] = 1.0;

        if mh.is_ras_good == 1 {
            // Voxel sizes, from the MGH header.
            let sizes = mh.delta;
            if sizes.iter().all(|s| s.is_finite() && *s > 0.0) {
                header.pixdim[1] = sizes[0];
                header.pixdim[2] = sizes[1];
                header.pixdim[3] = sizes[2];
            }

            // Compute the voxel-to-RAS affine (vox2ras) from the MGH header.
            if let Some(affine) = mgh_affine_from_header(mh) {
                // s-form: the vox2ras affine itself.
                header.sform_code = XFORM_SCANNER_ANAT;
                header.srow_x = [
                    affine[0][0], affine[0][1], affine[0][2], affine[0][3],
                ];
                header.srow_y = [
                    affine[1][0], affine[1][1], affine[1][2], affine[1][3],
                ];
                header.srow_z = [
                    affine[2][0], affine[2][1], affine[2][2], affine[2][3],
                ];

                // q-form: encode the same affine as a quaternion.
                if let Some((qfac, b, c, d, qoffset)) = affine_to_qform(&affine) {
                    header.qform_code = XFORM_SCANNER_ANAT;
                    header.pixdim[0] = qfac;
                    header.quatern_b = b;
                    header.quatern_c = c;
                    header.quatern_d = d;
                    header.qoffset_x = qoffset[0];
                    header.qoffset_y = qoffset[1];
                    header.qoffset_z = qoffset[2];
                } else {
                    header.qform_code = XFORM_UNKNOWN;
                }
            } else {
                header.sform_code = XFORM_UNKNOWN;
                header.qform_code = XFORM_UNKNOWN;
            }
        }

        Ok(Nifti1 { header, volume: mgh })
    }

    /// Get the volume as an [`FsMgh`] instance. This is the volume read from a NIfTI file, or the
    /// volume that was wrapped via [`Nifti1::from_mgh`].
    pub fn to_mgh(&self) -> FsMgh {
        self.volume.clone()
    }

    /// Get the dimensions of the volume data.
    pub fn dim(&self) -> [usize; 4] {
        self.volume.dim()
    }
}

/// Convert a [`Nifti1`] volume into an [`FsMgh`] volume (see [`Nifti1::to_mgh`]).
impl From<Nifti1> for FsMgh {
    fn from(nifti: Nifti1) -> FsMgh {
        nifti.volume
    }
}

/// Discard exactly `num_bytes` bytes from the input reader.
fn discard_bytes<S: BufRead>(input: &mut S, num_bytes: usize) -> Result<()> {
    let mut remaining = num_bytes;
    let mut tmp = [0u8; 4096];
    while remaining > 0 {
        let n = remaining.min(tmp.len());
        input
            .read_exact(&mut tmp[..n])
            .map_err(|e| NeuroformatsError::Io(e))?;
        remaining -= n;
    }
    Ok(())
}

/// A helper returning `1` for values `<= 0` and the value itself otherwise.
fn max_1(v: i16) -> i16 {
    if v > 0 {
        v
    } else {
        1
    }
}

/// Decode raw NIfTI voxel data bytes into an [`FsMghData`] volume.
///
/// The bytes are interpreted in NIfTI order (the first volume dimension varies fastest, same as in
/// MGH files), with the given byte order, and each raw value is scaled by `scaling`.
fn decode_volume(
    data_bytes: &[u8],
    mri_type: i32,
    little_endian: bool,
    scaling: &VolumeScaling,
    dims: &[usize; 4],
) -> Result<FsMghData> {
    let vol_dim = Dim([dims[0], dims[1], dims[2], dims[3]]);
    let nvox = dims[0] * dims[1] * dims[2] * dims[3];

    let mut data = FsMghData {
        mri_uchar: None,
        mri_int: None,
        mri_float: None,
        mri_short: None,
    };

    match mri_type {
        MRI_UCHAR => {
            let values: Vec<u8> = if scaling.is_identity() {
                data_bytes.to_vec()
            } else {
                data_bytes
                    .iter()
                    .map(|&v| {
                        let scaled = (v as f32) * scaling.slope + scaling.inter;
                        scaled.round().clamp(0.0, 255.0) as u8
                    })
                    .collect()
            };
            data.mri_uchar = Some(
                Array::from_shape_vec(vol_dim, values)
                    .map_err(|e| NeuroformatsError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?,
            );
        }
        MRI_SHORT => {
            let raw = decode_ints::<i16>(data_bytes, 2, nvox, little_endian)?;
            let values: Vec<i16> = if scaling.is_identity() {
                raw
            } else {
                raw.iter()
                    .map(|&v| ((v as f32) * scaling.slope + scaling.inter).round() as i16)
                    .collect()
            };
            data.mri_short = Some(
                Array::from_shape_vec(vol_dim, values)
                    .map_err(|e| NeuroformatsError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?,
            );
        }
        MRI_INT => {
            let raw = decode_ints::<i32>(data_bytes, 4, nvox, little_endian)?;
            let values: Vec<i32> = if scaling.is_identity() {
                raw
            } else {
                raw.iter()
                    .map(|&v| ((v as f32) * scaling.slope + scaling.inter).round() as i32)
                    .collect()
            };
            data.mri_int = Some(
                Array::from_shape_vec(vol_dim, values)
                    .map_err(|e| NeuroformatsError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?,
            );
        }
        MRI_FLOAT => {
            let raw = decode_floats(data_bytes, nvox, little_endian)?;
            let values: Vec<f32> = if scaling.is_identity() {
                raw
            } else {
                raw.iter().map(|&v| v * scaling.slope + scaling.inter).collect()
            };
            data.mri_float = Some(
                Array::from_shape_vec(vol_dim, values)
                    .map_err(|e| NeuroformatsError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?,
            );
        }
        other => {
            return Err(NeuroformatsError::UnsupportedNiftiDataType(other as i16));
        }
    }

    Ok(data)
}

/// Decode `num_items` fixed-size integers of the given byte width from `bytes`, with the given byte order.
fn decode_ints<T>(bytes: &[u8], width: usize, num_items: usize, little_endian: bool) -> Result<Vec<T>>
where
    T: IntFromBytes,
{
    let mut out = Vec::with_capacity(num_items);
    for chunk in bytes.chunks_exact(width) {
        out.push(T::from_bytes(chunk, little_endian));
    }
    if out.len() != num_items {
        return Err(NeuroformatsError::InvalidNiftiFormat(
            "voxel data section has an unexpected length.".to_string(),
        ));
    }
    Ok(out)
}

/// Helper trait to decode an integer type from a fixed-size byte chunk in a given byte order.
trait IntFromBytes {
    fn from_bytes(bytes: &[u8], little_endian: bool) -> Self;
}

impl IntFromBytes for i16 {
    fn from_bytes(bytes: &[u8], little_endian: bool) -> Self {
        if little_endian {
            LittleEndian::read_i16(bytes)
        } else {
            BigEndian::read_i16(bytes)
        }
    }
}

impl IntFromBytes for i32 {
    fn from_bytes(bytes: &[u8], little_endian: bool) -> Self {
        if little_endian {
            LittleEndian::read_i32(bytes)
        } else {
            BigEndian::read_i32(bytes)
        }
    }
}

/// Decode `num_items` f32 values from `bytes`, with the given byte order.
fn decode_floats(bytes: &[u8], num_items: usize, little_endian: bool) -> Result<Vec<f32>> {
    let mut out = Vec::with_capacity(num_items);
    for chunk in bytes.chunks_exact(4) {
        if little_endian {
            out.push(LittleEndian::read_f32(chunk));
        } else {
            out.push(BigEndian::read_f32(chunk));
        }
    }
    if out.len() != num_items {
        return Err(NeuroformatsError::InvalidNiftiFormat(
            "voxel data section has an unexpected length.".to_string(),
        ));
    }
    Ok(out)
}

/// Compute the 4x4 voxel-to-RAS affine (vox2ras) from an [`FsMghHeader`], if it carries RAS info.
///
/// The linear part maps a unit step along voxel axis j to a world displacement of
/// `delta[j] * mdc_row_j` (i.e. column j is scaled by the voxel size of axis j, with the rows of
/// the MGH `mdc` matrix being the unit direction cosines of the 3 axes). The translation is the RAS
/// coordinate of voxel (0, 0, 0). This is the same convention FreeSurfer's `mri_convert` uses.
fn mgh_affine_from_header(mh: &FsMghHeader) -> Option<[[f32; 4]; 4]> {
    if mh.is_ras_good != 1 {
        return None;
    }
    validate_finite_f32_slice(&mh.delta, "delta").ok()?;
    validate_finite_f32_slice(&mh.mdc_raw, "mdc_raw").ok()?;
    validate_finite_f32_slice(&mh.p_xyz_c, "p_xyz_c").ok()?;

    let delta = mh.delta;
    let mdc = mh.mdc_raw;

    // Linear part: affine[i][j] = delta[j] * mdc[j*3 + i].
    let mut affine = [[0.0f32; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            affine[i][j] = delta[j] * mdc[j * 3 + i];
        }
    }

    // RAS of the center voxel (integer division, matching FsMghHeader::vox2ras).
    let c_crs = [
        (mh.dim1len / 2) as f32,
        (mh.dim2len / 2) as f32,
        (mh.dim3len / 2) as f32,
    ];
    for i in 0..3 {
        let mut center_world = 0.0;
        for j in 0..3 {
            center_world += affine[i][j] * c_crs[j];
        }
        // RAS of voxel (0,0,0) = RAS of center voxel - linear part * center voxel index.
        affine[i][3] = mh.p_xyz_c[i] - center_world;
    }
    affine[3][3] = 1.0;

    Some(affine)
}

/// Decompose a voxel-to-RAS affine into NIfTI q-form parameters.
///
/// Returns `(qfac, quatern_b, quatern_c, quatern_d, qoffset)` where `qoffset` is the RAS
/// coordinate of voxel (0, 0, 0). The `qfac` factor encodes the handedness of the coordinate
/// system and is stored in `pixdim[0]`. Returns `None` if the affine cannot be represented as a
/// q-form (e.g. it contains a zero voxel size).
fn affine_to_qform(affine: &[[f32; 4]; 4]) -> Option<(f32, f32, f32, f32, [f32; 3])> {
    // Extract the rotation part and the voxel sizes (column norms of the linear part).
    let mut sizes = [0.0f32; 3];
    let mut rot = [[0.0f32; 3]; 3];
    for j in 0..3 {
        let mut norm = 0.0;
        for i in 0..3 {
            norm += affine[i][j] * affine[i][j];
        }
        sizes[j] = norm.sqrt();
        if !sizes[j].is_finite() || sizes[j] < f32::EPSILON {
            return None;
        }
        for i in 0..3 {
            rot[i][j] = affine[i][j] / sizes[j];
        }
    }

    // qfac encodes the sign of the determinant of the rotation part.
    let det = determinant3x3(&rot);
    let qfac = if det < 0.0 { -1.0 } else { 1.0 };

    // Make the rotation proper (det = +1) by flipping the 3rd column; qfac encodes the flip.
    for i in 0..3 {
        rot[i][2] *= qfac;
    }

    let (_a, b, c, d) = rotation_matrix_to_quaternion(&rot);
    let qoffset = [affine[0][3], affine[1][3], affine[2][3]];
    Some((qfac, b, c, d, qoffset))
}

/// Compute the determinant of a 3x3 matrix given in column-major order (columns `rot[0..3][axis]`).
fn determinant3x3(m: &[[f32; 3]; 3]) -> f32 {
    m[0][0] * (m[1][1] * m[2][2] - m[2][1] * m[1][2])
        - m[1][0] * (m[0][1] * m[2][2] - m[2][1] * m[0][2])
        + m[2][0] * (m[0][1] * m[1][2] - m[1][1] * m[0][2])
}

/// Convert a 3x3 rotation matrix (proper rotation, det = +1) to a quaternion `(a, b, c, d)`, where
/// `a` is the scalar part and `b, c, d` the vector parts.
///
/// Uses Shepperd's method. The quaternion sign is arbitrary (`q` and `-q` represent the same
/// rotation), which is fine for the NIfTI q-form.
fn rotation_matrix_to_quaternion(r: &[[f32; 3]; 3]) -> (f32, f32, f32, f32) {
    let trace = r[0][0] + r[1][1] + r[2][2];
    let (a, b, c, d);
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        a = 0.25 * s;
        b = (r[2][1] - r[1][2]) / s;
        c = (r[0][2] - r[2][0]) / s;
        d = (r[1][0] - r[0][1]) / s;
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
        a = (r[2][1] - r[1][2]) / s;
        b = 0.25 * s;
        c = (r[0][1] + r[1][0]) / s;
        d = (r[0][2] + r[2][0]) / s;
    } else if r[1][1] > r[2][2] {
        let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
        a = (r[0][2] - r[2][0]) / s;
        b = (r[0][1] + r[1][0]) / s;
        c = 0.25 * s;
        d = (r[1][2] + r[2][1]) / s;
    } else {
        let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
        a = (r[1][0] - r[0][1]) / s;
        b = (r[0][2] + r[2][0]) / s;
        c = (r[1][2] + r[2][1]) / s;
        d = 0.25 * s;
    }
    (a, b, c, d)
}

/// Build the rotation matrix from a quaternion `(a, b, c, d)`.
fn quaternion_to_rotation(a: f32, b: f32, c: f32, d: f32) -> [[f32; 3]; 3] {
    [
        [
            a * a + b * b - c * c - d * d,
            2.0 * (b * c - a * d),
            2.0 * (b * d + a * c),
        ],
        [
            2.0 * (b * c + a * d),
            a * a + c * c - b * b - d * d,
            2.0 * (c * d - a * b),
        ],
        [
            2.0 * (b * d - a * c),
            2.0 * (c * d + a * b),
            a * a + d * d - b * b - c * c,
        ],
    ]
}

/// Decode the voxel-to-world transform from a NIfTI header into the MGH `delta`, `mdc_raw` and
/// `p_xyz_c` fields. The s-form is preferred, the q-form is used as a fallback.
///
/// Returns `None` if the header carries no usable transform (both xform codes are 0).
fn decode_transform(
    header: &Nifti1Header,
    dims: &[usize; 4],
) -> Result<Option<([f32; 3], [f32; 9], [f32; 3])>> {
    // Get the linear part and translation of the affine (voxel-to-world).
    let (linear, translation): ([[f32; 3]; 3], [f32; 3]) = if header.sform_code > 0 {
        validate_finite_f32_slice(&header.srow_x, "srow_x")?;
        validate_finite_f32_slice(&header.srow_y, "srow_y")?;
        validate_finite_f32_slice(&header.srow_z, "srow_z")?;
        let linear = [
            [header.srow_x[0], header.srow_x[1], header.srow_x[2]],
            [header.srow_y[0], header.srow_y[1], header.srow_y[2]],
            [header.srow_z[0], header.srow_z[1], header.srow_z[2]],
        ];
        let translation = [header.srow_x[3], header.srow_y[3], header.srow_z[3]];
        (linear, translation)
    } else if header.qform_code > 0 {
        validate_finite_f32_slice(
            &[
                header.quatern_b,
                header.quatern_c,
                header.quatern_d,
                header.qoffset_x,
                header.qoffset_y,
                header.qoffset_z,
            ],
            "qform quaternion/qoffset",
        )?;
        validate_finite_f32_slice(&header.pixdim[0..4], "pixdim")?;
        let a = (1.0 - (header.quatern_b * header.quatern_b
            + header.quatern_c * header.quatern_c
            + header.quatern_d * header.quatern_d))
            .max(0.0)
            .sqrt();
        let qfac = if header.pixdim[0] < 0.0 { -1.0 } else { 1.0 };
        let mut rot = quaternion_to_rotation(a, header.quatern_b, header.quatern_c, header.quatern_d);
        // Apply qfac (a possible reflection) to the 3rd column.
        for i in 0..3 {
            rot[i][2] *= qfac;
        }
        // Scale the columns by the voxel sizes.
        let mut linear = [[0.0f32; 3]; 3];
        for j in 0..3 {
            let size = header.pixdim[j + 1].max(0.0);
            for i in 0..3 {
                linear[i][j] = rot[i][j] * size;
            }
        }
        let translation = [header.qoffset_x, header.qoffset_y, header.qoffset_z];
        (linear, translation)
    } else {
        return Ok(None);
    };

    // Voxel sizes are the norms of the columns of the linear part.
    let mut delta = [0.0f32; 3];
    let mut direction = [[0.0f32; 3]; 3];
    for j in 0..3 {
        let mut norm = 0.0;
        for i in 0..3 {
            norm += linear[i][j] * linear[i][j];
        }
        delta[j] = norm.sqrt();
        if delta[j] > f32::EPSILON {
            for i in 0..3 {
                direction[i][j] = linear[i][j] / delta[j];
            }
        } else {
            // Degenerate column (zero voxel size): fall back to a unit vector along the world axis.
            delta[j] = 1.0;
            for i in 0..3 {
                direction[i][j] = if i == j { 1.0 } else { 0.0 };
            }
        }
    }

    // MGH stores the unit direction cosines of the 3 axes in the rows of `mdc_raw`: row j of the
    // MGH `mdc` matrix is the direction of voxel axis j, which is the j-th column of `linear`.
    let mut mdc_raw = [0.0f32; 9];
    for j in 0..3 {
        for i in 0..3 {
            mdc_raw[j * 3 + i] = direction[i][j];
        }
    }

    // The RAS of the center voxel is `translation + linear * center_voxel_index` (integer division).
    let c_crs = [
        (dims[0] / 2) as f32,
        (dims[1] / 2) as f32,
        (dims[2] / 2) as f32,
    ];
    let mut p_xyz_c = translation;
    for i in 0..3 {
        for j in 0..3 {
            p_xyz_c[i] += linear[i][j] * c_crs[j];
        }
    }

    Ok(Some((delta, mdc_raw, p_xyz_c)))
}

/// Build an [`FsMghHeader`] from a NIfTI header (used when reading a NIfTI file).
fn mgh_header_from_nifti(header: &Nifti1Header, dims: &[usize; 4]) -> Result<FsMghHeader> {
    let mut mh = FsMghHeader::default();
    mh.dim1len = dims[0] as i32;
    mh.dim2len = dims[1] as i32;
    mh.dim3len = dims[2] as i32;
    mh.dim4len = dims[3] as i32;
    mh.dtype = nifti_dtype_to_mri(header.datatype)?;
    mh.dof = 0;

    if let Some((delta, mdc_raw, p_xyz_c)) = decode_transform(header, dims)? {
        mh.is_ras_good = 1;
        mh.delta = delta;
        mh.mdc_raw = mdc_raw;
        mh.p_xyz_c = p_xyz_c;
    } else {
        mh.is_ras_good = 0;
    }

    Ok(mh)
}

/// Read a NIfTI-1 file (`.nii` or `.nii.gz`).
///
/// The file format is determined from the file extension: files ending with `.nii.gz` are read in
/// gzip-compressed form, files ending with `.nii` are read uncompressed.
///
/// The returned [`Nifti1`] holds the parsed NIfTI header (with all metadata of the original file)
/// and the volume data in the crate's [`FsMgh`] model. Use [`Nifti1::to_mgh`] to get the volume.
///
/// # Errors
///
/// * `InvalidNiftiFormat` if the file is not a valid single-file NIfTI-1 file (wrong magic string,
///   bad `sizeof_hdr`, invalid `vox_offset`, unsupported number of dimensions, etc.).
/// * `UnsupportedNiftiDataType` if the file uses a data type that the MGH volume model cannot hold.
/// * `AllocationTooLarge` / `IntegerOverflow` if the volume is too large to load safely (see the
///   global loading limits).
pub fn read_nifti<P: AsRef<Path> + Copy>(path: P) -> Result<Nifti1> {
    Nifti1::from_file(path)
}

/// Write a [`Nifti1`] volume to a file in NIfTI-1 format (single-file `.nii`, big-endian).
///
/// Whether the file is gzip-compressed is determined from the file extension: files ending with
/// `.nii.gz` are written gzip-compressed, all others uncompressed.
///
/// The voxel data of `nifti.volume` is written as-is (no scaling is applied, `scl_slope`/`scl_inter`
/// are set to 1 and 0), in NIfTI voxel order (the first volume dimension varies fastest, same as in
/// MGH files).
///
/// # Errors
///
/// * `InvalidNiftiFormat` if the header and the volume are inconsistent (e.g. a dimension exceeds
///   the NIfTI-1 int16 range).
/// * `UnsupportedNiftiDataType` if the volume data type cannot be represented as a NIfTI data type.
pub fn write_nifti<P: AsRef<Path> + Copy>(path: P, nifti: &Nifti1) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(&file);
    if is_nii_gz_file(path) {
        write_nifti_to(
            flate2::write::GzEncoder::new(writer, Compression::default()),
            nifti,
        )
    } else {
        write_nifti_to(writer, nifti)
    }
}

/// Write a [`Nifti1`] volume to a byte stream in NIfTI-1 format (big-endian).
fn write_nifti_to<W>(w: W, nifti: &Nifti1) -> Result<()>
where
    W: Write,
{
    let header = &nifti.header;
    let volume = &nifti.volume;
    let mh = &volume.header;

    // Validate that the volume data type is supported and matches the header.
    let mri_type = nifti_dtype_to_mri(header.datatype)?;
    if mri_type != mh.dtype {
        return Err(NeuroformatsError::InvalidNiftiFormat(format!(
            "inconsistent volume data type: header says {}, volume says {}.",
            header.datatype, mh.dtype
        )));
    }

    // Validate that header dims and volume dims match.
    let expected_dims = [
        max_1(header.dim[1]) as usize,
        max_1(header.dim[2]) as usize,
        max_1(header.dim[3]) as usize,
        max_1(header.dim[4]) as usize,
    ];
    let volume_dims = volume.dim();
    if expected_dims != volume_dims {
        return Err(NeuroformatsError::InvalidNiftiFormat(format!(
            "inconsistent dimensions: header says {:?}, volume has {:?}.",
            expected_dims, volume_dims
        )));
    }

    let mut out = BufWriter::new(w);

    // Serialize the header (big-endian by default) and the 4-byte extension indicator.
    let header_bytes = header.to_bytes(false);
    out.write_all(&header_bytes)?;
    out.write_all(&[0u8; 4])?; // no extensions

    // Write the voxel data, in NIfTI order (same as MGH order: first dim varies fastest).
    let mut data_writer = ByteOrdered::runtime(&mut out, Endianness::Big);
    if mri_type == MRI_UCHAR {
        for v in volume.data.mri_uchar.as_ref().unwrap().iter() {
            data_writer.write_u8(*v)?;
        }
    } else if mri_type == MRI_SHORT {
        for v in volume.data.mri_short.as_ref().unwrap().iter() {
            data_writer.write_i16(*v)?;
        }
    } else if mri_type == MRI_INT {
        for v in volume.data.mri_int.as_ref().unwrap().iter() {
            data_writer.write_i32(*v)?;
        }
    } else if mri_type == MRI_FLOAT {
        for v in volume.data.mri_float.as_ref().unwrap().iter() {
            data_writer.write_f32(*v)?;
        }
    } else {
        return Err(NeuroformatsError::UnsupportedNiftiDataType(header.datatype));
    }

    out.flush()?;
    Ok(())
}

impl fmt::Display for Nifti1 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "NIfTI-1 volume with dim {}, {}, {}, {} and data type {}.",
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
    use crate::fs_mgh::{read_mgh, write_mgh};
    use approx::assert_abs_diff_eq;
    use std::io::Cursor;
    use tempfile::tempdir;

    const NII_FILE: &str = "resources/subjects_dir/subject1/mri/brain.nii";
    const MGZ_FILE: &str = "resources/subjects_dir/subject1/mri/brain.mgz";

    /// Assert that two FsMgh volumes contain the same voxel data.
    fn assert_same_data(a: &FsMgh, b: &FsMgh) {
        assert_eq!(a.header.dtype, b.header.dtype);
        assert_eq!(a.dim(), b.dim());
        match a.header.dtype {
            MRI_UCHAR => {
                let (da, db) = (a.data.mri_uchar.as_ref().unwrap(), b.data.mri_uchar.as_ref().unwrap());
                assert_eq!(da, db);
            }
            MRI_SHORT => {
                let (da, db) = (a.data.mri_short.as_ref().unwrap(), b.data.mri_short.as_ref().unwrap());
                assert_eq!(da, db);
            }
            MRI_INT => {
                let (da, db) = (a.data.mri_int.as_ref().unwrap(), b.data.mri_int.as_ref().unwrap());
                assert_eq!(da, db);
            }
            MRI_FLOAT => {
                let (da, db) = (a.data.mri_float.as_ref().unwrap(), b.data.mri_float.as_ref().unwrap());
                assert_abs_diff_eq!(da, db, epsilon = 1e-5);
            }
            other => panic!("unexpected data type {}", other),
        }
    }

    /// Assert that two float slices are element-wise close within `eps`.
    fn assert_f32_slice_approx(a: &[f32], b: &[f32], eps: f32) {
        assert_eq!(a.len(), b.len(), "slices differ in length ({} vs {})", a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_abs_diff_eq!(*x, *y, epsilon = eps);
        }
    }

    /// Assert that the RAS info of two FsMgh volumes matches.
    fn assert_same_ras(a: &FsMgh, b: &FsMgh) {
        assert_eq!(a.header.is_ras_good, b.header.is_ras_good);
        if a.header.is_ras_good == 1 {
            for i in 0..3 {
                assert_abs_diff_eq!(a.header.delta[i], b.header.delta[i], epsilon = 1e-4);
            }
            for i in 0..9 {
                assert_abs_diff_eq!(a.header.mdc_raw[i], b.header.mdc_raw[i], epsilon = 1e-4);
            }
            for i in 0..3 {
                assert_abs_diff_eq!(a.header.p_xyz_c[i], b.header.p_xyz_c[i], epsilon = 1e-4);
            }
            let vox2ras_a = a.header.vox2ras().unwrap();
            let vox2ras_b = b.header.vox2ras().unwrap();
            assert_abs_diff_eq!(vox2ras_a, vox2ras_b, epsilon = 1e-3);
        }
    }

    #[test]
    fn one_can_read_the_demo_nifti_file() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let brain = nifti.to_mgh();

        assert_eq!(brain.dim(), [256, 256, 256, 1]);
        assert_eq!(brain.header.dtype, MRI_UCHAR);
        assert_eq!(brain.header.is_ras_good, 1);
        assert_eq!(nifti.header.datatype, DT_UINT8);
        assert_eq!(nifti.header.sform_code, XFORM_SCANNER_ANAT);
        assert_eq!(nifti.header.qform_code, XFORM_SCANNER_ANAT);

        // Same voxel values as in the demo MGH file.
        let data = brain.data.mri_uchar.unwrap();
        assert_eq!(data[[99, 99, 99, 0]], 77);
        assert_eq!(data[[109, 109, 109, 0]], 71);
        assert_eq!(data[[0, 0, 0, 0]], 0);
        assert_eq!(data.mapv(|a| a as i32).sum(), 121035479);
    }

    #[test]
    fn reading_nifti_and_mgh_gives_the_same_volume() {
        let from_nii = read_nifti(NII_FILE).unwrap().to_mgh();
        let from_mgz = read_mgh(MGZ_FILE).unwrap();
        assert_same_data(&from_nii, &from_mgz);
        assert_same_ras(&from_nii, &from_mgz);
    }

    #[test]
    fn the_freesurfer_brain_nifti_matches_the_brain_mgz() {
        // `brain.nii` was created from `brain.mgz` with FreeSurfer's `mri_convert`, so the two
        // files must describe the same volume: same voxel data and the same voxel-to-RAS geometry.
        // This cross-checks the NIfTI reader against an independent reference (real FreeSurfer
        // output), not just against a self-round-trip.
        let nifti = read_nifti(NII_FILE).unwrap();
        let from_nii = nifti.to_mgh();
        let from_mgz = read_mgh(MGZ_FILE).unwrap();

        // (1) Same dimensions, data type and RAS flag.
        assert_eq!(from_nii.dim(), from_mgz.dim());
        assert_eq!(from_nii.header.dtype, from_mgz.header.dtype);
        assert_eq!(from_nii.header.dtype, MRI_UCHAR);
        assert_eq!(from_nii.header.is_ras_good, 1);
        assert_eq!(from_mgz.header.is_ras_good, 1);

        // (2) Same voxel data.
        let nii_data = from_nii.data.mri_uchar.as_ref().unwrap();
        let mgz_data = from_mgz.data.mri_uchar.as_ref().unwrap();
        assert_eq!(nii_data, mgz_data);

        // (3) The RAS header fields derived from the NIfTI file equal the ones read from the MGH
        // file, and match the known reference values of this demo volume.
        let expected_delta = [1.0_f32, 1.0, 1.0];
        let expected_mdc_raw = [-1.0_f32, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];
        let expected_p_xyz_c = [-0.49995422_f32, 29.372742, -48.90473];
        assert_f32_slice_approx(&from_nii.header.delta, &expected_delta, 1e-4);
        assert_f32_slice_approx(&from_nii.header.mdc_raw, &expected_mdc_raw, 1e-4);
        assert_f32_slice_approx(&from_nii.header.p_xyz_c, &expected_p_xyz_c, 1e-4);
        assert_f32_slice_approx(&from_mgz.header.delta, &expected_delta, 1e-4);
        assert_f32_slice_approx(&from_mgz.header.mdc_raw, &expected_mdc_raw, 1e-4);
        assert_f32_slice_approx(&from_mgz.header.p_xyz_c, &expected_p_xyz_c, 1e-4);

        // (4) The vox2ras matrix reconstructed from the NIfTI file equals the one from the MGH
        // file, and both equal the affine that FreeSurfer stored directly in the NIfTI s-form
        // (the raw srow_x/y/z rows of `brain.nii`), which is the voxel-(0,0,0)-anchored affine.
        let vox2ras_nii = from_nii.header.vox2ras().unwrap();
        let vox2ras_mgz = from_mgz.header.vox2ras().unwrap();
        assert_abs_diff_eq!(vox2ras_nii, vox2ras_mgz, epsilon = 1e-2);

        let expected_vox2ras = [
            [-1.0_f32, 0.0, 0.0, 127.5],
            [0.0, 0.0, 1.0, -98.6273],
            [0.0, -1.0, 0.0, 79.0953],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let sform_rows = [nifti.header.srow_x, nifti.header.srow_y, nifti.header.srow_z];
        for i in 0..3 {
            for j in 0..4 {
                assert_abs_diff_eq!(vox2ras_nii[[i, j]], expected_vox2ras[i][j], epsilon = 1e-2);
                // Our reconstruction reproduces the affine stored in the file's s-form.
                assert_abs_diff_eq!(vox2ras_nii[[i, j]], sform_rows[i][j], epsilon = 1e-2);
            }
        }

        // (5) Anchor handling: the s-form translation stored in `brain.nii` is the RAS of voxel
        // (0,0,0) (P0 = (127.5, -98.6, 79.1)), which is *not* the MGH center voxel p_xyz_c
        // (about (-0.5, 29.4, -48.9)). Reading the file must re-anchor that translation into the
        // MGH center convention (see (3)).
        let sform_translation = [
            nifti.header.srow_x[3],
            nifti.header.srow_y[3],
            nifti.header.srow_z[3],
        ];
        assert_f32_slice_approx(&sform_translation, &[127.5_f32, -98.6273, 79.0953], 1e-2);
        // The two anchors really do differ (by L * (n/2), about half the volume extent).
        for i in 0..3 {
            assert!(
                (sform_translation[i] - from_nii.header.p_xyz_c[i]).abs() > 10.0,
                "expected the voxel-(0,0,0) RAS and the center voxel RAS to differ by more than 10 mm"
            );
        }
    }

    #[test]
    fn mgh_can_be_converted_to_nifti_and_back() {
        let brain = read_mgh(MGZ_FILE).unwrap();

        let dir = tempdir().unwrap();
        let nii_path = dir.path().join("brain.nii");

        let nifti = Nifti1::from_mgh(brain.clone()).unwrap();
        write_nifti(&nii_path, &nifti).unwrap();

        let brain2 = read_nifti(&nii_path).unwrap().to_mgh();
        assert_same_data(&brain, &brain2);
        assert_same_ras(&brain, &brain2);
    }

    #[test]
    fn nifti_can_be_converted_to_mgh_and_back() {
        let brain = read_nifti(NII_FILE).unwrap().to_mgh();

        let dir = tempdir().unwrap();
        let mgh_path = dir.path().join("brain.mgh");

        write_mgh(&mgh_path, &brain).unwrap();

        let brain2 = read_mgh(&mgh_path).unwrap();
        assert_same_data(&brain, &brain2);
        assert_same_ras(&brain, &brain2);
    }

    #[test]
    fn writing_the_mgz_as_nifti_matches_the_freesurfer_reference() {
        // FreeSurfer's mri_convert wrote resources/.../brain.nii from brain.mgz. Our writer should
        // produce the same affine (s-form rows and q-offset) for the same input volume.
        let brain = read_mgh(MGZ_FILE).unwrap();
        let nifti = Nifti1::from_mgh(brain).unwrap();
        let h = &nifti.header;

        assert_eq!(h.sform_code, XFORM_SCANNER_ANAT);
        assert_eq!(h.qform_code, XFORM_SCANNER_ANAT);
        // The s-form rows equal the vox2ras rows (matching mri_convert's brain.nii).
        let expected_sform = [
            [-1.0, 0.0, 0.0, 127.50005],
            [0.0, 0.0, 1.0, -98.62726],
            [0.0, -1.0, 0.0, 79.09527],
        ];
        for (i, row) in expected_sform.iter().enumerate() {
            let srow = match i {
                0 => h.srow_x,
                1 => h.srow_y,
                _ => h.srow_z,
            };
            for (j, v) in row.iter().enumerate() {
                assert_abs_diff_eq!(srow[j], *v, epsilon = 1e-3);
            }
        }
        // q-offset equals the s-form translation (voxel (0,0,0) RAS).
        assert_abs_diff_eq!(h.qoffset_x, 127.50005, epsilon = 1e-3);
        assert_abs_diff_eq!(h.qoffset_y, -98.62726, epsilon = 1e-3);
        assert_abs_diff_eq!(h.qoffset_z, 79.09527, epsilon = 1e-3);
        // qfac from pixdim[0].
        assert_abs_diff_eq!(h.pixdim[0], -1.0, epsilon = 1e-4);
    }

    #[test]
    fn a_small_float_volume_can_be_written_and_reread_without_ras() {
        // Build a synthetic float volume without RAS info.
        let mut mh = FsMghHeader::default();
        mh.dim1len = 2;
        mh.dim2len = 3;
        mh.dim3len = 4;
        mh.dim4len = 1;
        mh.dtype = MRI_FLOAT;
        mh.is_ras_good = 0;
        let values: Vec<f32> = (0..24).map(|i| i as f32 + 0.5).collect();
        let data = FsMghData {
            mri_float: Some(Array::from_shape_vec(Dim([2, 3, 4, 1]), values).unwrap()),
            mri_uchar: None,
            mri_int: None,
            mri_short: None,
        };
        let volume = FsMgh { header: mh, data };

        let nifti = Nifti1::from_mgh(volume.clone()).unwrap();
        assert_eq!(nifti.header.sform_code, XFORM_UNKNOWN);
        assert_eq!(nifti.header.qform_code, XFORM_UNKNOWN);

        let dir = tempdir().unwrap();
        let nii_path = dir.path().join("small.nii");
        write_nifti(&nii_path, &nifti).unwrap();

        let volume2 = read_nifti(&nii_path).unwrap().to_mgh();
        assert_same_data(&volume, &volume2);
        assert_eq!(volume2.header.is_ras_good, 0);
    }

    #[test]
    fn a_qform_only_file_can_be_read() {
        // Build a small NIfTI file that only has a q-form (no s-form) and read it back.
        let mut header = Nifti1Header::default();
        header.dim[0] = 3;
        header.dim[1] = 2;
        header.dim[2] = 3;
        header.dim[3] = 4;
        header.dim[4] = 1;
        header.datatype = DT_FLOAT32;
        header.bitpix = 32;
        header.pixdim[0] = 1.0;
        header.pixdim[1] = 1.0;
        header.pixdim[2] = 1.0;
        header.pixdim[3] = 1.0;
        header.vox_offset = NIFTI_DEFAULT_VOX_OFFSET;
        header.qform_code = XFORM_SCANNER_ANAT;
        header.sform_code = XFORM_UNKNOWN;
        // 90 degree rotation about the z axis: (a, b, c, d) = (sqrt(0.5), 0, 0, sqrt(0.5)).
        header.quatern_b = 0.0;
        header.quatern_c = 0.0;
        header.quatern_d = (0.5_f32).sqrt();
        header.qoffset_x = 10.0;
        header.qoffset_y = 20.0;
        header.qoffset_z = 30.0;

        let values: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let mh = mgh_header_from_nifti(&header, &[2, 3, 4, 1]).unwrap();
        let volume = FsMgh {
            header: mh,
            data: FsMghData {
                mri_float: Some(Array::from_shape_vec(Dim([2, 3, 4, 1]), values).unwrap()),
                mri_uchar: None,
                mri_int: None,
                mri_short: None,
            },
        };
        let nifti = Nifti1 { header, volume };

        let dir = tempdir().unwrap();
        let nii_path = dir.path().join("qform.nii");
        write_nifti(&nii_path, &nifti).unwrap();

        let mgh = read_nifti(&nii_path).unwrap().to_mgh();
        assert_eq!(mgh.dim(), [2, 3, 4, 1]);
        assert_eq!(mgh.header.dtype, MRI_FLOAT);
        assert_eq!(mgh.header.is_ras_good, 1);

        // Expected mdc for a 90 degree rotation about z: row j of the MGH mdc is the direction of
        // voxel axis j (= column j of the rotation matrix).
        let expected_mdc_raw = [
            0.0, 1.0, 0.0, // row 0: axis 0 direction (world +y)
            -1.0, 0.0, 0.0, // row 1: axis 1 direction (world -x)
            0.0, 0.0, 1.0, // row 2: axis 2 direction (world +z)
        ];
        for i in 0..9 {
            assert_abs_diff_eq!(mgh.header.mdc_raw[i], expected_mdc_raw[i], epsilon = 1e-5);
        }
        // The center voxel RAS (c_crs = (1, 1, 2)) is translation + linear * center.
        assert_abs_diff_eq!(mgh.header.p_xyz_c[0], 10.0 - 1.0, epsilon = 1e-4);
        assert_abs_diff_eq!(mgh.header.p_xyz_c[1], 20.0 + 1.0, epsilon = 1e-4);
        assert_abs_diff_eq!(mgh.header.p_xyz_c[2], 30.0 + 2.0, epsilon = 1e-4);
    }

    #[test]
    fn gzipped_nifti_roundtrip() {
        let brain = read_mgh(MGZ_FILE).unwrap();
        let dir = tempdir().unwrap();
        let nii_gz_path = dir.path().join("brain.nii.gz");

        let nifti = Nifti1::from_mgh(brain.clone()).unwrap();
        write_nifti(&nii_gz_path, &nifti).unwrap();

        let brain2 = read_nifti(&nii_gz_path).unwrap().to_mgh();
        assert_same_data(&brain, &brain2);
        assert_same_ras(&brain, &brain2);
    }

    #[test]
    fn header_roundtrips_through_bytes() {
        let nifti = read_nifti(NII_FILE).unwrap();
        let h = &nifti.header;
        // The demo file is little-endian; verify serialization roundtrips in both byte orders.
        for little in [true, false].iter() {
            let bytes = h.to_bytes(*little);
            let h2 = Nifti1Header::from_bytes(&bytes, *little);
            assert_eq!(h, &h2);
        }
    }

    // ----- Security / malformed-input tests (in the style of the other format modules) -----

    #[test]
    fn rejects_a_two_file_volume() {
        let mut hdr_bytes = [0u8; NIFTI_HEADER_SIZE_IN_BYTES];
        BigEndian::write_i32(&mut hdr_bytes[0..4], 348);
        hdr_bytes[344..348].copy_from_slice(&MAGIC_TWO_FILE);
        let mut cursor = Cursor::new(hdr_bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_a_bad_magic_string() {
        let mut hdr_bytes = [0u8; NIFTI_HEADER_SIZE_IN_BYTES];
        BigEndian::write_i32(&mut hdr_bytes[0..4], 348);
        hdr_bytes[344..348].copy_from_slice(&[b'x', b'y', b'z', 0]);
        let mut cursor = Cursor::new(hdr_bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_a_bad_sizeof_hdr() {
        let mut hdr_bytes = [0u8; NIFTI_HEADER_SIZE_IN_BYTES];
        BigEndian::write_i32(&mut hdr_bytes[0..4], 999);
        let mut cursor = Cursor::new(hdr_bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_negative_dim1() {
        // The FreeSurfer "surface hack": dim[1] <= 0.
        let mut h = Nifti1Header::default();
        h.dim[0] = 3;
        h.dim[1] = -100;
        let bytes = h.to_bytes(false);
        let mut cursor = Cursor::new(bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_more_than_4_dimensions() {
        let mut h = Nifti1Header::default();
        h.dim[0] = 5;
        h.dim[1] = 2;
        h.dim[2] = 2;
        h.dim[3] = 2;
        h.dim[4] = 2;
        h.dim[5] = 2; // 5th dim > 1 -> unsupported
        h.datatype = DT_UINT8;
        h.bitpix = 8;
        let bytes = h.to_bytes(false);
        let mut cursor = Cursor::new(bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_an_unsupported_data_type() {
        let mut h = Nifti1Header::default();
        h.dim[0] = 3;
        h.dim[1] = 2;
        h.dim[2] = 2;
        h.dim[3] = 2;
        h.dim[4] = 1;
        h.datatype = 64; // DT_FLOAT64
        h.bitpix = 64;
        let mut file_bytes = h.to_bytes(false).to_vec();
        file_bytes.extend_from_slice(&[0u8; 4]);
        file_bytes.extend_from_slice(&[0u8; 8]);
        let mut cursor = Cursor::new(file_bytes);
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(
            result,
            Err(NeuroformatsError::UnsupportedNiftiDataType(64))
        ));
    }

    #[test]
    fn rejects_an_invalid_vox_offset() {
        let mut h = Nifti1Header::default();
        h.dim[0] = 3;
        h.dim[1] = 2;
        h.dim[2] = 2;
        h.dim[3] = 2;
        h.dim[4] = 1;
        h.datatype = DT_UINT8;
        h.bitpix = 8;
        h.vox_offset = 10.0; // smaller than the header size
        let bytes = h.to_bytes(false);
        let mut cursor = Cursor::new(bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidNiftiFormat(_))));
    }

    #[test]
    fn rejects_nan_in_sform() {
        let mut h = Nifti1Header::default();
        h.dim[0] = 3;
        h.dim[1] = 2;
        h.dim[2] = 2;
        h.dim[3] = 2;
        h.dim[4] = 1;
        h.datatype = DT_UINT8;
        h.bitpix = 8;
        h.sform_code = XFORM_SCANNER_ANAT;
        h.srow_x = [f32::NAN, 0.0, 0.0, 0.0];
        let mut file_bytes = h.to_bytes(false).to_vec();
        file_bytes.extend_from_slice(&[0u8; 4]);
        file_bytes.extend_from_slice(&[0u8; 8]);
        let mut cursor = Cursor::new(file_bytes);
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::InvalidHeaderValue(_))));
    }

    #[test]
    fn rejects_huge_dimensions() {
        // dims that overflow the allocation limit.
        let mut h = Nifti1Header::default();
        h.dim[0] = 4;
        h.dim[1] = 32767;
        h.dim[2] = 32767;
        h.dim[3] = 32767;
        h.dim[4] = 1;
        h.datatype = DT_UINT8;
        h.bitpix = 8;
        let bytes = h.to_bytes(false);
        let mut cursor = Cursor::new(bytes.to_vec());
        let result = Nifti1::from_reader(&mut cursor);
        assert!(matches!(result, Err(NeuroformatsError::AllocationTooLarge)));
    }

    #[test]
    fn the_vox2ras_fix_matches_freesurfer_for_anisotropic_data() {
        // With the corrected convention (column scaling), an MGH with anisotropic voxel sizes must
        // produce a vox2ras whose column j has length delta[j]. Regression test for the vox2ras
        // scaling fix in fs_mgh.rs.
        let mut mh = FsMghHeader::default();
        mh.dim1len = 10;
        mh.dim2len = 10;
        mh.dim3len = 10;
        mh.dim4len = 1;
        mh.dtype = MRI_FLOAT;
        mh.is_ras_good = 1;
        mh.delta = [1.0, 2.0, 3.0];
        mh.mdc_raw = [-1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];
        mh.p_xyz_c = [0.0, 0.0, 0.0];

        let v = mh.vox2ras().unwrap();
        for j in 0..3 {
            let mut norm = 0.0;
            for i in 0..3 {
                norm += v[[i, j]] * v[[i, j]];
            }
            assert_abs_diff_eq!(norm.sqrt(), mh.delta[j], epsilon = 1e-5);
        }
        // And converting to NIfTI and back preserves the vox2ras.
        let volume = FsMgh {
            header: mh,
            data: FsMghData {
                mri_float: Some(Array::from_shape_vec(
                    Dim([10, 10, 10, 1]),
                    vec![0.0f32; 1000],
                ).unwrap()),
                mri_uchar: None,
                mri_int: None,
                mri_short: None,
            },
        };
        let nifti = Nifti1::from_mgh(volume.clone()).unwrap();
        let dir = tempdir().unwrap();
        let nii_path = dir.path().join("aniso.nii");
        write_nifti(&nii_path, &nifti).unwrap();
        let volume2 = read_nifti(&nii_path).unwrap().to_mgh();
        assert_same_ras(&volume, &volume2);
    }
}
