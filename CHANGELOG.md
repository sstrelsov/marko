# Changelog

## 0.1.8

### Fixed
- Restore Homebrew formula name to `marko` after package rename

## 0.1.7

### Fixed
- Fix self-update archive path after package rename to `marko-md`

## 0.1.6

### Added
- Publish to crates.io as `marko-md` in CI release workflow
- Changelog-based release notes
- Pre-release changelog reminder in `make release`

### Changed
- Renamed package from `marko` to `marko-md` for crates.io (binary still called `marko`)
- Local `make release` no longer attempts crates.io publish

## 0.1.4

### Added
- Background update check and Makefile release targets

### Changed
- Replace hard-wrapping with visual soft-wrap

## [0.1.3](https://github.com/sstrelsov/marko/compare/v0.1.2...v0.1.3) (2026-02-19)


### Bug Fixes

* handle existing release in cargo-dist workflow ([f48c05f](https://github.com/sstrelsov/marko/commit/f48c05faab906ebaa90c2c3bca70fd9b3b7020f7))
* let cargo-dist own github release creation ([e6e9c9f](https://github.com/sstrelsov/marko/commit/e6e9c9f6a8cd10380d15aa96f821d3f4f4bf7f6c))

## [0.1.2](https://github.com/sstrelsov/marko/compare/v0.1.1...v0.1.2) (2026-02-18)


### Bug Fixes

* release archive format ([2a6ed3c](https://github.com/sstrelsov/marko/commit/2a6ed3c691ee5a8903bc011cf23e43bd398e655c))

## [0.1.1](https://github.com/sstrelsov/marko/compare/v0.1.0...v0.1.1) (2026-02-18)


### Bug Fixes

* scroll and page height ([#2](https://github.com/sstrelsov/marko/issues/2)) ([89b7479](https://github.com/sstrelsov/marko/commit/89b74792b9c5c0645b5db5e428a55ae273e5f36e))
