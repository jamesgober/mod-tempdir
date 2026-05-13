//! Orphan-entry cleanup. See [`cleanup_orphans`].
//!
//! Companion module to [`crate::TempDir`] and [`crate::NamedTempFile`].
//! Provides a single free function that scans the OS temp dir for
//! default-prefix entries this crate could have created and removes
//! those whose owning processes are no longer alive.

use std::io;
use std::time::{Duration, SystemTime};

/// Sweep the OS temp directory for default-prefix entries this crate
/// could have created and remove those that look orphaned.
///
/// An entry is removed when **both** of these hold:
///
/// 1. **The owning process is not alive.** Each default-prefix entry
///    carries the originating process's PID in its basename
///    (`.tmp-{pid}-{name}` for [`crate::TempDir`],
///    `.tmpfile-{pid}-{name}` for [`crate::NamedTempFile`]). On Linux
///    liveness is checked via the presence of `/proc/{pid}`. On
///    macOS and Windows the liveness check is treated as "process is
///    dead" (since cross-platform process introspection without
///    platform deps is not available); the age check carries the
///    safety burden alone on those platforms.
/// 2. **The entry's mtime is at least `max_age_hours` old.** This is
///    the load-bearing safety guard on macOS and Windows: pick a
///    threshold larger than any process you expect to legitimately
///    hold temp paths.
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
    let temp_dir = std::env::temp_dir();
    let max_age = Duration::from_secs(max_age_hours.saturating_mul(3600));
    let now = SystemTime::now();

    let mut removed = 0_usize;

    for entry_result in std::fs::read_dir(&temp_dir)? {
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

    let metadata = match entry.metadata() {
        Ok(m) => m,
        Err(_) => return false,
    };
    let mtime = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return false,
    };
    let age = match now.duration_since(mtime) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if age < max_age {
        return false;
    }

    let path = entry.path();
    let result = if is_dir_pattern {
        std::fs::remove_dir_all(&path)
    } else {
        std::fs::remove_file(&path)
    };
    result.is_ok()
}

/// Returns `true` if process `pid` is alive on this host.
///
/// Linux: checks `/proc/{pid}` for existence.
///
/// macOS, Windows: returns `false` unconditionally. Cross-platform
/// process introspection without `libc` / `windows-sys` is not
/// available; the age check is the sole safety gate on those
/// platforms. Returning `false` here lets the AND condition in
/// [`cleanup_orphans`] degrade to age-only.
#[cfg(target_os = "linux")]
fn pid_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn pid_alive(_pid: u32) -> bool {
    // Cross-platform process introspection without deps is not
    // available; see [`cleanup_orphans`] rustdoc. Returning false
    // lets the AND condition degrade to age-only on this platform.
    false
}
