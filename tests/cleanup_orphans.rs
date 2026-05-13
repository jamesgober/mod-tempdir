// Integration tests for `cleanup_orphans`. Tests are isolated from
// each other (and from other test binaries running in parallel) by
// embedding a per-test bogus PID and a per-test "tag" character in
// the orphan basenames, so each test only ever asserts on entries
// it created.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use mod_tempdir::cleanup_orphans;

/// Build a basename in the crate's default directory pattern.
/// `pid` is the PID encoded into the basename; `tag` distinguishes
/// one test's orphans from another's.
fn fake_dir_basename(pid: u32, tag: &str) -> String {
    // 12-char suffix using the same alphabet the crate emits.
    format!(".tmp-{pid}-{tag:0>12}")
}

/// Build a basename in the crate's default file pattern.
fn fake_file_basename(pid: u32, tag: &str) -> String {
    format!(".tmpfile-{pid}-{tag:0>12}")
}

/// Cross-platform helper: set the mtime of a directory.
///
/// On Windows, opening a directory handle requires
/// `FILE_FLAG_BACKUP_SEMANTICS` (`0x02000000`); a plain `File::open`
/// returns `Access denied`. On Unix, directories can be opened
/// read-only and `set_modified` works against the resulting handle.
fn set_dir_modified(path: &Path, when: SystemTime) {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .expect("open dir for set_modified (windows)")
    };
    #[cfg(not(windows))]
    let handle = File::open(path).expect("open dir for set_modified (unix)");

    handle
        .set_modified(when)
        .expect("set_modified failed on directory handle");
}

/// Cross-platform helper: set the mtime of a regular file.
fn set_file_modified(path: &Path, when: SystemTime) {
    OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open file for set_modified")
        .set_modified(when)
        .expect("set_modified failed on file handle");
}

/// Best-effort purge of a stale entry left by a previous failed run.
/// Try both "file" and "dir" removal so we don't care what type
/// the leftover was.
fn purge_stale(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
    let _ = std::fs::remove_file(path);
}

/// Create a fake orphan directory with a backdated mtime. If a
/// previous failed run left an entry at the same path, it is purged
/// first so this run starts from a known state.
fn create_orphan_dir(pid: u32, tag: &str, age: Duration) -> PathBuf {
    let path = std::env::temp_dir().join(fake_dir_basename(pid, tag));
    purge_stale(&path);
    std::fs::create_dir(&path).expect("create_dir failed for fake orphan dir");
    set_dir_modified(&path, SystemTime::now() - age);
    path
}

/// Create a fake orphan file with a backdated mtime. Same fresh-
/// start guard as `create_orphan_dir`.
fn create_orphan_file(pid: u32, tag: &str, age: Duration) -> PathBuf {
    let path = std::env::temp_dir().join(fake_file_basename(pid, tag));
    purge_stale(&path);
    File::create(&path).expect("File::create failed for fake orphan file");
    set_file_modified(&path, SystemTime::now() - age);
    path
}

/// Best-effort post-test cleanup: makes sure nothing we created
/// outlives a failed test.
fn force_remove(path: &PathBuf) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn removes_old_orphan_directory() {
    // Bogus PID well above any realistic live PID on the host.
    let pid = u32::MAX;
    let tag = "ZRMVDIRAAAAA";
    let age = Duration::from_secs(10 * 3600); // 10 hours old

    let path = create_orphan_dir(pid, tag, age);
    assert!(path.exists(), "fake orphan dir was not created");

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(
        !path.exists(),
        "10-hour-old orphan dir survived cleanup_orphans(1): {path:?}"
    );
    force_remove(&path);
}

#[test]
fn removes_old_orphan_file() {
    let pid = u32::MAX - 1;
    let tag = "ZRMVFILEAAAA";
    let age = Duration::from_secs(10 * 3600);

    let path = create_orphan_file(pid, tag, age);
    assert!(path.exists(), "fake orphan file was not created");

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(
        !path.exists(),
        "10-hour-old orphan file survived cleanup_orphans(1): {path:?}"
    );
    force_remove(&path);
}

#[test]
fn keeps_recent_entry() {
    // Recent (zero-age) orphan. Even with a dead PID, the age
    // threshold of 1 hour must protect it.
    let pid = u32::MAX - 2;
    let tag = "ZKPRECENTAAA";
    let path = create_orphan_dir(pid, tag, Duration::from_secs(0));
    assert!(path.exists());

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(
        path.exists(),
        "recent entry was removed despite age threshold: {path:?}"
    );
    force_remove(&path);
}

#[test]
fn ignores_custom_prefix_entries() {
    // Caller-supplied prefix (no `.tmp-` / `.tmpfile-` lead) is
    // outside our namespace; cleanup_orphans must not touch it,
    // regardless of age.
    let basename = "user-fixture-99999-ZKPCSTOMAAAA";
    let path = std::env::temp_dir().join(basename);
    purge_stale(&path);
    std::fs::create_dir(&path).expect("create_dir failed");
    set_dir_modified(&path, SystemTime::now() - Duration::from_secs(48 * 3600));

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(path.exists(), "custom-prefix entry was removed: {path:?}");
    force_remove(&path);
}

#[test]
fn ignores_legacy_format_entries() {
    // Legacy v0.9.0 / v0.9.1 entry: `.tmp-{name}` with NO PID
    // segment. Not eligible for cleanup; must be left alone.
    let basename = ".tmp-ZKPLEGACYAAAA"; // no `{pid}-` after the prefix
    let path = std::env::temp_dir().join(basename);
    purge_stale(&path);
    std::fs::create_dir(&path).expect("create_dir failed");
    set_dir_modified(&path, SystemTime::now() - Duration::from_secs(48 * 3600));

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(path.exists(), "legacy-format entry was removed: {path:?}");
    force_remove(&path);
}

#[cfg(target_os = "linux")]
#[test]
fn keeps_entry_owned_by_live_process_on_linux() {
    // On Linux the PID-liveness check uses /proc. Encode the
    // current process's PID into the orphan basename; cleanup must
    // refuse to remove it, even when it satisfies the age criterion.
    let pid = std::process::id();
    let tag = "ZKPLIVEAAAAA";
    let age = Duration::from_secs(10 * 3600);

    let path = create_orphan_dir(pid, tag, age);
    assert!(path.exists());

    let _ = cleanup_orphans(1).expect("cleanup_orphans failed");

    assert!(
        path.exists(),
        "live-process entry was removed by cleanup_orphans on Linux: {path:?}"
    );
    force_remove(&path);
}
