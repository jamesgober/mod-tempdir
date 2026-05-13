// Spec requirement from v0.9.1: TempDir and NamedTempFile must
// coexist in the same scope without naming collisions. The
// distinct default basename prefixes (`.tmp-` for dirs, `.tmpfile-`
// for files) make this a visual property as well as a functional
// one.

use mod_tempdir::{NamedTempFile, TempDir};

#[test]
fn tempdir_and_named_file_coexist() {
    let dir = TempDir::new().expect("TempDir::new failed");
    let file = NamedTempFile::new().expect("NamedTempFile::new failed");

    assert!(dir.path().is_dir(), "TempDir path is not a directory");
    assert!(
        file.path().is_file(),
        "NamedTempFile path is not a regular file"
    );
    assert_ne!(
        dir.path(),
        file.path(),
        "TempDir and NamedTempFile produced the same path"
    );
}

#[test]
fn default_basenames_are_distinguishable() {
    let dir = TempDir::new().expect("TempDir::new failed");
    let file = NamedTempFile::new().expect("NamedTempFile::new failed");

    let dir_name = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let file_name = file
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    assert!(
        dir_name.starts_with(".tmp-"),
        "TempDir default basename should start with '.tmp-', got: {dir_name}"
    );
    assert!(
        file_name.starts_with(".tmpfile-"),
        "NamedTempFile default basename should start with '.tmpfile-', got: {file_name}"
    );
    // Cross-check: the two prefixes do not overlap. `.tmpfile-` has
    // an `f` at index 4 where `.tmp-` has `-`, so neither prefix is
    // a prefix of the other.
    assert!(!dir_name.starts_with(".tmpfile-"));
    assert!(!file_name.starts_with(".tmp-"));
}
