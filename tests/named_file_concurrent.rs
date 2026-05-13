// Stress test: many threads call `NamedTempFile::new()` simultaneously
// and must each receive a unique path. Mirrors
// `tests/concurrent_create.rs` (which covers TempDir). Runs on both
// feature configurations so the collision guarantee is verified for
// both the placeholder generator and the `mod_rand::tier2` generator.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use mod_tempdir::NamedTempFile;

/// Thread count for the stress test. Calibrated to match
/// `tests/concurrent_create.rs`. 256 spans many nanosecond ticks
/// during thread spawn, which is the adversarial case for any
/// time-based name generator, and stays cheap on CI runners.
const THREADS: usize = 256;

#[test]
fn concurrent_named_temp_file_new_yields_unique_paths() {
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);

    for _ in 0..THREADS {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // Synchronize all threads so the `new()` calls happen as
            // close together in time as the scheduler will allow.
            // Maximizes contention on the shared name generator.
            b.wait();
            NamedTempFile::new().expect("NamedTempFile::new failed under contention")
        }));
    }

    let files: Vec<NamedTempFile> = handles
        .into_iter()
        .map(|h| h.join().expect("worker thread panicked"))
        .collect();

    let paths: HashSet<PathBuf> = files.iter().map(|f| f.path().to_path_buf()).collect();

    assert_eq!(
        paths.len(),
        THREADS,
        "path collision under contention: {} unique paths for {THREADS} threads",
        paths.len()
    );

    // Verify every reported path actually exists as a regular file.
    for f in &files {
        assert!(
            f.path().is_file(),
            "NamedTempFile reports {:?} but the file is not on disk",
            f.path()
        );
    }

    // `files` drops here. Each NamedTempFile cleans up its own file.
}
