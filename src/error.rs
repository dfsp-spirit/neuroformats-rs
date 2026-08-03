//! Errors one may encounter when using neuroformats.

use quick_error::quick_error;
use std::io::Error as IOError;

quick_error! {
    /// Error type for all error variants originated by this crate.
    #[derive(Debug)]
    pub enum NeuroformatsError {
        /// Invalid curv file: wrong magic number.
        InvalidCurvFormat {
            display("Invalid Curv file")
        }

        VertexColorCountMismatch {
            display("Invalid number of vertex colors for the mesh")
        }

        InvalidFsSurfaceFormat {
            display("Invalid FreeSurfer surf file")
        }

        InvalidFsLabelFormat {
            display("Invalid FreeSurfer label file")
        }

        InvalidWavefrontObjectFormat {
            display("Invalid Wavefront Object format file or unsupported dialect")
        }

        UnsupportedFsAnnotFormatVersion {
            display("Unsupported FreeSurfer annot file format version")
        }

        EmptyWavefrontObjectFile {
            display("The Wavefront Object mesh file does not contain a mesh")
        }

        InvalidFsMghFormat {
            display("Invalid FreeSurfer MGH file")
        }

        UnsupportedMriDataTypeInMgh {
            display("Invalid or unsupported MRI_DTYPE")
        }

        NoRasInformationInHeader {
            display("The MGH header does not contain valid RAS information.")
        }

        /// Requested allocation exceeds the configured maximum (see [`crate::config::set_max_bytes_per_file`]).
        AllocationTooLarge {
            display("Requested allocation exceeds maximum allowed size. Increase the limit with neuroformats::set_max_bytes_per_file() if the file is legitimate.")
        }

        /// A header field contains an invalid value (NaN, Inf, or negative dimension).
        InvalidHeaderValue(msg: String) {
            display("Invalid header value: {}", msg)
        }

        /// Integer overflow when computing required allocation size from dimensions.
        IntegerOverflow {
            display("Integer overflow when computing allocation size from header dimensions")
        }

        /// A variable-length string exceeds the maximum allowed length.
        StringTooLong {
            display("Variable-length string exceeds the configured maximum length")
        }

        /// Vertex data contains NaN or infinite floating-point values.
        InvalidVertexValue(msg: String) {
            display("Invalid vertex value: {}", msg)
        }

        /// The requested brain region was not found in the annotation.
        RegionNotFound(region: String) {
            display("Region '{}' not found in annotation", region)
        }

        /// I/O Error
        Io(err: IOError) {
            from()
            source(err)
        }
    }
}

/// Alias type for results originated from this crate.
pub type Result<T> = ::std::result::Result<T, NeuroformatsError>;
