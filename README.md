<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <strong>mod-tempdir</strong>
    <br>
    <sup><sub>TEMPORARY DIRECTORY MANAGEMENT FOR RUST</sub></sup>
</h1>

<p align="center">
    <a href="https://crates.io/crates/mod-tempdir"><img alt="crates.io" src="https://img.shields.io/crates/v/mod-tempdir.svg"></a>
    <a href="https://crates.io/crates/mod-tempdir"><img alt="downloads" src="https://img.shields.io/crates/d/mod-tempdir.svg"></a>
    <a href="https://docs.rs/mod-tempdir"><img alt="docs.rs" src="https://docs.rs/mod-tempdir/badge.svg"></a>
    <img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.75%2B-blue.svg?style=flat-square" title="Rust Version">
    <a href="https://github.com/jamesgober/mod-tempdir/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/mod-tempdir/actions/workflows/ci.yml/badge.svg"></a>
</p>

<p align="center">
    Auto-cleanup on Drop. Collision-resistant naming. Cross-platform paths.<br>
    Zero runtime deps in the default build. <code>tempfile</code> replacement at MSRV 1.75.
</p>

---

## What it does

Creates a temporary directory, hands you the path, and deletes it
(recursively) when the handle goes out of scope. The OS-standard
temp location is used on every supported platform: `%TEMP%` on
Windows, `/tmp` on Linux and macOS, anywhere `std::env::temp_dir()`
points elsewhere.

Cleanup is best-effort. A failure during drop (file still held open,
permission denied, network filesystem hiccup) is silent: a `Drop`
impl must not panic. Use `persist()` if you want the directory to
survive past drop so you can inspect it.

## Why a tempfile replacement

The `tempfile` crate pulls in `getrandom 0.4`, which uses
`edition2024` and requires Rust 1.85+. If your project supports an
older MSRV, that single dependency forces the rest of your build to
follow.

`mod-tempdir` provides the same core capability at MSRV 1.75. The
default build has zero runtime dependencies outside `std`. An opt-in
feature delegates name generation to `mod-rand::tier2` when you want
uniformly distributed names from a separately maintained generator;
the public API is identical either way.

The current trade: no cleanup-on-startup pass for orphaned entries
from crashed processes. Tracked for a later release in the `0.9.x`
line.

## Quick start

```toml
[dependencies]
mod-tempdir = "0.9"
```

```rust
use mod_tempdir::{NamedTempFile, TempDir};
use std::io::Write;

// A temporary directory:
let dir = TempDir::new()?;
let file_path = dir.path().join("test.txt");
std::fs::write(&file_path, b"hello")?;
// `dir` and its contents are deleted at end of scope.

// A standalone temporary file:
let f = NamedTempFile::new()?;
let mut h = std::fs::OpenOptions::new().write(true).open(f.path())?;
h.write_all(b"hello")?;
// `f` is deleted at end of scope.
# Ok::<(), std::io::Error>(())
```

## API

```rust
TempDir::new()                  // -> io::Result<TempDir>
TempDir::with_prefix("test")    // -> io::Result<TempDir> with custom prefix
dir.path()                       // -> &Path
dir.persist()                    // -> PathBuf (disables cleanup)
dir.cleanup_on_drop()            // -> bool

NamedTempFile::new()             // -> io::Result<NamedTempFile>
NamedTempFile::with_prefix("x")  // -> io::Result<NamedTempFile> with custom prefix
file.path()                      // -> &Path
file.persist()                   // -> PathBuf (disables cleanup)
file.persist_atomic(target)      // -> Result<PathBuf, PersistAtomicError> (atomic durable move, source preserved on failure)
file.cleanup_on_drop()           // -> bool

cleanup_orphans(max_age_hours)   // -> io::Result<usize> (removed count)
```

Both types share the same `with_prefix` / `path` / `persist` /
`cleanup_on_drop` shape, the same name-generation pipeline, and the
same silent best-effort Drop semantics. The `TempDir` signature
surface has not changed since `0.1.0`. `NamedTempFile` and
`cleanup_orphans` joined the public surface in `0.9.2`;
`NamedTempFile::persist_atomic` joins in `0.9.3`. All of it is
stable through the rest of the `0.9.x` line; the `1.0.0` release
pins everything.

### Default basenames

Default basenames are deliberately distinguishable so an operator
inspecting the OS temp dir can tell entries apart at a glance, and
they carry the originating process's PID so orphans from crashed
runs can be identified later:

| Type            | Default basename                     |
|-----------------|--------------------------------------|
| `TempDir`       | `.tmp-{pid}-{12-char-name}`          |
| `NamedTempFile` | `.tmpfile-{pid}-{12-char-name}`      |

Caller-supplied prefixes via `with_prefix` are joined verbatim
(without the PID segment) and override the default. The user's
namespace is the user's responsibility to clean up.

## Feature flags

| Flag       | Default | Effect |
|------------|---------|--------|
| `mod-rand` | off     | Use `mod_rand::tier2::unique_name` for naming. Adds one optional dependency (`mod-rand`, itself free of further deps). Applies to both `TempDir` and `NamedTempFile`. |

Default naming uses an internal mixer over the process ID, the
nanosecond clock, and a per-process atomic counter. It is fast,
collision-free within a process, and good enough for test fixtures.

The `mod-rand` feature swaps that mixer for `mod_rand::tier2`, a
SplitMix + Stafford-finisher pipeline that produces a uniform
distribution across the alphabet without changing how names look
on disk. Enable when you want the stronger statistical properties
of a tested generator. Both paths use the same Crockford base32
alphabet (`0-9A-Z` minus `I`, `L`, `O`, `U`), so a caller that
inspects directory basenames keeps working when the feature is
toggled.

```toml
[dependencies]
mod-tempdir = { version = "0.9", features = ["mod-rand"] }
```

## How it picks unique names

By default, the name is derived from:

- Process ID (`PID`)
- Current nanosecond timestamp
- An atomic counter that guarantees uniqueness within a process

This is collision-resistant enough for test fixtures and concurrent
local work. It is **not** cryptographically secure. If you need
crypto-quality random names (for example, for security-sensitive
temp paths), generate one with `mod_rand::tier3` and pass it to
`TempDir::with_prefix`.

## Cleaning up after crashes

If a process panics, gets SIGKILL'd, or otherwise exits without
running `Drop`, its temp entries are left behind. `cleanup_orphans`
sweeps the OS temp directory for default-prefix entries from this
crate and removes the ones whose owning processes are no longer
alive:

```rust
use mod_tempdir::cleanup_orphans;

// At startup, before creating any new temp entries:
let removed = cleanup_orphans(24).unwrap_or(0);
eprintln!("cleanup_orphans removed {removed} orphan(s)");
```

Removal requires **both** conditions: the originating PID is no
longer alive, and the entry is at least `max_age_hours` old.

PID liveness is checked via `/proc/{pid}` on Linux. On macOS and
Windows, cross-platform process introspection requires platform
crates this library does not pull in, so the liveness check is
treated as "dead" there. On those platforms the age threshold is
the only safety guard: pick `max_age_hours` larger than any
legitimate process lifetime, or call `cleanup_orphans` only at
known-safe moments (typically program startup).

Caller-supplied `with_prefix` paths and legacy entries from earlier
versions (without a PID segment in the basename) are never touched.
The user's namespace and historical entries are off-limits.

## Atomic persistence

`NamedTempFile::persist_atomic(target)` is the finalize-with-durability
counterpart to `persist`. It performs the canonical "atomic durable
write" sequence so that either the previous version of `target` or
the new contents survive a crash, never a half-written file:

1. `fsync` the temp file to push its contents to disk.
2. Atomic `std::fs::rename` onto `target` (`rename(2)` on Unix,
   `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows).
3. Best-effort `fsync` of the target's parent directory so the
   rename itself survives a crash.

```rust
use mod_tempdir::NamedTempFile;
use std::io::Write;

let f = NamedTempFile::new()?;
{
    let mut h = std::fs::OpenOptions::new().write(true).open(f.path())?;
    h.write_all(b"finalized payload")?;
}
match f.persist_atomic("config.toml") {
    Ok(landed) => {
        // `landed` is the target path; cleanup-on-drop is disabled.
    }
    Err(e) => {
        // The temp file is preserved on disk and the original
        // NamedTempFile is in `e.file` so you can retry.
        eprintln!("persist failed: {}", e.error);
    }
}
# Ok::<(), std::io::Error>(())
```

On any failure (rename error, missing parent, cross-filesystem
target), the temp file is **preserved** on disk and the original
`NamedTempFile` is returned to the caller via the
[`PersistAtomicError`] error type so a retry or fallback path
doesn't lose the source. This matches the data-integrity guarantee
of the `tempfile` crate's persist API.

`rename` is atomic only within a single filesystem. If `target` is
on a different mount than `std::env::temp_dir()`, `persist_atomic`
returns `EXDEV` (Unix) or the Windows equivalent inside the
`PersistAtomicError`. For cross-filesystem finalization, copy
through the target filesystem first using `TempDir::with_prefix`
rooted at the target's parent.

## Concurrency

Every public constructor (`TempDir::new`, `TempDir::with_prefix`,
`NamedTempFile::new`, `NamedTempFile::with_prefix`) is safe to call
from many threads at once. The verification suite includes paired
stress tests that fire 256 threads through a shared barrier for each
type and assert every returned path is distinct. Both stress tests
run on both feature configurations.

## The `dev-*` and `mod-*` ecosystem

This crate is the foundation for `dev-fixtures`'s temporary working
directories. It also slots cleanly into any project that needs
auto-cleanup temp dirs without pulling in `tempfile`'s dep tree.

## Roadmap

- `v0.9.0` shipped the `mod-rand` integration.
- `v0.9.2` followed up with `NamedTempFile`, `cleanup_orphans`, and
  PID-aware default basenames. The originally planned `v0.9.1`
  (a separate `NamedTempFile` release) was bundled into `v0.9.2`;
  no `v0.9.1` tag was published.
- `v0.9.3` added `NamedTempFile::persist_atomic` using `std::fs`
  primitives (fsync + atomic rename + parent-dir fsync). A previously
  reserved `fsys` integration for this milestone was audited and not
  taken; `fsys`'s atomic-rename primitive is `pub(crate)` and its
  public alternative requires a single-root handle, neither of which
  fits the generic `temp_dir → arbitrary_target` move. `std::fs::rename`
  invokes the same OS primitives `fsys` uses internally.

Remaining items before `v1.0.0`:

- `v1.0.0`: API stabilization (final rustdoc pass, cross-platform CI
  green on all three OSes, one downstream consumer integration)

A very early plan for `v0.9.1` proposed routing directory operations
through `fsys`; that idea was retired during the `0.9.x` line because
for single-syscall operations like `mkdir(2)` and `unlink(2)`,
`std::fs` is already the fastest available path. See the project
ROADMAP for the retirement note.

## Minimum supported Rust version

`1.75`. Pinned in `Cargo.toml` and verified by CI on every push.

## License

Apache-2.0. See [LICENSE](LICENSE).



<!-- COPYRIGHT
---------------------------------->
<div align="center">
  <br>
  <h2></h2>
  Copyright &copy; 2026 James Gober.
</div>