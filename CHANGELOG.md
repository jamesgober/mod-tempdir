# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- `fsys` integration for cross-platform filesystem ops
- `NamedTempFile` companion type
- Cleanup-on-startup for orphaned dirs from crashed processes
- Windows file-lock retry logic

[Unreleased]: https://github.com/jamesgober/mod-tempdir/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/mod-tempdir/compare/v0.1.0...v0.9.0
[0.1.0]: https://github.com/jamesgober/mod-tempdir/releases/tag/v0.1.0
