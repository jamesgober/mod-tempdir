// Integration tests for `NamedTempFile::persist_atomic`.
//
// All tests use the OS temp dir as the target location so the rename
// is guaranteed to be on the same filesystem as the source. Each
// test uses a unique target basename so concurrent runs don't
// collide.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;

use mod_tempdir::NamedTempFile;

fn unique_target(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("persist-atomic-{tag}-{pid}-{nanos}"))
}

fn write_payload(path: &std::path::Path, data: &[u8]) {
    let mut h = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open temp file for write");
    h.write_all(data).expect("write payload");
}

#[test]
fn moves_temp_file_to_target_and_preserves_content() {
    let f = NamedTempFile::new().expect("NamedTempFile::new failed");
    let source = f.path().to_path_buf();
    write_payload(&source, b"persist_atomic preserves bytes");

    let target = unique_target("preserve");
    // Defensive: in case a previous failed run left it.
    let _ = std::fs::remove_file(&target);

    let landed = f
        .persist_atomic(&target)
        .expect("persist_atomic failed on same-filesystem move");

    assert_eq!(landed, target, "persist_atomic returned wrong path");
    assert!(
        target.is_file(),
        "target does not exist after persist_atomic"
    );
    assert!(
        !source.exists(),
        "source temp file still exists after persist_atomic"
    );

    let contents = std::fs::read(&target).expect("read target");
    assert_eq!(contents, b"persist_atomic preserves bytes");

    let _ = std::fs::remove_file(&target);
}

#[test]
fn replaces_existing_target() {
    let target = unique_target("replace");
    // Pre-create the target with stale content.
    std::fs::write(&target, b"stale").expect("seed target");

    let f = NamedTempFile::new().expect("NamedTempFile::new failed");
    write_payload(f.path(), b"fresh");

    let landed = f
        .persist_atomic(&target)
        .expect("persist_atomic should replace existing target");

    let contents = std::fs::read(&landed).expect("read target");
    assert_eq!(
        contents, b"fresh",
        "target was not replaced by persist_atomic"
    );

    let _ = std::fs::remove_file(&landed);
}

#[test]
fn errors_when_target_parent_missing_and_preserves_source() {
    // A target under a non-existent directory cannot succeed.
    let f = NamedTempFile::new().expect("NamedTempFile::new failed");
    let source = f.path().to_path_buf();

    let target = std::env::temp_dir()
        .join("nonexistent-parent-for-persist-atomic")
        .join("file.bin");

    let err = f
        .persist_atomic(&target)
        .expect_err("persist_atomic should error when target parent missing");

    // The data-integrity contract: on failure, the temp file is
    // preserved on disk AND the original NamedTempFile is returned
    // intact inside the error so the caller can retry or fall back.
    assert!(
        source.exists(),
        "source temp file should survive a failed persist_atomic"
    );
    assert_eq!(
        err.file.path(),
        source.as_path(),
        "recovered NamedTempFile should point at the original temp path"
    );
    // Drop on `err.file` will clean up the temp at end of scope.
}

#[test]
fn original_path_no_longer_exists_after_success() {
    // The signature consumes `self`, so this is mostly a clarity
    // check: after a successful persist_atomic, nothing remains at
    // the original temp path. Useful as a guard against any future
    // change that might leave the temp behind by mistake.
    let f = NamedTempFile::new().expect("NamedTempFile::new failed");
    let source = f.path().to_path_buf();

    let target = unique_target("gone");
    let _ = std::fs::remove_file(&target);

    let landed = f.persist_atomic(&target).expect("persist_atomic failed");

    assert!(!source.exists(), "source temp file should be gone");
    assert!(target.is_file(), "target should exist");
    assert_eq!(landed, target);

    let _ = std::fs::remove_file(&target);
}
