# mod-tempdir: Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`mod-tempdir` MUST provide temporary directory management with
automatic cleanup. It is the zero-dependency, low-MSRV replacement
for `tempfile`.

## 2. Core capabilities

- Create temp directories in the OS's standard temp location.
- Create temp files in the OS's standard temp location.
- Auto-delete temp directories (recursively) on Drop.
- Auto-delete temp files on Drop.
- Collision-resistant naming, shared between directories and files.
- Optional persistence (disable cleanup for debugging).
- Optional prefix for tracking purposes.
- Optional sweep at process startup to remove entries left behind
  by crashed processes (`cleanup_orphans`).

## 3. API

```rust
pub struct TempDir { /* private */ }

impl TempDir {
    pub fn new() -> io::Result<Self>;
    pub fn with_prefix(prefix: &str) -> io::Result<Self>;
    pub fn path(&self) -> &Path;
    pub fn persist(self) -> PathBuf;
    pub fn cleanup_on_drop(&self) -> bool;
}

impl Drop for TempDir { /* recursive cleanup */ }

pub struct NamedTempFile { /* private */ }

impl NamedTempFile {
    pub fn new() -> io::Result<Self>;
    pub fn with_prefix(prefix: &str) -> io::Result<Self>;
    pub fn path(&self) -> &Path;
    pub fn persist(self) -> PathBuf;
    pub fn persist_atomic(self, target: impl AsRef<Path>) -> Result<PathBuf, PersistAtomicError>;
    pub fn cleanup_on_drop(&self) -> bool;
}

impl Drop for NamedTempFile { /* remove_file */ }

pub struct PersistAtomicError {
    pub error: io::Error,
    pub file: NamedTempFile,
}

pub fn cleanup_orphans(max_age_hours: u64) -> io::Result<usize>;
```

Default basenames carry the originating process's PID:
`TempDir::new` produces `.tmp-{pid}-{name12}` and
`NamedTempFile::new` produces `.tmpfile-{pid}-{name12}`. The PID is
the key `cleanup_orphans` uses to identify orphans from crashed
runs. `with_prefix` outputs do not carry a PID and are outside
`cleanup_orphans`'s namespace.

## 4. Determinism

- Directory creation MUST be idempotent in the sense that two
  concurrent `TempDir::new()` calls in the same process MUST NOT
  produce the same path (atomic counter guarantees this).
- File creation MUST be idempotent in the same sense: two concurrent
  `NamedTempFile::new()` calls in the same process MUST NOT produce
  the same path. The shared name generator means a single guarantee
  covers both types.
- A `TempDir::new()` call and a concurrent `NamedTempFile::new()`
  call MUST NOT collide. Distinct default basename prefixes
  (`.tmp-` for directories, `.tmpfile-` for files) reinforce this
  on top of the underlying name uniqueness.
- Two processes MUST be extremely unlikely to produce the same path
  (PID + nanos varies).

## 5. Cleanup semantics

- `TempDir::drop` MUST attempt recursive deletion via
  `std::fs::remove_dir_all`.
- `NamedTempFile::drop` MUST attempt deletion via
  `std::fs::remove_file`.
- Deletion failures (file in use, permissions, Windows
  `ERROR_SHARING_VIOLATION` from a caller-held handle) MUST be
  silent. Drop MUST NOT panic.
- `persist()` MUST disable cleanup so the directory or file
  survives drop.

## 6. Dependencies

This crate MUST NOT add any runtime dependency outside `std` in its
default build. As of `0.9.0`, an optional dependency on `mod-rand`
is available behind the `mod-rand` feature flag for uniformly
distributed naming. No `tempfile`, `getrandom`, or `rand` direct
dependencies on any feature configuration.

For filesystem operations (`mkdir`, `rmdir`, `rmdir_all`), this crate
MUST use `std::fs` directly. These operations map to single OS
syscalls and have no library-level optimization headroom. A prior
plan to route them through `fsys` was retired (see ROADMAP.md
"Note on the retired fsys integration milestone"); `std::fs` IS the
fast path for these operations.

## 7. Out of scope

- Cryptographically random naming (use `mod-rand::tier3` to generate
  the name and pass it as a prefix).
- File-locking primitives (not our domain).
- NFS / network filesystem edge cases (best-effort cleanup, document
  the limitation).
- General filesystem operation speed-ups beyond `std::fs`. For
  storage-engine workloads (durable writes, journal append, atomic
  rename with fsync), use `fsys` directly from the calling crate;
  `mod-tempdir` does not perform those workloads.

## 8. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API.
