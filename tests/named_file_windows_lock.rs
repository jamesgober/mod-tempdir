// Windows-specific test: NamedTempFile::drop must not panic if a
// caller holds an open handle to the file at the moment of Drop.
// Cleanup will fail silently in that case; the file may be left on
// disk. The library cannot force-close a caller-owned handle.

#![cfg(windows)]

use std::fs::OpenOptions;

use mod_tempdir::NamedTempFile;

#[test]
fn drop_with_open_handle_does_not_panic_and_is_silent() {
    let f = NamedTempFile::new().expect("NamedTempFile::new failed");
    let path = f.path().to_path_buf();

    // Acquire an open handle to the file before dropping the
    // NamedTempFile. This is the scenario where Windows returns
    // ERROR_SHARING_VIOLATION from `remove_file` during Drop.
    let held = OpenOptions::new()
        .read(true)
        .open(&path)
        .expect("opening a read handle against NamedTempFile path failed");

    // Load-bearing assertion: the explicit drop call returns without
    // panicking. Whether the file is removed depends on Windows
    // behavior with the still-open handle; we make no assertion
    // about its post-Drop existence.
    drop(f);

    // Close the held handle and ensure the file is cleaned up so
    // this test does not leak into the OS temp dir on Windows.
    drop(held);
    let _ = std::fs::remove_file(&path);
}
