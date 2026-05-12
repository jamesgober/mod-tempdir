//! # mod-tempdir
//!
//! Temporary directory management for Rust. Auto-cleanup on Drop,
//! collision-resistant naming, cross-platform paths.
//!
//! Designed as a `tempfile` replacement at MSRV 1.75 with zero
//! external dependencies.
//!
//! ## Quick example
//!
//! ```no_run
//! use mod_tempdir::TempDir;
//!
//! let dir = TempDir::new().unwrap();
//! // ... use dir.path() to do work ...
//! // dir is automatically deleted when it goes out of scope
//! ```
//!
//! ## Status
//!
//! `v0.1.0` is the name-claim release. Real implementation lands in
//! `0.9.x` using fsys for the filesystem primitives.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary directory that auto-deletes when dropped.
///
/// # Example
///
/// ```no_run
/// use mod_tempdir::TempDir;
///
/// let dir = TempDir::new().unwrap();
/// let file_path = dir.path().join("test.txt");
/// std::fs::write(&file_path, b"hello").unwrap();
/// // dir and its contents are deleted at end of scope
/// ```
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl TempDir {
    /// Create a new temporary directory in the system's temp location
    /// (`/tmp` on Linux/macOS, `%TEMP%` on Windows).
    ///
    /// In `0.1.0` this is a placeholder using a deterministic name
    /// pattern. Real collision-resistant naming lands in `0.9.x`.
    pub fn new() -> io::Result<Self> {
        let name = unique_name(12);
        let path = std::env::temp_dir().join(format!(".tmp-{name}"));
        std::fs::create_dir(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Create a new temporary directory with the given prefix.
    pub fn with_prefix(prefix: &str) -> io::Result<Self> {
        let name = unique_name(12);
        let path = std::env::temp_dir().join(format!("{prefix}-{name}"));
        std::fs::create_dir(&path)?;
        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Return the path of this temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume this `TempDir` and return the path, disabling cleanup
    /// on drop. The directory and its contents will persist.
    ///
    /// Use this when you want to inspect contents after a test fails.
    pub fn persist(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Return `true` if the directory will be deleted on drop.
    pub fn cleanup_on_drop(&self) -> bool {
        self.cleanup_on_drop
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn unique_name(len: usize) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    let pid = std::process::id() as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    // Placeholder mixing. Real mix function (using mod-rand::tier2)
    // lands in 0.9.x.
    let mut state = pid.wrapping_mul(0x9E3779B97F4A7C15)
        ^ nanos.wrapping_mul(0xBF58476D1CE4E5B9)
        ^ counter.wrapping_mul(0x94D049BB133111EB);

    let mut out = String::with_capacity(len);
    while out.len() < len {
        out.push(ALPHABET[(state & 31) as usize] as char);
        state >>= 5;
        if state == 0 {
            state = nanos.wrapping_mul(counter.wrapping_add(1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_dir() {
        let dir = TempDir::new().unwrap();
        assert!(dir.path().exists());
        assert!(dir.path().is_dir());
    }

    #[test]
    fn auto_cleanup() {
        let path = {
            let dir = TempDir::new().unwrap();
            dir.path().to_path_buf()
        };
        // After drop, the directory should no longer exist.
        assert!(!path.exists());
    }

    #[test]
    fn persist_disables_cleanup() {
        let dir = TempDir::new().unwrap();
        let path = dir.persist();
        assert!(path.exists());
        // Clean up manually since persist was used.
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn with_prefix_works() {
        let dir = TempDir::with_prefix("test").unwrap();
        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("test-"));
    }

    #[test]
    fn two_dirs_unique() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        assert_ne!(a.path(), b.path());
    }
}
