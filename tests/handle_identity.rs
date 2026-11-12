// Regression tests: `NamedTempFile` keeps the handle from its
// exclusive create, writes go through it, and `persist_atomic` and
// `reopen` refuse to act on an entry that replaced the original file.
//
// The "other party" that swaps the entry is simulated in-process, in
// a private directory, so nothing outside the test's own `TempDir` is
// touched.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use mod_tempdir::{NamedTempFile, TempDir};

/// Move the original entry aside and put a different file at its name.
/// Returns where the original went.
fn swap_in_other_file(tmp: &NamedTempFile, content: &[u8]) -> PathBuf {
    let moved = tmp.path().with_extension("moved");
    std::fs::rename(tmp.path(), &moved).expect("move original aside");
    std::fs::write(tmp.path(), content).expect("plant replacement");
    moved
}

#[test]
fn writes_through_the_handle_reach_the_created_file() {
    let dir = TempDir::new().unwrap();
    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();

    tmp.write_all(b"via write impl").unwrap();
    assert_eq!(std::fs::read(tmp.path()).unwrap(), b"via write impl");

    tmp.as_file_mut().set_len(0).unwrap();
    tmp.seek(SeekFrom::Start(0)).unwrap();
    tmp.as_file().write_all(b"via as_file").unwrap();
    tmp.seek(SeekFrom::Start(0)).unwrap();
    let mut back = String::new();
    tmp.read_to_string(&mut back).unwrap();
    assert_eq!(back, "via as_file");
}

#[test]
fn handle_write_does_not_follow_a_hard_link_swap() {
    let dir = TempDir::new().unwrap();
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, b"victim data").unwrap();

    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();
    let moved = tmp.path().with_extension("moved");
    std::fs::rename(tmp.path(), &moved).unwrap();
    std::fs::hard_link(&victim, tmp.path()).unwrap();

    tmp.write_all(b"caller payload").unwrap();
    tmp.flush().unwrap();

    assert_eq!(std::fs::read(&victim).unwrap(), b"victim data");
    assert_eq!(std::fs::read(&moved).unwrap(), b"caller payload");
}

#[test]
fn persist_atomic_refuses_a_replaced_entry() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("config.toml");

    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();
    tmp.write_all(b"caller config").unwrap();
    let _moved = swap_in_other_file(&tmp, b"attacker config");

    let err = tmp
        .persist_atomic(&target)
        .expect_err("persist_atomic published a replaced entry");
    assert_eq!(err.error.kind(), std::io::ErrorKind::Other);
    assert!(!target.exists(), "target was created from a replaced entry");
    // The original value comes back intact for retry or cleanup.
    assert!(err.file.cleanup_on_drop());
}

#[test]
fn reopen_refuses_a_replaced_entry() {
    let dir = TempDir::new().unwrap();
    let tmp = NamedTempFile::new_in(dir.path()).unwrap();
    let _moved = swap_in_other_file(&tmp, b"other");

    let err = tmp.reopen().expect_err("reopen opened a replaced entry");
    assert_eq!(err.kind(), std::io::ErrorKind::Other);
}

#[test]
fn reopen_gives_a_second_handle_to_the_same_file() {
    let dir = TempDir::new().unwrap();
    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();
    tmp.write_all(b"shared").unwrap();

    let mut other = tmp.reopen().unwrap();
    let mut text = String::new();
    other.read_to_string(&mut text).unwrap();
    assert_eq!(text, "shared");
}

#[test]
fn persist_atomic_publishes_what_was_written_through_the_handle() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.bin");

    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();
    tmp.write_all(b"final bytes").unwrap();
    let source = tmp.path().to_path_buf();
    let landed = tmp.persist_atomic(&target).unwrap();

    assert_eq!(landed, target);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"final bytes");
}

#[test]
fn path_writes_still_work_and_persist() {
    // Existing callers that write by path in a private directory keep
    // working: the entry is still the same file.
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("by-path.txt");

    let tmp = NamedTempFile::new_in(dir.path()).unwrap();
    std::fs::write(tmp.path(), b"by path").unwrap();
    tmp.persist_atomic(&target).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"by path");
}

#[test]
fn drop_removes_the_file_while_the_handle_is_held() {
    let dir = TempDir::new().unwrap();
    let path = {
        let tmp = NamedTempFile::new_in(dir.path()).unwrap();
        tmp.path().to_path_buf()
    };
    assert!(!Path::new(&path).exists());
}

#[cfg(unix)]
#[test]
fn persist_atomic_refuses_a_symlink_swap() {
    let dir = TempDir::new().unwrap();
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, b"victim data").unwrap();
    let target = dir.path().join("config.toml");

    let tmp = NamedTempFile::new_in(dir.path()).unwrap();
    let moved = tmp.path().with_extension("moved");
    std::fs::rename(tmp.path(), moved).unwrap();
    std::os::unix::fs::symlink(&victim, tmp.path()).unwrap();

    let err = tmp
        .persist_atomic(&target)
        .expect_err("published a symlink");
    assert_eq!(err.error.kind(), std::io::ErrorKind::Other);
    assert!(!target.exists());
}
