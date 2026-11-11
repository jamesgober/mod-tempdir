//! # mod-tempdir
//!
//! Temporary directory and file management for Rust. Auto-cleanup on
//! Drop, unpredictable names, private permissions, cross-platform
//! paths.
//!
//! Two types and one orphan-cleanup function:
//!
//! * [`TempDir`]: a directory created under the OS temp location (or
//!   a directory you choose with [`TempDir::new_in`]), recursively
//!   deleted on Drop.
//! * [`NamedTempFile`]: a single file created under the OS temp
//!   location (or a directory you choose with
//!   [`NamedTempFile::new_in`]), deleted on Drop.
//! * [`cleanup_orphans`]: sweeps the OS temp directory for entries
//!   left behind by crashed processes and removes those that are
//!   both PID-dead and older than a caller-supplied age threshold.
//!
//! Both types share the same name-generation pipeline, the same
//! `new_in` / `with_prefix` / `persist` / `cleanup_on_drop` API shape,
//! and the same silent best-effort Drop semantics.
//!
//! Designed as a `tempfile` replacement at MSRV 1.75. The default
//! build has zero runtime dependencies outside `std`.
//!
//! ## Quick example
//!
//! ```
//! use mod_tempdir::{NamedTempFile, TempDir};
//!
//! let dir = TempDir::new()?;
//! std::fs::write(dir.path().join("notes.txt"), b"hello")?;
//!
//! let file = NamedTempFile::new()?;
//! std::fs::write(file.path(), b"hello")?;
//!
//! // Both are deleted automatically when they go out of scope.
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! ## How entries are created
//!
//! * **Exclusive creation.** Files are opened with
//!   [`OpenOptions::create_new`](std::fs::OpenOptions::create_new) and
//!   directories with [`std::fs::DirBuilder`], so creation never opens,
//!   truncates or follows anything that already exists at the chosen
//!   path. If the path is taken, a fresh name is drawn and creation is
//!   retried (up to 64 attempts).
//! * **Unpredictable names.** The 12-character name fragment carries
//!   60 bits drawn from the operating system's random source in every
//!   build. The default build gets them through `std`'s
//!   [`RandomState`], whose
//!   keys `std` reads from the OS random source; with the `mod-rand`
//!   feature the fragment comes straight from
//!   `mod_rand::tier3::random_base32`.
//! * **Private permissions.** On Unix, files are created with mode
//!   `0600` and directories with mode `0700` (the process umask can
//!   only remove bits from these). On Windows, new entries inherit the
//!   access control list of the directory they are created in. The
//!   default `%TEMP%` lives inside the user's profile, which grants
//!   access to that user, `SYSTEM` and administrators only. This crate
//!   does not set Windows ACLs itself (that needs Win32 calls outside
//!   `std`), so when you pass a shared directory to `new_in`, the new
//!   entry is as private as that directory makes it.
//! * **Prefixes stay inside the parent.** `with_prefix` rejects a
//!   prefix containing a path separator (`/` or `\`), `..`, a NUL
//!   byte, or (on Windows) a `:`, with
//!   [`io::ErrorKind::InvalidInput`].
//!
//! ## Feature flags
//!
//! * `mod-rand` (off by default): draw the name fragment directly from
//!   `mod_rand::tier3::random_base32`, the OS random source, on every
//!   call. If that call fails, the default `std` path is used instead.
//!   The alphabet is Crockford base32 on both paths, so any caller
//!   pattern-matching on the basename keeps working when the feature
//!   is toggled. Applies to both [`TempDir`] and [`NamedTempFile`].
//!
//! To enable in `Cargo.toml`:
//!
//! ```toml
//! mod-tempdir = { version = "1", features = ["mod-rand"] }
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
#[cfg(test)]
mod security_tests;

pub use cleanup::cleanup_orphans;
pub use named_file::{NamedTempFile, PersistAtomicError};

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary directory that auto-deletes when dropped.
///
/// # Example
///
/// ```
/// use mod_tempdir::TempDir;
///
/// let dir = TempDir::new()?;
/// let file_path = dir.path().join("test.txt");
/// std::fs::write(&file_path, b"hello")?;
/// // dir and its contents are deleted at end of scope
/// # Ok::<(), std::io::Error>(())
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
    /// 12-character Crockford base32 string drawn from the OS random
    /// source. The directory is created exclusively, with mode `0700`
    /// on Unix; if the name is already taken, a fresh one is drawn.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] if the directory cannot be
    /// created, or [`io::ErrorKind::AlreadyExists`] if 64 fresh names
    /// in a row were all taken.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::new()?;
    /// assert!(dir.path().is_dir());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new() -> io::Result<Self> {
        Self::new_in(std::env::temp_dir())
    }

    /// Create a new temporary directory inside `dir`.
    ///
    /// Same naming, permissions and cleanup as [`TempDir::new`], but
    /// under a parent directory of your choice, which must already
    /// exist. [`cleanup_orphans`] only sweeps the OS temp location, so
    /// entries left behind under other parents are not swept.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] if the directory cannot be
    /// created (for example, `dir` does not exist), or
    /// [`io::ErrorKind::AlreadyExists`] if 64 fresh names in a row were
    /// all taken.
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::TempDir;
    ///
    /// let parent = TempDir::new()?;
    /// let child = TempDir::new_in(parent.path())?;
    /// assert_eq!(child.path().parent(), Some(parent.path()));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new_in(dir: impl AsRef<Path>) -> io::Result<Self> {
        let pid = std::process::id();
        let path = create_unique(
            dir.as_ref(),
            |name| format!(".tmp-{pid}-{name}"),
            create_private_dir,
        )?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Create a new temporary directory with the given prefix.
    ///
    /// The final basename is `{prefix}-{12-char-name}`, created in the
    /// system's temp location with the same exclusive creation and
    /// permissions as [`TempDir::new`].
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] if `prefix` contains a
    /// path separator (`/` or `\`), `..`, a NUL byte, or (on Windows) a
    /// `:`. Otherwise returns the same errors as [`TempDir::new`].
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::TempDir;
    ///
    /// let dir = TempDir::with_prefix("my-app")?;
    /// assert!(dir
    ///     .path()
    ///     .file_name()
    ///     .unwrap()
    ///     .to_string_lossy()
    ///     .starts_with("my-app-"));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        Self::with_prefix_in(std::env::temp_dir(), prefix)
    }

    /// Create a new temporary directory with the given prefix inside
    /// `dir`.
    ///
    /// Combines [`TempDir::new_in`] and [`TempDir::with_prefix`].
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::InvalidInput`] for a prefix that
    /// [`TempDir::with_prefix`] rejects. Otherwise returns the same
    /// errors as [`TempDir::new_in`].
    ///
    /// # Example
    ///
    /// ```
    /// use mod_tempdir::TempDir;
    ///
    /// let parent = TempDir::new()?;
    /// let child = TempDir::with_prefix_in(parent.path(), "work")?;
    /// assert_eq!(child.path().parent(), Some(parent.path()));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn with_prefix_in(dir: impl AsRef<Path>, prefix: &str) -> io::Result<Self> {
        validate_prefix(prefix)?;
        let path = create_unique(
            dir.as_ref(),
            |name| format!("{prefix}-{name}"),
            create_private_dir,
        )?;
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

/// How many fresh names to try before giving up on a taken path.
const MAX_ATTEMPTS: u32 = 64;

/// Length of the random fragment in every generated basename.
const NAME_LEN: usize = 12;

/// Crockford base32: `0-9A-Z` minus `I`, `L`, `O`, `U`.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Reject a prefix that could move the entry out of its parent
/// directory or name something other than a single path component.
pub(crate) fn validate_prefix(prefix: &str) -> io::Result<()> {
    let bad = prefix.contains('/')
        || prefix.contains('\\')
        || prefix.contains("..")
        || prefix.contains('\0')
        || (cfg!(windows) && prefix.contains(':'));
    if bad {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "temporary name prefix must be a plain name: no path separators, `..`, `:` or NUL",
        ));
    }
    Ok(())
}

/// Create a new entry at `dir/basename(random)`, retrying with a
/// fresh name whenever `create` reports that the path is taken.
pub(crate) fn create_unique(
    dir: &Path,
    basename: impl Fn(&str) -> String,
    create: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    for _ in 0..MAX_ATTEMPTS {
        let path = dir.join(basename(&unique_name(NAME_LEN)));
        match create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not find an unused temporary name",
    ))
}

/// Create one directory exclusively, owner-only on Unix.
fn create_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::DirBuilder::new().create(path)
    }
}

#[cfg(test)]
thread_local! {
    pub(crate) static FORCED_NAMES: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn unique_name(len: usize) -> String {
    #[cfg(test)]
    if let Some(name) = FORCED_NAMES.with(|f| f.borrow_mut().pop()) {
        return name;
    }
    unique_name_impl(len)
}

#[cfg(feature = "mod-rand")]
fn unique_name_impl(len: usize) -> String {
    mod_rand::tier3::random_base32(len).unwrap_or_else(|_| os_keyed_name(len))
}

#[cfg(not(feature = "mod-rand"))]
#[inline]
fn unique_name_impl(len: usize) -> String {
    os_keyed_name(len)
}

/// A Crockford base32 name whose bits come from `std`'s
/// OS-seeded hasher.
///
/// `RandomState::new` hands out SipHash keys that `std` reads from the
/// operating system's random source, so every word below is a keyed
/// hash under a secret 128-bit key. The counter, PID and clock inputs
/// only keep successive words distinct; they are not the source of
/// the unpredictability.
fn os_keyed_name(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut word = os_keyed_u64();
        // 12 characters of 5 bits each use 60 of the 64 bits.
        for _ in 0..12 {
            if out.len() == len {
                break;
            }
            out.push(ALPHABET[(word & 31) as usize] as char);
            word >>= 5;
        }
    }
    out
}

fn os_keyed_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    hasher.write_u128(nanos);
    hasher.finish()
}

/// Internal test hook. **Not part of the stable public API.** This
/// symbol exists only to let this crate's integration tests exercise
/// the name generator without paying for a filesystem syscall per
/// sample. External code must not call it; it may be renamed or
/// removed in any release, including a patch.
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

    #[test]
    fn os_keyed_names_have_the_requested_length_and_alphabet() {
        for len in [0, 1, 12, 13, 25, 64] {
            let name = os_keyed_name(len);
            assert_eq!(name.len(), len);
            assert!(name.bytes().all(|b| ALPHABET.contains(&b)));
        }
    }

    #[test]
    fn prefix_validation() {
        for ok in ["", "app", "my-app.v2", "a b", "dot.name"] {
            assert!(validate_prefix(ok).is_ok(), "{ok:?}");
        }
        for bad in ["..", "../x", "a/b", "a\\b", "x\0y", "a..b"] {
            let e = validate_prefix(bad).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{bad:?}");
        }
    }
}
