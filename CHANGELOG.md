# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.2-alpha] - 2026-04-19
### Changed
- make path mapping segment-safe to avoid false matches on similar prefixes
- distinguish real missing files from other stat/protocol errors during diffing
- keep non-not-found errors visible instead of treating them as missing files
- move sync decision logic from `diffview` into `internal/sync`
- process hosts and marked paths deterministically for more stable behavior

## [0.1.1-alpha] - 2026-04-18
### Changed
- improve drift version output for builds installed via `go install`
- keep showing injected versions for release builds
- fall back to Go build metadata for tagged installs
