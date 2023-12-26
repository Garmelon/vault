# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

Procedure when bumping the version number:

1. Update dependencies in a separate commit
2. Set version number in `Cargo.toml`
3. Add new section in this changelog
4. Commit with message `Bump version to X.Y.Z`
5. Create tag named `vX.Y.Z`
6. Push `master` and the new tag

## Unreleased

## v0.3.0 - 2023-12-26

### Changed

- **(breaking)** Bumped `rusqlite` dependency from `0.29` to `0.30`

## v0.2.0 - 2023-05-14

### Added

- `serde` feature
- `serde::from_row_via_index`
- `serde::from_row_via_name`

### Changed

- **(breaking)**
  Error handling of `Action`s is now more complex but more powerful. In
  particular, `Action`s can now return almost arbitrary errors without nesting
  `Result`s like before.
- **(breaking)** Renamed `Action::Result` to `Action::Output`
- **(breaking)** Bumped `rusqlite` dependency from `0.28` to `0.29`

## v0.1.0 - 2023-02-12

Initial release
