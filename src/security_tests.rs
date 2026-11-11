//! Regression tests for exclusive creation and collision retry.
//!
//! These tests force the next generated name through the test-only
//! `FORCED_NAMES` hook, plant something at that path first, and check
//! that creation neither touches the planted entry nor fails: it must
//! move on to a fresh name.

use std::path::PathBuf;

use crate::{NamedTempFile, TempDir, FORCED_NAMES};

fn force_next_name(name: &str) {
    FORCED_NAMES.with(|f| f.borrow_mut().push(name.to_string()));
}

fn default_file_path(name: &str) -> PathBuf {
    let pid = std::process::id();
    std::env::temp_dir().join(format!(".tmpfile-{pid}-{name}"))
}

fn default_dir_path(name: &str) -> PathBuf {
    let pid = std::process::id();
    std::env::temp_dir().join(format!(".tmp-{pid}-{name}"))
}

#[test]
fn named_file_does_not_truncate_an_existing_file() {
    let squat = default_file_path("SQUATFILE001");
    std::fs::write(&squat, b"keep me").unwrap();

    force_next_name("SQUATFILE001");
    let created = NamedTempFile::new();
    let content = std::fs::read(&squat).unwrap();
    let _ = std::fs::remove_file(&squat);

    assert_eq!(content, b"keep me", "existing file was truncated");
    let f = created.expect("creation must retry with a fresh name");
    assert_ne!(f.path(), squat.as_path());
    assert!(f.path().is_file());
}

#[test]
fn named_file_with_prefix_does_not_truncate_an_existing_file() {
    let squat = std::env::temp_dir().join("mtd-squat-SQUATFILE002");
    std::fs::write(&squat, b"keep me").unwrap();

    force_next_name("SQUATFILE002");
    let created = NamedTempFile::with_prefix("mtd-squat");
    let content = std::fs::read(&squat).unwrap();
    let _ = std::fs::remove_file(&squat);

    assert_eq!(content, b"keep me", "existing file was truncated");
    let f = created.expect("creation must retry with a fresh name");
    assert_ne!(f.path(), squat.as_path());
}

#[test]
fn temp_dir_retries_when_the_name_is_taken() {
    let squat = default_dir_path("SQUATDIR0001");
    let _ = std::fs::create_dir(&squat);

    force_next_name("SQUATDIR0001");
    let created = TempDir::new();
    let _ = std::fs::remove_dir(&squat);

    let dir = created.expect("creation must retry with a fresh name");
    assert_ne!(dir.path(), squat.as_path());
    assert!(dir.path().is_dir());
}

#[cfg(unix)]
#[test]
fn named_file_does_not_follow_a_planted_symlink() {
    let victim_dir = TempDir::new().unwrap();
    let victim = victim_dir.path().join("victim");
    std::fs::write(&victim, b"do not touch").unwrap();

    let link = default_file_path("SQUATLINK001");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&victim, &link).unwrap();

    force_next_name("SQUATLINK001");
    let created = NamedTempFile::new();
    let content = std::fs::read(&victim).unwrap();
    let _ = std::fs::remove_file(&link);

    assert_eq!(content, b"do not touch", "symlink target was truncated");
    let f = created.expect("creation must retry with a fresh name");
    assert_ne!(f.path(), link.as_path());
    let meta = std::fs::symlink_metadata(f.path()).unwrap();
    assert!(meta.file_type().is_file());
}

#[cfg(unix)]
#[test]
fn named_file_with_prefix_does_not_follow_a_planted_symlink() {
    let victim_dir = TempDir::new().unwrap();
    let victim = victim_dir.path().join("victim");
    std::fs::write(&victim, b"do not touch").unwrap();

    let link = std::env::temp_dir().join("mtd-link-SQUATLINK002");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&victim, &link).unwrap();

    force_next_name("SQUATLINK002");
    let created = NamedTempFile::with_prefix("mtd-link");
    let content = std::fs::read(&victim).unwrap();
    let _ = std::fs::remove_file(&link);

    assert_eq!(content, b"do not touch", "symlink target was truncated");
    assert!(created.is_ok(), "creation must retry with a fresh name");
}
