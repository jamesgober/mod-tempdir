// Security regression tests: private permissions on Unix and
// rejection of prefixes that would leave the parent directory.

use std::io::ErrorKind;

use mod_tempdir::{NamedTempFile, TempDir};

const BAD_PREFIXES: &[&str] = &[
    "../escape",
    "..",
    "a/b",
    "a\\b",
    "/abs",
    "nested/../../escape",
    "nul\0byte",
];

#[test]
fn temp_dir_rejects_prefixes_that_leave_the_parent() {
    for prefix in BAD_PREFIXES {
        match TempDir::with_prefix(prefix) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidInput, "prefix {prefix:?}"),
            Ok(dir) => panic!("prefix {prefix:?} was accepted: {}", dir.path().display()),
        }
    }
}

#[test]
fn named_file_rejects_prefixes_that_leave_the_parent() {
    for prefix in BAD_PREFIXES {
        match NamedTempFile::with_prefix(prefix) {
            Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidInput, "prefix {prefix:?}"),
            Ok(f) => panic!("prefix {prefix:?} was accepted: {}", f.path().display()),
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_drive_prefixes_are_rejected() {
    for prefix in ["C:escape", "C:\\escape"] {
        let e = TempDir::with_prefix(prefix).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidInput, "prefix {prefix:?}");
        let e = NamedTempFile::with_prefix(prefix).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::InvalidInput, "prefix {prefix:?}");
    }
}

#[test]
fn ordinary_prefixes_still_work() {
    for prefix in ["my-app", "my_app.v2", "", "a b"] {
        let dir = TempDir::with_prefix(prefix).unwrap();
        assert_eq!(dir.path().parent(), Some(std::env::temp_dir().as_path()));
        let f = NamedTempFile::with_prefix(prefix).unwrap();
        assert_eq!(f.path().parent(), Some(std::env::temp_dir().as_path()));
    }
}

#[cfg(unix)]
mod unix_permissions {
    use std::os::unix::fs::PermissionsExt;

    use mod_tempdir::{NamedTempFile, TempDir};

    fn mode(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn named_files_are_owner_only() {
        let f = NamedTempFile::new().unwrap();
        assert_eq!(mode(f.path()), 0o600);
        let f = NamedTempFile::with_prefix("mtd-perm").unwrap();
        assert_eq!(mode(f.path()), 0o600);
    }

    #[test]
    fn temp_dirs_are_owner_only() {
        let d = TempDir::new().unwrap();
        assert_eq!(mode(d.path()), 0o700);
        let d = TempDir::with_prefix("mtd-perm").unwrap();
        assert_eq!(mode(d.path()), 0o700);
    }
}
