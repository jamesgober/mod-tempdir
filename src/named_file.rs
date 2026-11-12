//! Temporary file management. See [`NamedTempFile`].
//!
//! Companion module to the crate root, which owns [`crate::TempDir`].
//! Both types share the internal name generator and the exclusive
//! create-and-retry loop, so the `mod-rand` feature controls naming
//! for files and directories in lockstep.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::sys::{file_id, path_id_no_follow};
use crate::{create_unique, validate_prefix};

/// A temporary file that auto-deletes when dropped.
///
/// Companion to [`crate::TempDir`]. Where `TempDir` manages a
/// directory, `NamedTempFile` manages a single zero-byte file at a
/// fresh path under the OS temp location (or a directory chosen with
/// [`NamedTempFile::new_in`]).
///
/// The value keeps the handle it created the file with, open for
/// reading and writing. Write through it with [`as_file_mut`],
/// [`as_file`] or the [`Write`] impl on `NamedTempFile`, and open
/// more handles with [`reopen`], which checks that the path still
/// names this file. [`persist_atomic`] syncs through the same handle
/// and refuses to rename if the path no longer names it.
///
/// Access by path ([`path`] with [`std::fs::write`] or
/// [`std::fs::OpenOptions`]) also works, but only the creation step
/// is exclusive: anyone who can write to the parent directory can
/// replace the entry with a link or another file after it is created,
/// and a later open by name acts on whatever is there. Use path access
/// only in a directory no other user or service can write, such as
/// the default per-user `%TEMP%` on Windows or a sticky `/tmp` on
/// Unix.
///
/// [`as_file`]: NamedTempFile::as_file
/// [`as_file_mut`]: NamedTempFile::as_file_mut
/// [`reopen`]: NamedTempFile::reopen
/// [`persist_atomic`]: NamedTempFile::persist_atomic
/// [`path`]: NamedTempFile::path
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
/// use std::io::Write;
///
/// let mut f = NamedTempFile::new()?;
/// f.write_all(b"hello")?;
/// assert_eq!(std::fs::read(f.path())?, b"hello");
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
    file: File,
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
    /// fresh name is drawn. The handle from that exclusive create is
    /// kept open for reading and writing; see
    /// [`as_file`](Self::as_file).
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
    /// temp location; sweep other parents with
    /// [`cleanup_orphans_in`](crate::cleanup_orphans_in).
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
        let (path, file) = create_unique(
            dir.as_ref(),
            |name| format!(".tmpfile-{pid}-{name}"),
            create_private_file,
        )?;
        Ok(Self {
            path,
            file,
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
        let (path, file) = create_unique(
            dir.as_ref(),
            |name| format!("{prefix}-{name}"),
            create_private_file,
        )?;
        Ok(Self {
            path,
            file,
            cleanup_on_drop: true,
        })
    }

    /// Return the path of this temporary file.
    ///
    /// Opening this path by name is safe only in a directory no one
    /// else can write; see the type docs. Prefer
    /// [`as_file`](Self::as_file) or [`reopen`](Self::reopen).
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
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the handle this file was created with.
    ///
    /// The handle is open for reading and writing and always refers to
    /// the file this value created, whatever later happens at
    /// [`path`](Self::path). `&File` implements [`Read`], [`Write`]
    /// and [`Seek`], so a shared reference is enough to do I/O.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::NamedTempFile;
    /// use std::io::Write;
    ///
    /// let f = NamedTempFile::new()?;
    /// let mut handle = f.as_file();
    /// handle.write_all(b"hello")?;
    /// assert_eq!(std::fs::read(f.path())?, b"hello");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn as_file(&self) -> &File {
        &self.file
    }

    /// Return the handle this file was created with, mutably.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::NamedTempFile;
    /// use std::io::Write;
    ///
    /// let mut f = NamedTempFile::new()?;
    /// f.as_file_mut().write_all(b"hello")?;
    /// f.as_file_mut().set_len(2)?;
    /// assert_eq!(std::fs::read(f.path())?, b"he");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn as_file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    /// Open a new read and write handle to this file.
    ///
    /// The path is opened again and the result is checked against the
    /// held handle, so the new handle refers to the same file. The new
    /// handle has its own cursor.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] if the path cannot be
    /// opened, or an error of kind [`io::ErrorKind::Other`] if the
    /// path no longer names the file this value created (it was
    /// removed and replaced).
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::NamedTempFile;
    /// use std::io::{Read, Write};
    ///
    /// let mut f = NamedTempFile::new()?;
    /// f.write_all(b"hello")?;
    /// let mut text = String::new();
    /// f.reopen()?.read_to_string(&mut text)?;
    /// assert_eq!(text, "hello");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn reopen(&self) -> io::Result<File> {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)?;
        match (file_id(&self.file)?, file_id(&opened)?) {
            (Some(held), Some(now)) if held != now => Err(replaced_error()),
            _ => Ok(opened),
        }
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
    /// 1. `fsync` the temp file contents to disk through the handle
    ///    held since creation ([`std::fs::File::sync_all`]).
    /// 2. Check that [`path`](Self::path) still names that file. If
    ///    the entry was removed and replaced (by a link or another
    ///    file), the persist fails with [`io::ErrorKind::Other`] and
    ///    nothing is renamed. On Unix the check compares device and
    ///    inode numbers without following a symbolic link; on Windows
    ///    it compares the volume serial number and file index of the
    ///    entry itself, not of a reparse point's target. The rename
    ///    still goes by name, so a party that can write to the parent
    ///    directory has a short window between the check and the
    ///    rename; keep files that need full protection in a directory
    ///    no one else can write.
    /// 3. Atomically rename the temp file onto `target` via
    ///    [`std::fs::rename`]. On Unix this is `rename(2)`; on
    ///    Windows it is `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`.
    ///    Both are atomic within a single filesystem.
    /// 4. Best-effort `fsync` of the target's parent directory so
    ///    the rename itself survives a crash. Failures here are
    ///    silent, matching the rest of the crate's durability story.
    ///    A bare file name as `target` syncs the current directory.
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
    /// - Source temp file already removed (race with cleanup), or
    ///   replaced by another entry.
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
    /// ```
    /// use mod_tempdir::{NamedTempFile, TempDir};
    /// use std::io::Write;
    ///
    /// let dir = TempDir::new()?;
    /// let target = dir.path().join("finalized.bin");
    ///
    /// let mut f = NamedTempFile::new_in(dir.path())?;
    /// f.write_all(b"finalized payload")?;
    /// let landed = f.persist_atomic(&target)?;
    /// assert_eq!(landed, target);
    /// assert_eq!(std::fs::read(&target)?, b"finalized payload");
    /// # Ok::<(), std::io::Error>(())
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

        // Step 1: fsync through the handle held since the exclusive
        // create. On failure, return `self` so the temp file is
        // preserved.
        if let Err(error) = self.file.sync_all() {
            return Err(PersistAtomicError { error, file: self });
        }

        // Step 2: refuse to publish an entry that is no longer the
        // file this value created.
        if let Err(error) = self.check_path_is_ours() {
            return Err(PersistAtomicError { error, file: self });
        }

        // Step 3: atomic rename. `std::fs::rename` is POSIX
        // `rename(2)` on Unix and `MoveFileExW` with
        // `MOVEFILE_REPLACE_EXISTING` on Windows. Both are atomic
        // within a single filesystem.
        if let Err(error) = std::fs::rename(&self.path, target) {
            return Err(PersistAtomicError { error, file: self });
        }

        // Step 4: best-effort fsync of the target's parent directory
        // so the rename itself is durable across a crash. Failures
        // are intentionally silent, matching the Drop philosophy. A
        // bare file name has an empty parent, which means the current
        // directory.
        if let Some(parent) = target.parent() {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            let _ = sync_directory(parent);
        }

        // The temp file no longer exists at `self.path`. Disable
        // cleanup explicitly so Drop does not attempt a no-op
        // `remove_file` against a path that has moved.
        self.cleanup_on_drop = false;

        Ok(target.to_path_buf())
    }

    /// Check that `self.path` still names the file behind `self.file`.
    fn check_path_is_ours(&self) -> io::Result<()> {
        match (file_id(&self.file)?, path_id_no_follow(&self.path)?) {
            (Some(held), Some(now)) if held != now => Err(replaced_error()),
            _ => Ok(()),
        }
    }
}

/// The error for a path that no longer names the file this
/// `NamedTempFile` created.
fn replaced_error() -> io::Error {
    io::Error::other("the temporary file's path no longer names the file it created")
}

impl Read for NamedTempFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for NamedTempFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for NamedTempFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
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
/// Returns the handle, open for reading and writing.
fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
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
