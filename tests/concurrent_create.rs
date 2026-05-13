// Stress test: many threads call `TempDir::new()` simultaneously and
// must each receive a unique path. Runs on both feature configurations
// (default and `mod-rand`) so the collision guarantee is verified for
// both the placeholder generator and the `mod_rand::tier2` generator.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use mod_tempdir::TempDir;

/// Thread count for the stress test. 256 is large enough to span many
/// nanosecond ticks during thread spawn, which is the adversarial case
/// for any time-based name generator, and small enough to stay cheap
/// on constrained CI runners.
const THREADS: usize = 256;

#[test]
fn concurrent_tempdir_new_yields_unique_paths() {
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);

    for _ in 0..THREADS {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // Synchronize all threads so the actual `new()` calls
            // happen as close together in time as the scheduler will
            // allow. This maximizes contention on the global counter
            // and on the nanosecond clock.
            b.wait();
            TempDir::new().expect("TempDir::new failed under contention")
        }));
    }

    let dirs: Vec<TempDir> = handles
        .into_iter()
        .map(|h| h.join().expect("worker thread panicked"))
        .collect();

    let paths: HashSet<PathBuf> = dirs.iter().map(|d| d.path().to_path_buf()).collect();

    assert_eq!(
        paths.len(),
        THREADS,
        "path collision under contention: {} unique paths for {THREADS} threads",
        paths.len()
    );

    // Verify every reported path actually materialized on disk.
    for d in &dirs {
        assert!(
            d.path().exists(),
            "TempDir reports {:?} but the directory is not on disk",
            d.path()
        );
    }

    // `dirs` drops here. Each TempDir cleans up its own directory.
}
