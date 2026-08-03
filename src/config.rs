//! Global configuration for neuroformats loading behavior.
//!
//! Controls allocation limits and safety bounds when reading neuroimaging files.
//! These settings help prevent denial-of-service attacks via malformed files that
//! claim to contain enormous amounts of data.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Default maximum bytes per file load: 1 GiB.
const DEFAULT_MAX_BYTES_PER_FILE: usize = 1_073_741_824;

/// Default maximum number of vertices for surface/curv/annot files.
const DEFAULT_MAX_VERTICES: usize = 10_000_000;

/// Default maximum length for variable-length strings read from files.
const DEFAULT_MAX_STRING_LENGTH: usize = 1_048_576; // 1 MiB

/// Default maximum number of entries in a label file.
const DEFAULT_MAX_LABEL_ENTRIES: usize = 10_000_000;

static MAX_BYTES_PER_FILE: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_BYTES_PER_FILE);
static MAX_VERTICES: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_VERTICES);
static MAX_STRING_LENGTH: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_STRING_LENGTH);
static MAX_LABEL_ENTRIES: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_LABEL_ENTRIES);

/// Get the current maximum allowed bytes per file load.
///
/// Defaults to 1 GiB (1,073,741,824 bytes).
pub fn max_bytes_per_file() -> usize {
    MAX_BYTES_PER_FILE.load(Ordering::Relaxed)
}

/// Set the maximum allowed bytes per file load.
///
/// This limit is checked before allocating memory when reading any file format.
/// Set to a higher value for large datasets, or lower for stricter security.
///
/// # Examples
///
/// ```
/// neuroformats::set_max_bytes_per_file(512 * 1024 * 1024); // 512 MiB
/// ```
pub fn set_max_bytes_per_file(limit: usize) {
    MAX_BYTES_PER_FILE.store(limit, Ordering::Relaxed);
}

/// Get the current maximum allowed vertex count for surface/curv/annot files.
///
/// Defaults to 10 million vertices.
pub fn max_vertices() -> usize {
    MAX_VERTICES.load(Ordering::Relaxed)
}

/// Set the maximum allowed vertex count for surface/curv/annot files.
pub fn set_max_vertices(limit: usize) {
    MAX_VERTICES.store(limit, Ordering::Relaxed);
}

/// Get the current maximum allowed string length when reading from files.
///
/// Defaults to 1 MiB.
pub fn max_string_length() -> usize {
    MAX_STRING_LENGTH.load(Ordering::Relaxed)
}

/// Set the maximum allowed string length when reading from files.
pub fn set_max_string_length(limit: usize) {
    MAX_STRING_LENGTH.store(limit, Ordering::Relaxed);
}

/// Get the current maximum allowed label entries.
///
/// Defaults to 10 million entries.
pub fn max_label_entries() -> usize {
    MAX_LABEL_ENTRIES.load(Ordering::Relaxed)
}

/// Set the maximum allowed label entries.
pub fn set_max_label_entries(limit: usize) {
    MAX_LABEL_ENTRIES.store(limit, Ordering::Relaxed);
}

/// Reset all limits to their default values.
pub fn reset_limits_to_defaults() {
    MAX_BYTES_PER_FILE.store(DEFAULT_MAX_BYTES_PER_FILE, Ordering::Relaxed);
    MAX_VERTICES.store(DEFAULT_MAX_VERTICES, Ordering::Relaxed);
    MAX_STRING_LENGTH.store(DEFAULT_MAX_STRING_LENGTH, Ordering::Relaxed);
    MAX_LABEL_ENTRIES.store(DEFAULT_MAX_LABEL_ENTRIES, Ordering::Relaxed);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn defaults_are_reasonable() {
        assert_eq!(max_bytes_per_file(), 1_073_741_824); // 1 GiB
        assert_eq!(max_vertices(), 10_000_000);
        assert_eq!(max_string_length(), 1_048_576); // 1 MiB
        assert_eq!(max_label_entries(), 10_000_000);
    }

    #[test]
    fn limits_can_be_changed_and_reset() {
        set_max_bytes_per_file(1024);
        assert_eq!(max_bytes_per_file(), 1024);

        set_max_vertices(100);
        assert_eq!(max_vertices(), 100);

        reset_limits_to_defaults();
        assert_eq!(max_bytes_per_file(), 1_073_741_824);
        assert_eq!(max_vertices(), 10_000_000);
    }
}
