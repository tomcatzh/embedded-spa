# Contributing

Small, contract-preserving changes are welcome.

Before opening a pull request:

```bash
make check
make test-nginx
```

Changes to ETags, cache directives, content negotiation, MIME behavior, or SPA
fallback rules must include a regression test. Do not weaken a strong ETag
after transforming response bytes; add a distinct representation instead.

Keep the public API small. A new configuration option should express a stable
HTTP policy, not an application-specific routing convention.
