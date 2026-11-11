//! Temporary file management. See [`NamedTempFile`].
//!
//! Companion module to the crate root, which owns [`crate::TempDir`].
//! Both types share the internal name generator and the exclusive
//! create-and-retry loop, so the `mod-rand` feature controls naming
//! for files and directories in lockstep.

use std::io;
use std::path::{Path, PathBuf};

use crate::{create_unique, validate_prefix};

/// A temporary file that auto-deletes when dropped.
///
/// Companion to [`crate::TempDir`]. Where `TempDir` manages a
/// directory, `NamedTempFile` manages a single zero-byte file at a
/// fresh path under the OS temp location (or a directory chosen with
/// [`NamedTempFile::new_in`]). The caller writes or reads the path
/// with [`std::fs::write`], [`std::fs::OpenOptions`], or any other
/// API.
///
/// The default basename is `.tmpfile-{pid}-{name12}`, intentionally
/// distinct from [`TempDir`](crate::TempDir)'s `.tmp-{pid}-{name12}`
/// so an operator inspecting the OS temp location can tell the two
/// apart at a glance. The 12 trailing characters come from the same
/// OS-random Crockford base32 generator as `TempDir`, so the optional
/// `mod-rand` feature controls both types in lockstep. The embedded
/// PID lets [`cleanup_orphans`](crate::cleanup_orphans) identify
/// files left behind by crashed processes.
///
/// # Example
///
/// ```
/// use mod_tempdir::NamedTempFile;
///
/// let f = NamedTempFile::new()?;
/// std::fs::write(f.path(), b"hello")?;
/// // `f` is deleted automatically when it goes out of scope.
/// # Ok::<(), std::io::Error>(())
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
    /// 12-character Crockford base32 string drawn from the OS random
    /// source.
    ///
    /// The file is created empty and exclusively
    /// ([`OpenOptions::create_new`](std::fs::OpenOptions::create_new)),
    /// with mode `0600` on Unix. Nothing that already exists at the
    /// chosen path is opened or truncated; if the path is taken, a
    /// fresh name is drawn. The handle is closed before this function
    /// returns, so the caller starts from a clean slate.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] if the file cannot be
    /// created, or [`io::ErrorKind::AlreadyExists`] if 64 fresh names
    /// in a row were all taken.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::new()?;
    /// assert!(f.path().is_file());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new() -> io::Result<Self> {
        Self::new_in(std::env::temp_dir())
    }

    /// Create a new temporary file inside `dir`.
    ///
    /// Same naming, permissions and cleanup as
    /// [`NamedTempFile::new`], but under a parent directory of your
    /// choice, which must already exist. Create the file next to the
    /// final destination when you plan to call
    /// [`persist_atomic`](Self::persist_atomic): `rename` is atomic
    /// only within one filesystem.
    /// [`cleanup_orphans`](crate::cleanup_orphans) only sweeps the OS
    /// temp location, so files left behind under other parents are
    /// not swept.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] if the file cannot be
    /// created (for example, `dir` does not exist), or
    /// [`io::ErrorKind::AlreadyExists`] if 64 fresh names in a row were
    /// all taken.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::{NamedTempFile, TempDir};
    ///
    /// let target_dir = TempDir::new()?;
    /// let target = target_dir.path().join("config.toml");
    ///
    /// let tmp = NamedTempFile::new_in(target_dir.path())?;
    /// std::fs::write(tmp.path(), b"key = 1")?;
    /// tmp.persist_atomic(&target)?;
    /// assert_eq!(std::fs::read(&target)?, b"key = 1");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new_in(dir: impl AsRef<Path>) -> io::Result<Self> {
        let pid = std::process::id();
        let path = create_unique(
            dir.as_ref(),
            |name| format!(".tmpfile-{pid}-{name}"),
            create_private_file,
        )?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Create a new temporary file with the given prefix.
    ///
    /// The final basename is `{prefix}-{12-char-name}`, created in the
    /// system's temp location with the same exclusive creation and
    /// permissions as [`NamedTempFile::new`].
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] if `prefix` contains a
    /// path separator (`/` or `\`), `..`, a NUL byte, or (on Windows) a
    /// `:`. Otherwise returns the same errors as
    /// [`NamedTempFile::new`].
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::with_prefix("my-fixture")?;
    /// assert!(f
    ///     .path()
    ///     .file_name()
    ///     .unwrap()
    ///     .to_string_lossy()
    ///     .starts_with("my-fixture-"));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        Self::with_prefix_in(std::env::temp_dir(), prefix)
    }

    /// Create a new temporary file with the given prefix inside `dir`.
    ///
    /// Combines [`NamedTempFile::new_in`] and
    /// [`NamedTempFile::with_prefix`].
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] for a prefix that
    /// [`NamedTempFile::with_prefix`] rejects. Otherwise returns the
    /// same errors as [`NamedTempFile::new_in`].
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::{NamedTempFile, TempDir};
    ///
    /// let dir = TempDir::new()?;
    /// let f = NamedTempFile::with_prefix_in(dir.path(), "upload")?;
    /// assert_eq!(f.path().parent(), Some(dir.path()));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn with_prefix_in(dir: impl AsRef<Path>, prefix: &str) -> io::Result<Self> {
        validate_prefix(prefix)?;
        let path = create_unique(
            dir.as_ref(),
            |name| format!("{prefix}-{name}"),
            create_private_file,
        )?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Return the path of this temporary file.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::new().unwrap();
    /// let mut handle = std::fs::OpenOptions::new()
    ///     .write(true)
    ///     .open(f.path())
    ///     .unwrap();
    /// # let _ = handle;
    /// ```
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume this `NamedTempFile` and return the path, disabling
    /// cleanup on drop. The file will persist.
    ///
    /// Use this when you want to inspect contents after a test
    /// fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::new().unwrap();
    /// let kept = f.persist();
    /// // `kept` survives past the original `f` going out of scope.
    /// # std::fs::remove_file(&kept).unwrap();
    /// ```
    pub fn persist(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Return `true` if the file will be deleted on drop.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let f = NamedTempFile::new().unwrap();
    /// assert!(f.cleanup_on_drop());
    /// ```
    pub fn cleanup_on_drop(&self) -> bool {
        self.cleanup_on_drop
    }

    /// Atomically move this file to `target` with crash-safety
    /// guarantees, then disable cleanup on drop.
    ///
    /// Performs the canonical "atomic durable write" sequence:
    ///
    /// 1. `fsync` the temp file contents to disk
    ///    ([`std::fs::File::sync_all`]).
    /// 2. Atomically rename the temp file onto `target` via
    ///    [`std::fs::rename`]. On Unix this is `rename(2)`; on
    ///    Windows it is `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`.
    ///    Both are atomic within a single filesystem.
    /// 3. Best-effort `fsync` of the target's parent directory so
    ///    the rename itself survives a crash. Failures here are
    ///    silent, matching the rest of the crate's durability story.
    ///
    /// On success, the temp file no longer exists at
    /// [`path`](Self::path); the data lives at `target`. Cleanup on
    /// drop is disabled and the consumed `self` does not attempt
    /// removal.
    ///
    /// # Errors
    ///
    /// On any failure (fsync, rename, etc.), the temp file is
    /// **preserved** on disk and returned to the caller via
    /// [`PersistAtomicError::file`]. The caller can inspect the
    /// underlying [`io::Error`], optionally fix the cause (e.g.,
    /// create the missing parent directory), and retry. This is
    /// the standard `tempfile`-crate pattern and matches the
    /// data-integrity guarantee that a failed atomic-persist must
    /// never lose the source.
    ///
    /// Common error causes:
    /// - Target's parent directory does not exist.
    /// - Target's parent is on a different filesystem (`EXDEV` on
    ///   Unix, `ERROR_NOT_SAME_DEVICE` on Windows).
    /// - Permission denied at the target location.
    /// - Source temp file already removed (race with cleanup).
    ///
    /// # Cross-filesystem behaviour
    ///
    /// `rename` is atomic only within a single filesystem. If
    /// `target` is on a different mount than the temp file,
    /// `rename` will return `EXDEV` on Unix or the equivalent on
    /// Windows. Create the temp file next to the target with
    /// [`NamedTempFile::new_in`] (for example, in `target`'s parent
    /// directory) so both are on the same filesystem.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    /// use std::io::Write;
    ///
    /// let f = NamedTempFile::new().unwrap();
    /// {
    ///     let mut h = std::fs::OpenOptions::new()
    ///         .write(true)
    ///         .open(f.path())
    ///         .unwrap();
    ///     h.write_all(b"finalized payload").unwrap();
    /// }
    ///
    /// let target = std::env::temp_dir().join("finalized.bin");
    /// let landed = f.persist_atomic(&target).unwrap();
    /// assert_eq!(landed, target);
    /// # std::fs::remove_file(&landed).unwrap();
    /// ```
    ///
    /// Retry pattern on recoverable error:
    ///
    /// ```no_run
    /// use mod_tempdir::NamedTempFile;
    ///
    /// let mut f = NamedTempFile::new().unwrap();
    /// let target = std::env::temp_dir().join("retry-target");
    /// loop {
    ///     match f.persist_atomic(&target) {
    ///         Ok(_landed) => break,
    ///         Err(e) => {
    ///             eprintln!("persist failed: {}", e.error);
    ///             // ... fix the underlying issue ...
    ///             f = e.file; // recover the temp file and try again
    ///             # break;
    ///         }
    ///     }
    /// }
    /// # std::fs::remove_file(&target).ok();
    /// ```
    pub fn persist_atomic(
        mut self,
        target: impl AsRef<Path>,
    ) -> Result<PathBuf, PersistAtomicError> {
        let target = target.as_ref();

        // Step 1: fsync the source. A writable handle is needed for
        // `sync_all` semantics on every platform we support. If
        // either the open or the fsync fails, return `self` to the
        // caller so the temp file is preserved.
        match std::fs::OpenOptions::new().write(true).open(&self.path) {
            Ok(handle) => {
                if let Err(error) = handle.sync_all() {
                    return Err(PersistAtomicError { error, file: self });
                }
            }
            Err(error) => return Err(PersistAtomicError { error, file: self }),
        }

        // Step 2: atomic rename. `std::fs::rename` is POSIX
        // `rename(2)` on Unix and `MoveFileExW` with
        // `MOVEFILE_REPLACE_EXISTING` on Windows. Both are atomic
        // within a single filesystem.
        if let Err(error) = std::fs::rename(&self.path, target) {
            return Err(PersistAtomicError { error, file: self });
        }

        // Step 3: best-effort fsync of the target's parent directory
        // so the rename itself is durable across a crash. Failures
        // are intentionally silent, matching the Drop philosophy.
        if let Some(parent) = target.parent() {
            let _ = sync_directory(parent);
        }

        // The temp file no longer exists at `self.path`. Disable
        // cleanup explicitly so Drop does not attempt a no-op
        // `remove_file` against a path that has moved.
        self.cleanup_on_drop = false;

        Ok(target.to_path_buf())
    }
}

/// Error returned by [`NamedTempFile::persist_atomic`] when the
/// atomic-persist sequence fails partway through.
///
/// The underlying [`io::Error`] is in [`PersistAtomicError::error`]
/// and the original [`NamedTempFile`] is in
/// [`PersistAtomicError::file`], preserved so the caller can retry
/// or fall back to other cleanup logic without losing the source.
///
/// # Example
///
/// ```no_run
/// use mod_tempdir::{NamedTempFile, PersistAtomicError};
///
/// fn persist_or_log(f: NamedTempFile, target: &str) {
///     match f.persist_atomic(target) {
///         Ok(landed) => println!("persisted to {}", landed.display()),
///         Err(PersistAtomicError { error, file }) => {
///             eprintln!("persist failed: {error}");
///             // `file` is the original NamedTempFile, intact.
///             // It will be cleaned up on Drop, or you can retry.
///             drop(file);
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct PersistAtomicError {
    /// The underlying I/O error that aborted the atomic persist.
    pub error: io::Error,
    /// The `NamedTempFile` that would have been moved, returned
    /// intact so the caller can retry or drop it.
    pub file: NamedTempFile,
}

impl std::fmt::Display for PersistAtomicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "atomic persist failed: {}", self.error)
    }
}

impl std::error::Error for PersistAtomicError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<PersistAtomicError> for io::Error {
    fn from(e: PersistAtomicError) -> Self {
        e.error
    }
}

/// Create one file exclusively, owner-only on Unix. Never opens,
/// truncates or follows an entry that already exists at `path`.
fn create_private_file(path: &Path) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map(drop)
}

/// Best-effort fsync of a directory. Used by
/// [`NamedTempFile::persist_atomic`] to make the rename durable.
///
/// Linux / macOS: open the directory and call `sync_all` (`fsync` on
/// the directory fd).
///
/// Windows: open with `FILE_FLAG_BACKUP_SEMANTICS` (required to get
/// a directory handle) and call `sync_all`. Directory fsync semantics
/// on NTFS are less load-bearing than on Unix; this is still
/// best-effort.
#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    let dir = std::fs::File::open(path)?;
    dir.sync_all()
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let dir = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    dir.sync_all()
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    // No portable directory fsync primitive available on this
    // platform; rename atomicity is the only durability guarantee.
    Ok(())
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
