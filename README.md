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
    Auto-cleanup on Drop. Exclusive creation, private permissions, OS-random names.<br>
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
default build has zero runtime dependencies outside `std`. Names come
from the operating system's random source in every build; an opt-in
feature routes that through `mod-rand::tier3` instead of `std`. The
public API is identical either way.

If a process crashes before `Drop` can run, the orphaned temp
entries are not lost forever: `cleanup_orphans` (see the
"Cleaning up after crashes" section below) sweeps them on
demand. Cross-filesystem persistence and OS-level filesystem
ops beyond `std::fs` remain out of scope; for those, compose
this crate with a storage-engine library like `fsys`.

## Quick start

```toml
[dependencies]
mod-tempdir = "1"
```

```rust
use mod_tempdir::{NamedTempFile, TempDir};

// A temporary directory:
let dir = TempDir::new()?;
let file_path = dir.path().join("test.txt");
std::fs::write(&file_path, b"hello")?;
// `dir` and its contents are deleted at end of scope.

// A standalone temporary file, written through the handle it was
// created with:
use std::io::Write;
let mut f = NamedTempFile::new()?;
f.write_all(b"hello")?;
// `f` is deleted at end of scope.
# Ok::<(), std::io::Error>(())
```

## API

```rust
TempDir::new()                  // -> io::Result<TempDir>
TempDir::new_in(dir)            // -> io::Result<TempDir> under `dir`
TempDir::with_prefix("test")    // -> io::Result<TempDir> with custom prefix
TempDir::with_prefix_in(dir, "test")
dir.path()                       // -> &Path
dir.persist()                    // -> PathBuf (disables cleanup)
dir.cleanup_on_drop()            // -> bool

NamedTempFile::new()             // -> io::Result<NamedTempFile>
NamedTempFile::new_in(dir)       // -> io::Result<NamedTempFile> under `dir`
NamedTempFile::with_prefix("x")  // -> io::Result<NamedTempFile> with custom prefix
NamedTempFile::with_prefix_in(dir, "x")
file.path()                      // -> &Path
file.as_file()                   // -> &File (the handle from the exclusive create)
file.as_file_mut()               // -> &mut File
file.reopen()                    // -> io::Result<File> (checked to be the same file)
file.persist()                   // -> PathBuf (disables cleanup)
file.persist_atomic(target)      // -> Result<PathBuf, PersistAtomicError> (atomic durable move, source preserved on failure)
file.cleanup_on_drop()           // -> bool

cleanup_orphans(max_age_hours)   // -> io::Result<usize> (removed count)
cleanup_orphans_in(dir, max_age_hours) // -> io::Result<usize>, sweeps `dir`
```

`NamedTempFile` also implements `Read`, `Write` and `Seek` through its
handle.

Both types share the same `with_prefix` / `path` / `persist` /
`cleanup_on_drop` shape, the same name-generation pipeline, and the
same silent best-effort Drop semantics. The `TempDir` signature
surface has not changed since `0.1.0`. `NamedTempFile` and
`cleanup_orphans` joined the public surface in `0.9.2`;
`NamedTempFile::persist_atomic` in `0.9.3`; `new_in` and
`with_prefix_in` on both types in `1.1.0`; the `NamedTempFile` handle
accessors and `cleanup_orphans_in` in `1.2.0`. As of `1.0.0` the
entire public API is pinned: breaking changes require a major
bump.

### Default basenames

Default basenames are deliberately distinguishable so an operator
inspecting the OS temp dir can tell entries apart at a glance, and
they carry the originating process's PID so orphans from crashed
runs can be identified later:

| Type            | Default basename                     |
|-----------------|--------------------------------------|
| `TempDir`       | `.tmp-{pid}-{12-char-name}`          |
| `NamedTempFile` | `.tmpfile-{pid}-{12-char-name}`      |

Caller-supplied prefixes via `with_prefix` replace the default
(without the PID segment). A prefix must be a plain name: one
containing `/`, `\`, `..`, a NUL byte, or (on Windows) `:` is
rejected with `io::ErrorKind::InvalidInput`. The user's namespace is
the user's responsibility to clean up.

## Feature flags

| Flag       | Default | Effect |
|------------|---------|--------|
| `mod-rand` | off     | Draw names from `mod_rand::tier3` (the OS random source) on every call. Adds one optional dependency (`mod-rand`, itself free of further deps). Applies to both `TempDir` and `NamedTempFile`. |

Both paths draw names from the operating system's random source. The
default build reads it through `std`: `RandomState` keys come from
the OS random source, and each name is a keyed hash under such a key.
The `mod-rand` feature reads fresh OS random bytes for every name
instead, falling back to the `std` path if that read fails. Both
paths use the same Crockford base32 alphabet (`0-9A-Z` minus `I`,
`L`, `O`, `U`), so a caller that inspects basenames keeps working
when the feature is toggled.

```toml
[dependencies]
mod-tempdir = { version = "1", features = ["mod-rand"] }
```

## How entries are created

- **Unpredictable names.** The 12-character fragment carries 60 bits
  from the OS random source, in every build.
- **Exclusive creation.** Files are created with
  `OpenOptions::create_new` and directories with `DirBuilder`, so
  nothing that already exists at the chosen path is opened, truncated
  or followed. A taken path gets a fresh name (up to 64 attempts).
- **Private permissions.** On Unix, files are `0600` and directories
  `0700`. On Windows, new entries inherit the ACL of their parent;
  the default `%TEMP%` is inside the user's profile, which only that
  user, `SYSTEM` and administrators can open. The crate does not set
  Windows ACLs itself, so an entry made with `new_in` in a shared
  directory is as private as that directory.
- **Prefixes stay inside the parent.** See "Default basenames" above.
- **Use after creation goes through the handle.** `NamedTempFile`
  keeps the handle from the exclusive create. Writes through it
  (`as_file`, `as_file_mut`, the `Write` impl) always reach the file
  that was created. `persist_atomic` syncs through it and refuses to
  rename if the path no longer names that file, and `reopen` makes
  the same check. Opening `path()` by name is safe only in a
  directory that no other user or service can write, such as the
  default per-user `%TEMP%` or a sticky `/tmp`: anyone who can write
  to the parent can swap the entry for a link or another file after
  it is created.

## Cleaning up after crashes

With the default `panic = "unwind"`, `Drop` runs while a panic
unwinds, so entries are removed. They are left behind when `Drop`
never runs: a build with `panic = "abort"`, `std::process::exit`, a
kill signal or a crash, or a value that was leaked or still held
elsewhere. `cleanup_orphans` sweeps the OS temp directory for
default-prefix entries from this crate and removes the ones whose
owning processes are no longer alive (`cleanup_orphans_in(dir, ..)`
does the same for a directory you choose):

```rust
use mod_tempdir::cleanup_orphans;

// At startup, before creating any new temp entries:
let removed = cleanup_orphans(24).unwrap_or(0);
eprintln!("cleanup_orphans removed {removed} orphan(s)");
```

Removal requires **both** conditions: the originating PID is no
longer alive, and the entry is at least `max_age_hours` old.

Entries carrying the calling process's own PID are never removed.
Other PIDs are checked with the platform's process table:
`/proc/{pid}` on Linux, `kill(pid, 0)` on other Unix systems, and
`OpenProcess` with `GetExitCodeProcess` on Windows, all through
functions `std` already links, so no dependency is added. When the
answer is unclear (for example, another user's process that cannot be
queried), the process counts as alive and its entries stay. A
directory's age is the newest mtime of the directory and its direct
children, so a working directory whose files are still being written
is not old. PIDs are reused, so an orphan whose PID now belongs to an
unrelated process waits until that process exits. On Linux, PID
namespaces that share a `/tmp` see different PIDs, so a container can
misjudge entries of another container; give each its own temp
directory.

Caller-supplied `with_prefix` paths and legacy entries from earlier
versions (without a PID segment in the basename) are never touched.
The user's namespace and historical entries are off-limits.

## Atomic persistence

`NamedTempFile::persist_atomic(target)` is the finalize-with-durability
counterpart to `persist`. It performs the canonical "atomic durable
write" sequence so that either the previous version of `target` or
the new contents survive a crash, never a half-written file:

1. `fsync` the temp file through the handle held since creation.
2. Check that the temp path still names that file; if it was swapped,
   fail without renaming. The rename itself still goes by name, so a
   short window remains in a directory others can write.
3. Atomic `std::fs::rename` onto `target` (`rename(2)` on Unix,
   `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows).
4. Best-effort `fsync` of the target's parent directory so the
   rename itself survives a crash (the current directory for a bare
   file name).

```rust
use mod_tempdir::NamedTempFile;
use std::io::Write;

let mut f = NamedTempFile::new()?;
f.write_all(b"finalized payload")?;
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
on a different mount than the temp file, `persist_atomic` returns
`EXDEV` (Unix) or the Windows equivalent inside the
`PersistAtomicError`. Create the temp file next to the target with
`NamedTempFile::new_in(target_dir)` so both share a filesystem:

```rust
use mod_tempdir::NamedTempFile;
use std::io::Write;

let mut tmp = NamedTempFile::new_in("/var/lib/app")?;
tmp.write_all(b"new contents")?;
tmp.persist_atomic("/var/lib/app/config.toml")?;
# Ok::<(), std::io::Error>(())
```

## Concurrency

Every public constructor (`new`, `new_in`, `with_prefix`,
`with_prefix_in` on both `TempDir` and `NamedTempFile`) is safe to call
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
- **`v1.0.0` pins the public API.** From this release forward,
  breaking changes require a major-version bump per SemVer.
  Additive surface goes to a minor version; bug fixes and doc
  improvements to a patch.
- `v1.1.0` creates entries exclusively with private permissions,
  draws names from the OS random source in every build, rejects
  path separators in prefixes, and adds `new_in` / `with_prefix_in`.
- `v1.2.0` keeps the `NamedTempFile` handle and checks the path
  before `persist_atomic` renames, adds `as_file`, `as_file_mut`,
  `reopen` and `cleanup_orphans_in`, and makes `cleanup_orphans` check
  process liveness on every platform.

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