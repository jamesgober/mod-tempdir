//! Orphan-entry cleanup. See [`cleanup_orphans`] and
//! [`cleanup_orphans_in`].
//!
//! Companion module to [`crate::TempDir`] and [`crate::NamedTempFile`].
//! Scans a directory for default-prefix entries this crate could have
//! created and removes those whose owning processes are no longer
//! alive.

use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::sys::pid_alive;

/// Sweep the OS temp directory for default-prefix entries this crate
/// could have created and remove those that look orphaned.
///
/// Same as [`cleanup_orphans_in`] with the directory returned by
/// [`std::env::temp_dir`].
///
/// An entry is removed when **both** of these hold:
///
/// 1. **The owning process is not alive.** Each default-prefix entry
///    carries the originating process's PID in its basename
///    (`.tmp-{pid}-{name}` for [`crate::TempDir`],
///    `.tmpfile-{pid}-{name}` for [`crate::NamedTempFile`]). Entries
///    carrying the calling process's own PID are never removed. Other
///    PIDs are checked with the platform's own process table:
///    `/proc/{pid}` on Linux, `kill(pid, 0)` on other Unix systems,
///    and `OpenProcess` plus `GetExitCodeProcess` on Windows. When the
///    platform cannot answer clearly (for example the process belongs
///    to another user and the query is denied), the process counts as
///    alive and its entries are kept. On platforms with no process
///    query at all, nothing is removed.
/// 2. **The entry is at least `max_age_hours` old.** For a file the
///    age comes from its mtime. For a directory it comes from the
///    newest mtime of the directory and its direct children, so a
///    working directory whose files are still being written is not
///    treated as old.
///
/// PIDs are reused by the operating system, so an orphan whose PID now
/// belongs to an unrelated live process is kept until that process
/// exits. That is the safe direction: a live entry is never removed
/// because of a PID check.
///
/// Entries that do not match the crate's default prefix patterns,
/// including any caller-supplied `with_prefix(...)` paths, are never
/// touched. Likewise, legacy entries from `0.9.0` and `0.9.1` that
/// predate the PID-in-basename format are ignored: they have no
/// PID segment to parse and are not eligible for cleanup.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] only if reading the OS temp
/// directory itself fails. Per-entry failures (permission denied,
/// entry disappeared between scan and removal, Windows handle still
/// open, etc.) are intentionally silent and not counted in the
/// return value. This matches the silent-Drop ethos for the rest of
/// the crate.
///
/// # Returns
///
/// The number of entries successfully removed.
///
/// # Example
///
/// ```no_run
/// use mod_tempdir::cleanup_orphans;
///
/// // At program startup, sweep anything older than 24 hours left
/// // behind by crashed earlier runs.
/// let removed = cleanup_orphans(24).unwrap_or(0);
/// eprintln!("cleanup_orphans removed {removed} orphaned entries");
/// ```
pub fn cleanup_orphans(max_age_hours: u64) -> io::Result<usize> {
    cleanup_orphans_in(std::env::temp_dir(), max_age_hours)
}

/// Sweep `dir` for default-prefix entries this crate could have
/// created and remove those that look orphaned.
///
/// Use this for entries created with [`crate::TempDir::new_in`] or
/// [`crate::NamedTempFile::new_in`], which [`cleanup_orphans`] does
/// not see, and in tests, so a sweep never touches the shared OS temp
/// directory. The rules for what is removed are the same as for
/// [`cleanup_orphans`]. Only direct children of `dir` are examined.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] only if reading `dir` itself
/// fails. Per-entry failures are silent and not counted.
///
/// # Returns
///
/// The number of entries successfully removed.
///
/// # Example
///
/// ```
/// use mod_tempdir::{cleanup_orphans_in, TempDir};
///
/// let scratch = TempDir::new()?;
/// // Nothing in a fresh directory is an orphan.
/// assert_eq!(cleanup_orphans_in(scratch.path(), 24)?, 0);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn cleanup_orphans_in(dir: impl AsRef<Path>, max_age_hours: u64) -> io::Result<usize> {
    let max_age = Duration::from_secs(max_age_hours.saturating_mul(3600));
    let now = SystemTime::now();

    let mut removed = 0_usize;

    for entry_result in std::fs::read_dir(dir.as_ref())? {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if try_remove_one(&entry, now, max_age) {
            removed += 1;
        }
    }

    Ok(removed)
}

/// Returns `true` iff `entry` was successfully removed.
///
/// All non-fatal conditions (wrong prefix, malformed PID, live
/// process, too recent, removal error) return `false` quietly.
fn try_remove_one(entry: &std::fs::DirEntry, now: SystemTime, max_age: Duration) -> bool {
    let name = entry.file_name();
    let name_str = match name.to_str() {
        Some(s) => s,
        None => return false,
    };

    let (is_dir_pattern, after_prefix) = if let Some(rest) = name_str.strip_prefix(".tmp-") {
        (true, rest)
    } else if let Some(rest) = name_str.strip_prefix(".tmpfile-") {
        (false, rest)
    } else {
        return false;
    };

    // Parse `{digits}-`. Legacy entries (no PID segment) fall out
    // here and are left alone.
    let (pid_str, _name12) = match after_prefix.split_once('-') {
        Some(p) => p,
        None => return false,
    };
    let pid = match pid_str.parse::<u32>() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if pid_alive(pid) {
        return false;
    }

    let path = entry.path();
    // `symlink_metadata` so a link planted under a default name is
    // judged (and removed) as the link itself.
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let mut newest = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };
    if is_dir_pattern && metadata.is_dir() {
        newest = newest_child_mtime(&path, newest);
    }
    let age = match now.duration_since(newest) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if age < max_age {
        return false;
    }

    let result = if is_dir_pattern {
        std::fs::remove_dir_all(&path)
    } else {
        std::fs::remove_file(&path)
    };
    result.is_ok()
}

/// Newest mtime among `dir`'s direct children, or `start` if none is
/// newer (or the directory cannot be read).
fn newest_child_mtime(dir: &Path, start: SystemTime) -> SystemTime {
    let mut newest = start;
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return newest,
    };
    for entry in entries.flatten() {
        if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
            if t > newest {
                newest = t;
            }
        }
    }
    newest
}
