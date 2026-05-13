// Happy-path smoke tests for NamedTempFile. Parallel structure to
// tests/smoke.rs (which covers TempDir).

use std::io::Write;

use mod_tempdir::NamedTempFile;

#[test]
fn smoke_creates_file() {
    let f = NamedTempFile::new().unwrap();
    assert!(f.path().exists());
    assert!(f.path().is_file());
}

#[test]
fn smoke_auto_cleanup_on_drop() {
    let path = {
        let f = NamedTempFile::new().unwrap();
        f.path().to_path_buf()
    };
    assert!(
        !path.exists(),
        "file should be cleaned up after NamedTempFile drops"
    );
}

#[test]
fn smoke_persist_keeps_file() {
    let f = NamedTempFile::new().unwrap();
    let path = f.persist();
    assert!(path.exists());
    // persist disabled cleanup; remove manually so the test is
    // self-contained.
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn smoke_with_prefix_uses_prefix() {
    let f = NamedTempFile::with_prefix("my-fixture").unwrap();
    let name = f.path().file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("my-fixture-"), "got: {name}");
}

#[test]
fn smoke_default_uses_tmpfile_prefix() {
    // Default basename pattern must be `.tmpfile-{name}` so it is
    // visually distinct from TempDir's `.tmp-{name}`.
    let f = NamedTempFile::new().unwrap();
    let name = f.path().file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(".tmpfile-"), "got: {name}");
}

#[test]
fn smoke_two_files_unique() {
    let a = NamedTempFile::new().unwrap();
    let b = NamedTempFile::new().unwrap();
    assert_ne!(a.path(), b.path());
}

#[test]
fn smoke_cleanup_on_drop_defaults_true() {
    let f = NamedTempFile::new().unwrap();
    assert!(f.cleanup_on_drop());
}

#[test]
fn smoke_write_and_read_via_open_options() {
    // The returned path must be usable for writes via standard
    // OpenOptions. This verifies the file was created with sensible
    // default mode bits on every platform.
    let f = NamedTempFile::new().unwrap();
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .open(f.path())
        .unwrap();
    handle.write_all(b"hello world").unwrap();
    drop(handle);

    let contents = std::fs::read_to_string(f.path()).unwrap();
    assert_eq!(contents, "hello world");
}
