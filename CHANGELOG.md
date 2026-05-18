# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1] - 2026-05-18

### Changed

- Optional `mod-rand` dependency bumped from `0.9.5` to `1.0`.
  `mod-rand 1.0.0` is a strict superset of `0.9.5`; the
  `mod_rand::tier2::unique_name` function this crate uses is part
  of `mod-rand`'s 1.x SemVer-locked surface (see
  [mod-rand `docs/STABILITY.md`](https://github.com/jamesgober/mod-rand/blob/main/docs/STABILITY.md)).
  Naming output, alphabet, length, and Drop semantics are
  unchanged.

### Compatibility

- No public API change in `mod-tempdir`. Callers depending on
  `mod-tempdir 1.0.0` upgrade by version bump only; no source
  changes.
- Default builds (no `mod-rand` feature) are entirely unaffected —
  `mod-rand` is gated on the optional feature and the no-feature
  path still uses the built-in placeholder generator.
- MSRV stays at Rust `1.75`. `mod-rand 1.0` also pins to `1.75`
  with no MSRV bump.

## [1.0.0] - 2026-05-13

### Stable API declaration

`v1.0.0` is the trust handshake. The public API surface is
identical to `v0.9.3`: `TempDir`, `NamedTempFile`,
`PersistAtomicError`, and the top-level `cleanup_orphans` function.
No new features ship in this release; no signatures change. From
this version forward, breaking changes require a major-version
bump per SemVer.

### Stability commitment

- Breaking API changes require `2.0.0`.
- Additive surface (new methods, new free functions, new types)
  bumps the minor version.
- Bug fixes and doc-only changes bump the patch version.
- MSRV stays at Rust `1.75` within the `1.x` line; any MSRV change
  ships in a minor release with notice.
- The `mod-rand` feature flag is part of the stable surface.
- Default basename formats (`.tmp-{pid}-{name12}` for `TempDir`,
  `.tmpfile-{pid}-{name12}` for `NamedTempFile`) are part of the
  stable contract. `cleanup_orphans` parses them.

### Changed (vs. `v0.9.3` published artifact)

- README "Why a tempfile replacement" paragraph rewritten: the
  stale "no cleanup-on-startup pass" sentence is replaced with a
  pointer to the `cleanup_orphans` section. `cleanup_orphans`
  has been live since `v0.9.2`; the README text predated it.
- `PersistAtomicError` struct rustdoc gains an `# Example` block
  showing the destructured-recovery pattern (`Err(PersistAtomicError
  { error, file }) => ...`). The struct already had rustdoc; this
  satisfies DIRECTIVES section 4 (every public item carries at
  least one example).

Both deltas first landed as `1b890e9 chore: pre-release audit for
v0.9.3` and were carried on `main` past the `v0.9.3` tag; this is
the first crates.io / docs.rs cut that includes them.

## [0.9.3] - 2026-05-13

### Added

- `NamedTempFile::persist_atomic(target) -> Result<PathBuf, PersistAtomicError>`:
  atomically move the temp file to `target` with crash-safety
  guarantees. Performs `fsync` on the source, an atomic
  `std::fs::rename` (POSIX `rename(2)` / Windows `MoveFileExW` with
  `MOVEFILE_REPLACE_EXISTING`), and a best-effort `fsync` of the
  target's parent directory so the rename itself survives a crash.
  Atomic within a single filesystem; cross-filesystem persistence
  remains the caller's responsibility. **On failure, the temp file
  is preserved on disk and the original `NamedTempFile` is returned
  inside the error** so a retry path doesn't lose the source. This
  matches the `tempfile` crate's persist API convention.
- `PersistAtomicError { error: io::Error, file: NamedTempFile }`:
  the structured error type returned by `persist_atomic` on failure.
  Implements `Debug`, `Display`, `std::error::Error`, and a `From`
  conversion to `io::Error` for callers that don't need the
  recovered file.
- `tests/persist_atomic.rs`: four integration tests covering basic
  move + content preservation, replacement of an existing target,
  the data-integrity error path (target's parent missing: source
  survives, recovered file points at original temp path), and the
  post-success invariant that nothing remains at the original temp
  path.

### Changed

- README and REPS section 3 updated to include `persist_atomic` in
  the public API surface.

### Note on the deferred `fsys` integration

The ROADMAP reserved a possible `v0.9.3+` `fsys` integration for
atomic persistence. After auditing the `fsys` public API, the
integration was not taken: `fsys`'s atomic-rename primitive
(`fsys::platform::atomic_rename`) is `pub(crate)` and not callable
from outside the crate, and its public alternative (`Handle::rename`)
requires both paths to live under a single handle root, which does
not fit a generic `temp_dir → arbitrary_target` move. `std::fs::rename`
maps to the same OS primitives `fsys` uses internally (POSIX
`rename(2)` on Unix, `MoveFileExW` on Windows), so the `std`-only
path is functionally equivalent for this use case and keeps the
default zero-dep build intact. Same architectural call as the
retired `v0.9.1` fsys-for-directory-ops milestone: when `fsys`'s
value-add lives in its internals rather than its public surface,
adding the dep does not pay off.

## [0.9.2] - 2026-05-13

### Note on version numbering

This release bundles what was originally planned as two separate
milestones (`v0.9.1` introducing `NamedTempFile`, `v0.9.2` introducing
`cleanup_orphans`) into a single `0.9.2` cut. No `v0.9.1` tag was
published. `v0.9.0` shipped on 2026-05-13 with the `mod-rand`
integration; `v0.9.2` ships on the same date.

### Added

- `cleanup_orphans(max_age_hours: u64) -> io::Result<usize>`: a
  top-level free function that sweeps the OS temp directory for
  default-prefix entries this crate could have created and removes
  those that are both (a) older than `max_age_hours` and (b) owned
  by a process that is no longer alive. PID liveness is checked via
  `/proc/{pid}` on Linux; on macOS and Windows the function falls
  back to age-only because cross-platform process introspection
  requires platform crates this library does not pull in. Per-entry
  errors are silent; the function returns the count of successful
  removals. Caller-supplied `with_prefix(...)` paths and legacy
  entries from `0.9.0` / `0.9.1` (without a PID segment) are never
  touched.
- `tests/cleanup_orphans.rs`: six (seven on Linux) integration
  tests covering removal of dead-PID orphans, age-threshold
  preservation of recent entries, namespace isolation of custom
  prefixes, legacy-format skipping, and Linux-only live-process
  preservation.
- `NamedTempFile`: file-based companion type to `TempDir`. Same API
  shape (`new`, `with_prefix`, `path`, `persist`, `cleanup_on_drop`),
  same name-generation pipeline (`mod-rand` feature applies to both
  types), same silent best-effort Drop semantics. File creation is
  backed by `std::fs::File::create`; cleanup by
  `std::fs::remove_file`. Default basename pattern is
  `.tmpfile-{pid}-{name}`, intentionally distinct from `TempDir`'s
  `.tmp-{pid}-{name}` so an operator inspecting the OS temp dir can
  tell files and directories apart. Windows handle-lock behavior is
  documented in the type's rustdoc.
- `tests/named_file_smoke.rs`: happy-path coverage (creation,
  cleanup, persist, prefix, default-prefix shape, uniqueness, write
  via `OpenOptions` roundtrip, `cleanup_on_drop` defaults).
- `tests/named_file_concurrent.rs`: 256 threads call
  `NamedTempFile::new` through a synchronized barrier; resulting
  paths asserted unique. Runs on both feature configurations.
- `tests/named_file_windows_lock.rs` (`#[cfg(windows)]`): verifies
  `Drop` does not panic when a caller-held handle blocks
  `remove_file`. Cleanup is silent in that scenario, matching the
  REPS section 5 rule.
- `tests/coexistence.rs`: verifies `TempDir` and `NamedTempFile`
  coexist in the same scope without naming collision and that
  default basenames are visually distinguishable.
- `tests/mod_rand_integration.rs`: added a
  `NamedTempFile::new`-based end-to-end uniqueness check parallel
  to the existing `TempDir` one.

### Changed

- **Default basename format now includes the originating PID.**
  `TempDir::new` produces `.tmp-{pid}-{name12}` (was `.tmp-{name12}`
  in `0.9.0`). `NamedTempFile::new` produces
  `.tmpfile-{pid}-{name12}` (was `.tmpfile-{name12}` in the
  unreleased `0.9.1` work). The new segment is what
  `cleanup_orphans` parses to identify the owning process.
  `with_prefix` outputs are unchanged. Callers that pattern-match
  on the prefix (e.g., `starts_with(".tmp-")`) keep working;
  callers that asserted on the exact basename length or segment
  count would need to adapt.
- `unique_name` graduated from `fn` to `pub(crate) fn` so the new
  `named_file` and `cleanup` modules can call the same generator.
  Internal change; no effect on the public API.
- Module-level rustdoc in `src/lib.rs` now introduces both
  `TempDir` and `NamedTempFile` plus `cleanup_orphans`.
- `REPS.md` sections 2 and 3 extended to cover `NamedTempFile` and
  `cleanup_orphans`. The PID-encoded basename format is documented
  in section 3.
- README updated: quick start, API listing, default-basename
  table, "Cleaning up after crashes" section, and roadmap.

### Migration

Callers that asserted on the v0.9.0 / v0.9.1 default basename
shape need to know that `.tmp-{name12}` and `.tmpfile-{name12}`
have become `.tmp-{pid}-{name12}` and `.tmpfile-{pid}-{name12}`.
`starts_with(".tmp-")` and `starts_with(".tmpfile-")` continue to
work; tests that parsed segment counts or basename lengths do not.
`with_prefix(...)` output is unchanged. `cleanup_orphans` is
purely additive.

### Documentation

- Retired the planned `v0.9.1` "fsys integration" milestone. The
  `fsys` crate's directory operations are thin wrappers over
  `std::fs` plus path-resolution overhead. For single-syscall ops
  like `mkdir` and `rmdir_all` there is no library-level
  optimization headroom; `std::fs` IS the fast path. The previous
  v0.9.2 (`NamedTempFile`) becomes the new v0.9.1; the previous
  v0.9.3 (cleanup on startup) becomes the new v0.9.2.
  `ROADMAP.md`, `PROMPTS.md`, and `REPS.md` updated to reflect.
  A possible future milestone may revisit `fsys` integration for
  atomic file persistence in `NamedTempFile::persist_atomic()`,
  where `fsys`'s atomic-write primitives genuinely add value.

## [0.9.0] - 2026-05-13

### Added

- Optional `mod-rand` feature flag (off by default). When enabled,
  directory naming is delegated to `mod_rand::tier2::unique_name`,
  which produces a uniformly distributed Crockford base32 name from a
  SplitMix + Stafford-finisher pipeline. Default builds remain free of
  any runtime dependency outside `std`.
- `tests/mod_rand_integration.rs`: exercises the feature-enabled name
  generator with a 10,000-sample uniqueness check, an alphabet
  distribution check, and a 100-iteration end-to-end check that goes
  through `TempDir::new()`.
- `tests/concurrent_create.rs`: 256 threads call `TempDir::new()` from
  a synchronized barrier and the resulting paths are asserted unique.
  Runs on both feature configurations.
- `#[doc(hidden)] pub fn __unique_name_for_tests`: internal test hook
  compiled only with the `mod-rand` feature. Not part of the stable
  public API; external code must not depend on it.

### Changed

- Module-level rustdoc in `src/lib.rs` now documents the new feature
  flag, the cleanup semantics, and the upgrade path.
- README updated to describe the `mod-rand` feature, the upgrade
  path, and the current state of the roadmap.
- Crate description in `Cargo.toml` updated to reflect the new
  default-vs-opt-in dependency stance.

### Migration

The public API is unchanged from `0.1.0`. No code changes are needed
to move from `0.1.x` to `0.9.0`. To opt into uniform random naming,
enable the feature in your `Cargo.toml`:

    mod-tempdir = { version = "0.9", features = ["mod-rand"] }

The naming alphabet (Crockford base32) is identical on both paths, so
any caller pattern matching on the directory basename keeps working
unchanged when the feature is toggled.

## [0.1.0] - 2026-05-11

### Added

- Initial crate skeleton.
- `TempDir` struct with `new`, `with_prefix`, `path`, `persist`,
  `cleanup_on_drop` methods.
- Automatic recursive deletion on `Drop`.
- Placeholder name generation (PID + nanos + counter).
- Smoke tests covering creation, cleanup, persist, prefix, uniqueness.

### Note

This is the name-claim release. Real implementations land in `0.9.x`:

- `mod-rand::tier2` integration for collision-resistant naming
  (shipped in `0.9.0`)
- `NamedTempFile` companion type (shipped in `0.9.2`)
- Cleanup-on-startup for orphaned dirs from crashed processes
  (shipped in `0.9.2`)
- Windows file-lock retry logic

[Unreleased]: https://github.com/jamesgober/mod-tempdir/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/jamesgober/mod-tempdir/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/jamesgober/mod-tempdir/compare/v0.9.3...v1.0.0
[0.9.3]: https://github.com/jamesgober/mod-tempdir/compare/v0.9.2...v0.9.3
[0.9.2]: https://github.com/jamesgober/mod-tempdir/compare/v0.9.0...v0.9.2
[0.9.0]: https://github.com/jamesgober/mod-tempdir/compare/v0.1.0...v0.9.0
[0.1.0]: https://github.com/jamesgober/mod-tempdir/releases/tag/v0.1.0
