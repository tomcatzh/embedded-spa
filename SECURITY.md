# Security policy

## Supported versions

Until the project reaches 1.0, only the latest tagged release receives fixes.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose embedded
secrets, bypass routing boundaries, poison shared caches, or serve attacker
controlled bytes with trusted headers.

Use GitHub's private vulnerability reporting for this repository. Include:

- the affected version or commit;
- a minimal reproduction;
- expected and observed HTTP responses;
- relevant proxy or CDN configuration;
- whether the issue reproduces without an intermediary.

This crate must never be used to embed secrets. Every embedded asset is
downloadable by a client that can reach the service.
