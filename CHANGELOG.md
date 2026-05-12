# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/jamesgober/mod-tempdir/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jamesgober/mod-tempdir/releases/tag/v0.1.0
