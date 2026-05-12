<h1 align="center">
    <strong>mod-tempdir</strong>
    <br>
    <sup><sub>TEMPORARY DIRECTORY MANAGEMENT FOR RUST</sub></sup>
</h1>

<p align="center">
    <a href="https://crates.io/crates/mod-tempdir"><img alt="crates.io" src="https://img.shields.io/crates/v/mod-tempdir.svg"></a>
    <a href="https://crates.io/crates/mod-tempdir"><img alt="downloads" src="https://img.shields.io/crates/d/mod-tempdir.svg"></a>
    <a href="https://docs.rs/mod-tempdir"><img alt="docs.rs" src="https://docs.rs/mod-tempdir/badge.svg"></a>
    <a href="https://github.com/jamesgober/mod-tempdir/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/mod-tempdir/actions/workflows/ci.yml/badge.svg"></a>
</p>

<p align="center">
    Auto-cleanup on Drop. Collision-resistant naming. Cross-platform paths.<br>
    Zero external dependencies. <code>tempfile</code> replacement at MSRV 1.75.
</p>

---

## What it does

Creates temporary directories, gives you the path, and automatically
deletes them (recursively) when the handle goes out of scope. Cross-
platform: uses `%TEMP%` on Windows, `/tmp` on Linux/macOS.

## Why a tempfile replacement

The `tempfile` crate pulls in `getrandom 0.4`, which uses
`edition2024` and requires Rust 1.85+. For projects with broader
MSRV targets, this is a real cost.

`mod-tempdir` provides the same core functionality with MSRV 1.75
and zero external dependencies. The trade: no `NamedTempFile` (just
directories for now), no cleanup-on-startup for orphaned dirs from
crashed processes. Both can be added in later releases if there's
demand.

## Quick start

```toml
[dependencies]
mod-tempdir = "0.1"
```

```rust
use mod_tempdir::TempDir;

let dir = TempDir::new()?;
let file_path = dir.path().join("test.txt");
std::fs::write(&file_path, b"hello")?;
// `dir` and its contents are deleted at end of scope.
# Ok::<(), std::io::Error>(())
```

## API

```rust
TempDir::new()                  // → io::Result<TempDir>
TempDir::with_prefix("test")    // → io::Result<TempDir> with custom prefix
dir.path()                       // → &Path
dir.persist()                    // → PathBuf (disables cleanup)
dir.cleanup_on_drop()            // → bool
```

## How it picks unique names

By default, the name is derived from:
- Process ID (`PID`)
- Current nanosecond timestamp
- Atomic counter (guarantees uniqueness within a process)

This is collision-resistant enough for test fixtures. **It is NOT
cryptographically secure** — if you need crypto-quality random
names (for security-sensitive temp files), wait for the
`mod-rand::tier3` integration in `0.9.x`.

## The `dev-*` and `mod-*` ecosystem

This crate is the foundation for `dev-fixtures`'s temporary working
directories. It also slots cleanly into any project that needs
auto-cleanup temp dirs without pulling in `tempfile`'s dep tree.

## Status

`v0.1.0` is the name-claim release. Real implementations of:

- Collision-resistant naming via `mod-rand::tier2`
- `fsys`-based filesystem primitives for performance
- Cleanup-on-startup for orphaned dirs from crashed processes
- Windows file-lock retry logic

land in `0.9.x`.

## Minimum supported Rust version

`1.75` — pinned in `Cargo.toml` and verified by CI.

## License

Apache-2.0. See [LICENSE](LICENSE).
