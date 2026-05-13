//! Temporary file management. See [`NamedTempFile`].
//!
//! Companion module to the crate root, which owns [`crate::TempDir`].
//! Both types share the internal [`crate::unique_name`] generator so
//! the `mod-rand` feature controls naming for files and directories
//! in lockstep.

use std::io;
use std::path::{Path, PathBuf};

use crate::unique_name;

/// A temporary file that auto-deletes when dropped.
///
/// Companion to [`crate::TempDir`]. Where `TempDir` manages a
/// directory, `NamedTempFile` manages a single zero-byte file at a
/// fresh path under the OS temp location. The caller reopens the
/// path with [`std::fs::OpenOptions`] (or any other API) when ready
/// to write or read.
///
/// The default basename is `.tmpfile-{pid}-{name12}`, intentionally
/// distinct from [`TempDir`](crate::TempDir)'s `.tmp-{pid}-{name12}`
/// so an operator inspecting the OS temp location can tell the two
/// apart at a glance. The 12 trailing characters use the same
/// Crockford base32 generator as `TempDir`, so the optional
/// `mod-rand` feature controls both types in lockstep. The embedded
/// PID lets [`cleanup_orphans`](crate::cleanup_orphans) identify
/// files left behind by crashed processes.
///
/// # Example
///
/// ```no_run
/// use mod_tempdir::NamedTempFile;
/// use std::io::Write;
///
/// let f = NamedTempFile::new().unwrap();
/// let mut handle = std::fs::OpenOptions::new()
///     .write(true)
///     .open(f.path())
///     .unwrap();
/// handle.write_all(b"hello").unwrap();
/// drop(handle);
/// // `f` is deleted automatically when it goes out of scope.
/// ```
///
/// # Cleanup semantics
///
/// Drop calls [`std::fs::remove_file`] best-effort. A failure (file
/// already gone, permission denied, or a still-open handle on
/// Windows) is intentionally swallowed: a `Drop` impl must not
/// panic. Use [`NamedTempFile::persist`] to keep the file alive past
/// drop.
///
/// # Windows handle-lock caveat
///
/// On Windows, [`std::fs::remove_file`] returns
/// `ERROR_SHARING_VIOLATION` (surfaced in Rust as
/// [`std::io::ErrorKind::PermissionDenied`]) if any process still
/// holds an open handle to the file at the moment of Drop. The
/// library does not retry. Drop must not block, and retries cannot
/// force-close a caller-owned handle. The file is left on disk in
/// that case. Close any handles you open against
/// [`NamedTempFile::path`] before the `NamedTempFile` drops to
/// guarantee cleanup.
#[derive(Debug)]
pub struct NamedTempFile {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl NamedTempFile {
    /// Create a new temporary file in the system's temp location
    /// (`/tmp` on Linux/macOS, `%TEMP%` on Windows).
    ///
    /// The basename is `.tmpfile-{pid}-{name12}` where `{pid}` is
    /// the current process ID (used by
    /// [`cleanup_orphans`](crate::cleanup_orphans) to identify
    /// entries left behind by crashed processes) and `{name12}` is a
    /// 12-character Crockford base32 string from the shared name
    /// generator. The file is materialized via
    /// [`std::fs::File::create`]; the returned `File` handle is
    /// closed before this function returns, so the caller starts
    /// from a clean slate.
    ///
    /// With the `mod-rand` feature enabled, the name fragment comes
    /// from `mod_rand::tier2::unique_name`. Without it, from the
    /// same internal process-unique mixer as
    /// [`TempDir::new`](crate::TempDir::new).
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] from
    /// [`std::fs::File::create`] if the file cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::new().unwrap();
    /// assert!(f.path().is_file());
    /// ```
    pub fn new() -> io::Result<Self> {
        let name = unique_name(12);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!(".tmpfile-{pid}-{name}"));
        std::fs::File::create(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Create a new temporary file with the given prefix.
    ///
    /// The final basename is `{prefix}-{12-char-name}`. The prefix
    /// is joined verbatim and is the caller's responsibility to
    /// sanitize.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] from
    /// [`std::fs::File::create`] if the file cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::with_prefix("my-fixture").unwrap();
    /// assert!(f
    ///     .path()
    ///     .file_name()
    ///     .unwrap()
    ///     .to_string_lossy()
    ///     .starts_with("my-fixture-"));
    /// ```
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        let name = unique_name(12);
        let path = std::env::temp_dir().join(format!("{prefix}-{name}"));
        std::fs::File::create(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Return the path of this temporary file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume this `NamedTempFile` and return the path, disabling
    /// cleanup on drop. The file will persist.
    ///
    /// Use this when you want to inspect contents after a test
    /// fails.
    pub fn persist(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Return `true` if the file will be deleted on drop.
    pub fn cleanup_on_drop(&self) -> bool {
        self.cleanup_on_drop
    }
}

impl Drop for NamedTempFile {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            // Cleanup is best-effort and must not panic in Drop.
            // Filesystem errors (file in use on Windows, permission
            // denied, file already gone) are intentionally swallowed
            // per REPS section 5.
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_file() {
        let f = NamedTempFile::new().unwrap();
        assert!(f.path().exists());
        assert!(f.path().is_file());
    }

    #[test]
    fn auto_cleanup() {
        let path = {
            let f = NamedTempFile::new().unwrap();
            f.path().to_path_buf()
        };
        assert!(!path.exists());
    }

    #[test]
    fn persist_disables_cleanup() {
        let f = NamedTempFile::new().unwrap();
        let path = f.persist();
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn with_prefix_works() {
        let f = NamedTempFile::with_prefix("named").unwrap();
        let name = f.path().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("named-"));
    }

    #[test]
    fn two_files_unique() {
        let a = NamedTempFile::new().unwrap();
        let b = NamedTempFile::new().unwrap();
        assert_ne!(a.path(), b.path());
    }
}
