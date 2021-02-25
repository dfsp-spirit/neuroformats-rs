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

        /// I/O Error
        Io(err: IOError) {
            from()
            source(err)
        }
    }
}

/// Alias type for results originated from this crate.
pub type Result<T> = ::std::result::Result<T, NeuroformatsError>;