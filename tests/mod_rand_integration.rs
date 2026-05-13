// Integration tests for the `mod-rand` feature. The entire file is
// gated so the no-feature build of `cargo test` compiles it to nothing.

#![cfg(feature = "mod-rand")]

use std::collections::{HashMap, HashSet};

use mod_tempdir::{__unique_name_for_tests, NamedTempFile, TempDir};

/// Crockford base32 alphabet: `0-9A-Z` minus `I, L, O, U`. Both the
/// crate's built-in placeholder and `mod_rand::tier2::unique_name`
/// emit names from this exact set.
const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[test]
fn ten_thousand_names_are_unique_and_well_formed() {
    const N: usize = 10_000;
    const LEN: usize = 12;

    let mut seen: HashSet<String> = HashSet::with_capacity(N);
    for _ in 0..N {
        let name = __unique_name_for_tests(LEN);
        assert_eq!(
            name.len(),
            LEN,
            "name {name:?} has wrong length: expected {LEN}, got {}",
            name.len()
        );
        assert!(
            name.chars().all(|c| ALPHABET.contains(c)),
            "name {name:?} contains a char outside Crockford base32"
        );
        assert!(
            seen.insert(name.clone()),
            "duplicate name {name:?} in batch of {N}"
        );
    }
    assert_eq!(seen.len(), N);
}

#[test]
fn name_alphabet_distribution_is_reasonable() {
    // Generates 120,000 characters (10,000 names of length 12).
    // A uniform generator would put ~3,750 of each of the 32 alphabet
    // characters. The bounds below are deliberately loose so the test
    // does not flake on a healthy generator but still catches a stuck
    // or biased one.
    const N: usize = 10_000;
    const LEN: usize = 12;
    const TOTAL_CHARS: usize = N * LEN;
    const MIN_PER_CHAR: usize = 100;
    // 12% upper bound vs. ~3.1% expected. A generator that crosses
    // this is broken, not unlucky.
    const MAX_FRACTION: f64 = 0.12;

    let mut counts: HashMap<char, usize> = HashMap::with_capacity(32);
    for _ in 0..N {
        for c in __unique_name_for_tests(LEN).chars() {
            *counts.entry(c).or_insert(0) += 1;
        }
    }

    for ch in ALPHABET.chars() {
        let count = counts.get(&ch).copied().unwrap_or(0);
        assert!(
            count >= MIN_PER_CHAR,
            "alphabet char {ch:?} appeared {count} times in {TOTAL_CHARS} \
             chars; expected at least {MIN_PER_CHAR}. Generator may be stuck."
        );
    }

    let max_count = counts.values().copied().max().unwrap_or(0);
    let max_fraction = max_count as f64 / TOTAL_CHARS as f64;
    assert!(
        max_fraction < MAX_FRACTION,
        "one alphabet char claims {:.2}% of output (max allowed {:.2}%); \
         generator is biased",
        max_fraction * 100.0,
        MAX_FRACTION * 100.0
    );

    // Sanity: no char outside the alphabet should appear at all.
    for (ch, count) in &counts {
        assert!(
            ALPHABET.contains(*ch),
            "char {ch:?} appeared {count} times but is outside Crockford base32"
        );
    }
}

#[test]
fn end_to_end_tempdir_paths_are_unique() {
    // Smaller end-to-end check that goes through TempDir::new(), so
    // the wiring from the generator into the real path layout is also
    // covered. Cleanup happens via Drop at end of scope.
    const N: usize = 100;

    let mut dirs: Vec<TempDir> = Vec::with_capacity(N);
    let mut paths: HashSet<std::path::PathBuf> = HashSet::with_capacity(N);

    for _ in 0..N {
        let d = TempDir::new().expect("TempDir::new failed");
        assert!(
            d.path().exists(),
            "TempDir reports path {:?} but the directory does not exist",
            d.path()
        );
        paths.insert(d.path().to_path_buf());
        dirs.push(d);
    }

    assert_eq!(
        paths.len(),
        N,
        "end-to-end TempDir paths collided: {} unique out of {N}",
        paths.len()
    );
}

#[test]
fn end_to_end_named_temp_file_paths_are_unique() {
    // Parallel of the TempDir end-to-end check. Verifies the
    // generator wiring inside `NamedTempFile::new()` produces unique
    // paths when the `mod-rand` feature is enabled.
    const N: usize = 100;

    let mut files: Vec<NamedTempFile> = Vec::with_capacity(N);
    let mut paths: HashSet<std::path::PathBuf> = HashSet::with_capacity(N);

    for _ in 0..N {
        let f = NamedTempFile::new().expect("NamedTempFile::new failed");
        assert!(
            f.path().is_file(),
            "NamedTempFile reports {:?} but the path is not a regular file",
            f.path()
        );
        paths.insert(f.path().to_path_buf());
        files.push(f);
    }

    assert_eq!(
        paths.len(),
        N,
        "end-to-end NamedTempFile paths collided: {} unique out of {N}",
        paths.len()
    );
}
