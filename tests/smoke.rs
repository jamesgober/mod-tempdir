use mod_tempdir::TempDir;

#[test]
fn smoke_creates_directory() {
    let dir = TempDir::new().unwrap();
    assert!(dir.path().exists());
    assert!(dir.path().is_dir());
}

#[test]
fn smoke_auto_cleanup_on_drop() {
    let path = {
        let dir = TempDir::new().unwrap();
        dir.path().to_path_buf()
    };
    assert!(!path.exists());
}

#[test]
fn smoke_persist_keeps_dir() {
    let dir = TempDir::new().unwrap();
    let path = dir.persist();
    assert!(path.exists());
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn smoke_with_prefix() {
    let dir = TempDir::with_prefix("test-xyz").unwrap();
    let name = dir.path().file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("test-xyz-"));
}

#[test]
fn smoke_unique_directories() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    assert_ne!(a.path(), b.path());
}

#[test]
fn smoke_write_and_read_inside_tempdir() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, b"hello world").unwrap();
    let contents = std::fs::read_to_string(&file).unwrap();
    assert_eq!(contents, "hello world");
}
