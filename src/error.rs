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

        /// I/O Error
        Io(err: IOError) {
            from()
            source(err)
        }
    }
}

/// Alias type for results originated from this crate.
pub type Result<T> = ::std::result::Result<T, NeuroformatsError>;