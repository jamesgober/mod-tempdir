//! # mod-tempdir
//!
//! Temporary directory and file management for Rust. Auto-cleanup on
//! Drop, collision-resistant naming, cross-platform paths.
//!
//! Two types and one orphan-cleanup function:
//!
//! * [`TempDir`]: a directory created under the OS temp location,
//!   recursively deleted on Drop.
//! * [`NamedTempFile`]: a single file created under the OS temp
//!   location, deleted on Drop.
//! * [`cleanup_orphans`]: sweeps the OS temp directory for entries
//!   left behind by crashed processes and removes those that are
//!   both PID-dead and older than a caller-supplied age threshold.
//!
//! Both types share the same name-generation pipeline, the same
//! `with_prefix` / `persist` / `cleanup_on_drop` API shape, and the
//! same silent best-effort Drop semantics.
//!
//! Designed as a `tempfile` replacement at MSRV 1.75. The default
//! build has zero runtime dependencies outside `std`. An optional
//! `mod-rand` feature swaps the built-in name mixer for
//! `mod_rand::tier2::unique_name`, which produces a uniformly
//! distributed name from a SplitMix + Stafford-finisher pipeline.
//!
//! ## Quick example
//!
//! ```no_run
//! use mod_tempdir::{NamedTempFile, TempDir};
//!
//! let dir = TempDir::new().unwrap();
//! // ... use dir.path() to do work ...
//!
//! let file = NamedTempFile::new().unwrap();
//! // ... use file.path() to write into the file ...
//!
//! // Both are deleted automatically when they go out of scope.
//! ```
//!
//! ## Feature flags
//!
//! * `mod-rand` (off by default): use [`mod_rand::tier2::unique_name`][mr-tier2]
//!   for naming. The alphabet is Crockford base32 on both paths, so
//!   any caller pattern-matching on the directory or file basename
//!   keeps working unchanged when the feature is toggled. Applies to
//!   both [`TempDir`] and [`NamedTempFile`].
//!
//! [mr-tier2]: https://docs.rs/mod-rand/latest/mod_rand/tier2/fn.unique_name.html
//!
//! To enable in `Cargo.toml`:
//!
//! ```toml
//! mod-tempdir = { version = "0.9", features = ["mod-rand"] }
//! ```
//!
//! ## Cleanup semantics
//!
//! `Drop::drop` removes the directory via
//! [`std::fs::remove_dir_all`] (for [`TempDir`]) or the file via
//! [`std::fs::remove_file`] (for [`NamedTempFile`]). Failures during
//! cleanup (file in use, permission denied, network filesystem
//! hiccup) are intentionally silent: a `Drop` impl must not panic.
//! Use `persist()` to keep the entry alive past drop if you need to
//! inspect it. See [`NamedTempFile`] for a Windows-specific note
//! about open file handles.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod cleanup;
mod named_file;

pub use cleanup::cleanup_orphans;
pub use named_file::{NamedTempFile, PersistAtomicError};

use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(feature = "mod-rand"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(feature = "mod-rand"))]
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary directory that auto-deletes when dropped.
///
/// # Example
///
/// ```no_run
/// use mod_tempdir::TempDir;
///
/// let dir = TempDir::new().unwrap();
/// let file_path = dir.path().join("test.txt");
/// std::fs::write(&file_path, b"hello").unwrap();
/// // dir and its contents are deleted at end of scope
/// ```
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl TempDir {
    /// Create a new temporary directory in the system's temp location
    /// (`/tmp` on Linux/macOS, `%TEMP%` on Windows).
    ///
    /// The basename is `.tmp-{pid}-{name12}` where `{pid}` is the
    /// current process ID (used by [`cleanup_orphans`] to identify
    /// entries left behind by crashed processes) and `{name12}` is a
    /// 12-character Crockford base32 string from the shared name
    /// generator. With the `mod-rand` feature enabled, the name
    /// fragment comes from [`mod_rand::tier2::unique_name`][mr-tier2];
    /// without it, from an internal process-unique mixer.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] from
    /// [`std::fs::create_dir`] if the directory cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// assert!(dir.path().is_dir());
    /// ```
    ///
    /// [mr-tier2]: https://docs.rs/mod-rand/latest/mod_rand/tier2/fn.unique_name.html
    pub fn new() -> io::Result<Self> {
        let name = unique_name(12);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!(".tmp-{pid}-{name}"));
        std::fs::create_dir(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Create a new temporary directory with the given prefix.
    ///
    /// The final basename is `{prefix}-{12-char-name}`. The prefix is
    /// joined verbatim and is the caller's responsibility to sanitize.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] from
    /// [`std::fs::create_dir`] if the directory cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::with_prefix("my-app").unwrap();
    /// assert!(dir
    ///     .path()
    ///     .file_name()
    ///     .unwrap()
    ///     .to_string_lossy()
    ///     .starts_with("my-app-"));
    /// ```
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        let name = unique_name(12);
        let path = std::env::temp_dir().join(format!("{prefix}-{name}"));
        std::fs::create_dir(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Return the path of this temporary directory.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// let log = dir.path().join("output.log");
    /// std::fs::write(&log, b"hello").unwrap();
    /// ```
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume this `TempDir` and return the path, disabling cleanup
    /// on drop. The directory and its contents will persist.
    ///
    /// Use this when you want to inspect contents after a test fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// let kept = dir.persist();
    /// // `kept` survives past the original `dir` going out of scope.
    /// # std::fs::remove_dir_all(&kept).unwrap();
    /// ```
    pub fn persist(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Return `true` if the directory will be deleted on drop.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// assert!(dir.cleanup_on_drop());
    /// ```
    pub fn cleanup_on_drop(&self) -> bool {
        self.cleanup_on_drop
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            // Cleanup is best-effort and must not panic in Drop. Any
            // filesystem error (file in use, permission denied) is
            // intentionally swallowed per REPS section 5.
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(feature = "mod-rand")]
#[inline]
pub(crate) fn unique_name(len: usize) -> String {
    mod_rand::tier2::unique_name(len)
}

#[cfg(not(feature = "mod-rand"))]
pub(crate) fn unique_name(len: usize) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    let pid = std::process::id() as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    // Placeholder mixing. The `mod-rand` feature replaces this entire
    // function with `mod_rand::tier2::unique_name`.
    let mut state = pid.wrapping_mul(0x9E3779B97F4A7C15)
        ^ nanos.wrapping_mul(0xBF58476D1CE4E5B9)
        ^ counter.wrapping_mul(0x94D049BB133111EB);

    let mut out = String::with_capacity(len);
    while out.len() < len {
        out.push(ALPHABET[(state & 31) as usize] as char);
        state >>= 5;
        if state == 0 {
            state = nanos.wrapping_mul(counter.wrapping_add(1));
        }
    }
    out
}

/// Internal test hook. **Not part of the stable public API.** This
/// symbol exists only to let this crate's integration tests exercise
/// the name generator without paying for a filesystem syscall per
/// sample. External code must not call it; it may be renamed or
/// removed in any release, including a patch.
///
/// Only compiled when the `mod-rand` feature is enabled, since that is
/// the only test file that needs it.
#[cfg(feature = "mod-rand")]
#[doc(hidden)]
pub fn __unique_name_for_tests(len: usize) -> String {
    unique_name(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_dir() {
        let dir = TempDir::new().unwrap();
        assert!(dir.path().exists());
        assert!(dir.path().is_dir());
    }

    #[test]
    fn auto_cleanup() {
        let path = {
            let dir = TempDir::new().unwrap();
            dir.path().to_path_buf()
        };
        // After drop, the directory should no longer exist.
        assert!(!path.exists());
    }

    #[test]
    fn persist_disables_cleanup() {
        let dir = TempDir::new().unwrap();
        let path = dir.persist();
        assert!(path.exists());
        // Clean up manually since persist was used.
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn with_prefix_works() {
        let dir = TempDir::with_prefix("test").unwrap();
        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("test-"));
    }

    #[test]
    fn two_dirs_unique() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        assert_ne!(a.path(), b.path());
    }
}
