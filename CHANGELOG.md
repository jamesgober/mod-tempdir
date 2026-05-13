# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `NamedTempFile`: file-based companion type to `TempDir`. Same API
  shape (`new`, `with_prefix`, `path`, `persist`, `cleanup_on_drop`),
  same name-generation pipeline (`mod-rand` feature applies to both
  types), same silent best-effort Drop semantics. File creation is
  backed by `std::fs::File::create`; cleanup by
  `std::fs::remove_file`. Default basename pattern is
  `.tmpfile-{name}`, intentionally distinct from `TempDir`'s
  `.tmp-{name}` so an operator inspecting the OS temp dir can tell
  files and directories apart. Windows handle-lock behavior is
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

- `unique_name` graduated from `fn` to `pub(crate) fn` so the new
  `named_file` module can call the same generator. Internal change;
  no effect on the public API.
- Module-level rustdoc in `src/lib.rs` now introduces both
  `TempDir` and `NamedTempFile` and points at `NamedTempFile` for
  the Windows handle-lock note.
- `REPS.md` sections 2, 3, 4, 5 extended to cover `NamedTempFile`.
- README updated: `NamedTempFile` is now part of the quick start,
  the API listing, and the concurrency section. Default-basename
  table added.

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
- `NamedTempFile` companion type (planned for `0.9.1`)
- Cleanup-on-startup for orphaned dirs from crashed processes
  (planned for `0.9.2`)
- Windows file-lock retry logic

[Unreleased]: https://github.com/jamesgober/mod-tempdir/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/mod-tempdir/compare/v0.1.0...v0.9.0
[0.1.0]: https://github.com/jamesgober/mod-tempdir/releases/tag/v0.1.0
