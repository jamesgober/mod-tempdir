# mod-tempdir — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`mod-tempdir` MUST provide temporary directory management with
automatic cleanup. It is the zero-dependency, low-MSRV replacement
for `tempfile`.

## 2. Core capabilities

- Create temp directories in the OS's standard temp location.
- Auto-delete (recursively) on Drop.
- Collision-resistant naming.
- Optional persistence (disable cleanup for debugging).
- Optional prefix for tracking purposes.

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
```

## 4. Determinism

- Directory creation MUST be idempotent in the sense that two
  concurrent `TempDir::new()` calls in the same process MUST NOT
  produce the same path (atomic counter guarantees this).
- Two processes MUST be extremely unlikely to produce the same path
  (PID + nanos varies).

## 5. Cleanup semantics

- `Drop` MUST attempt recursive deletion via `std::fs::remove_dir_all`.
- Deletion failures (file in use, permissions) MUST be silent —
  don't panic in Drop.
- `persist()` MUST disable cleanup so the directory survives drop.

## 6. Dependencies

This crate MUST NOT have runtime dependencies outside `std` in
`v0.1.0`. In `0.9.x`, optional dependencies on `mod-rand` (for
better naming) and `fsys` (for better filesystem primitives) may
be added behind feature flags.

## 7. Out of scope

- Cryptographically random naming (use `mod-rand::tier3` to generate
  the name and pass it as a prefix).
- File-locking primitives (not our domain).
- NFS / network filesystem edge cases (best-effort cleanup, document
  the limitation).

## 8. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API.
