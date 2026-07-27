# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-07-27

### Fixed

- Keep committed compressed fixtures unchanged during normal checks so
  `cargo package` remains strict and reproducible across operating systems.
- Use Node 24-based GitHub checkout actions and remove unnecessary Node setup
  from jobs that consume committed fixtures.

## [0.1.0] - 2026-07-27

### Added

- Axum response layer for `rust-embed` SPA assets.
- Negotiation between identity, gzip, and Brotli representations.
- Strong SHA-256 ETags from `rust-embed` metadata.
- Cache policies for the SPA index, immutable assets, revalidated assets, and
  error responses.
- HTML-only SPA fallback and true static-asset 404 responses.
- Real Nginx proxy-cache integration harness.
- Agent-first integration instructions and acceptance checklist.

[Unreleased]: https://github.com/tomcatzh/embedded-spa/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/tomcatzh/embedded-spa/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/tomcatzh/embedded-spa/releases/tag/v0.1.0
