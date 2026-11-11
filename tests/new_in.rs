// `new_in` / `with_prefix_in`: create entries under a caller-chosen
// parent, so `persist_atomic` can rename within one filesystem.

use std::io::ErrorKind;

use mod_tempdir::{NamedTempFile, TempDir};

#[test]
fn temp_dir_new_in_creates_under_the_parent() {
    let parent = TempDir::new().unwrap();
    let child = TempDir::new_in(parent.path()).unwrap();
    assert_eq!(child.path().parent(), Some(parent.path()));
    assert!(child.path().is_dir());
    let name = child.path().file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(&format!(".tmp-{}-", std::process::id())));

    let path = child.path().to_path_buf();
    drop(child);
    assert!(!path.exists(), "child must be removed on drop");
}

#[test]
fn named_file_new_in_creates_under_the_parent() {
    let parent = TempDir::new().unwrap();
    let f = NamedTempFile::new_in(parent.path()).unwrap();
    assert_eq!(f.path().parent(), Some(parent.path()));
    assert!(f.path().is_file());
    let name = f.path().file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(&format!(".tmpfile-{}-", std::process::id())));
}

#[test]
fn with_prefix_in_uses_parent_and_prefix() {
    let parent = TempDir::new().unwrap();
    let d = TempDir::with_prefix_in(parent.path(), "work").unwrap();
    let f = NamedTempFile::with_prefix_in(parent.path(), "upload").unwrap();
    assert_eq!(d.path().parent(), Some(parent.path()));
    assert_eq!(f.path().parent(), Some(parent.path()));
    assert!(d
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("work-"));
    assert!(f
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("upload-"));
}

#[test]
fn with_prefix_in_rejects_escaping_prefixes() {
    let parent = TempDir::new().unwrap();
    let e = TempDir::with_prefix_in(parent.path(), "../up").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidInput);
    let e = NamedTempFile::with_prefix_in(parent.path(), "../up").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidInput);
}

#[test]
fn new_in_missing_parent_is_an_error() {
    let parent = TempDir::new().unwrap();
    let missing = parent.path().join("does-not-exist");
    assert_eq!(
        TempDir::new_in(&missing).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        NamedTempFile::new_in(&missing).unwrap_err().kind(),
        ErrorKind::NotFound
    );
}

#[test]
fn persist_atomic_next_to_the_target() {
    let target_dir = TempDir::new().unwrap();
    let target = target_dir.path().join("config.toml");
    std::fs::write(&target, b"old").unwrap();

    let tmp = NamedTempFile::new_in(target_dir.path()).unwrap();
    std::fs::write(tmp.path(), b"new").unwrap();
    let landed = tmp.persist_atomic(&target).unwrap();

    assert_eq!(landed, target);
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    // Only the target remains in the directory.
    let entries: Vec<_> = std::fs::read_dir(target_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(entries.len(), 1);
}

#[cfg(unix)]
#[test]
fn new_in_entries_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let parent = TempDir::new().unwrap();
    let d = TempDir::new_in(parent.path()).unwrap();
    let f = NamedTempFile::new_in(parent.path()).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(d.path()), 0o700);
    assert_eq!(mode(f.path()), 0o600);
}
