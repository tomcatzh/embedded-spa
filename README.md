# embedded-spa

[![CI](https://github.com/tomcatzh/embedded-spa/actions/workflows/ci.yml/badge.svg)](https://github.com/tomcatzh/embedded-spa/actions/workflows/ci.yml)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Serve a Vite-style single-page application from one Axum executable with
cache-correct identity, gzip, and Brotli representations.

`embedded-spa` is intentionally small. It does not build your frontend, own
your API router, compress bytes at request time, or require a particular
JavaScript framework. It turns immutable files supplied by
[`rust-embed`](https://github.com/pyrossh/rust-embed) into correct Axum
responses.

> 给人类维护者：这是一个 MIT 独立项目。README 以 coding agent 能直接复制、
> 验证和排错为优先；核心 HTTP 契约和生产注意事项也完整记录在下面。

## Coding agent: start here

If you are integrating this crate into an existing Rust + SPA application,
perform these steps in order:

1. Confirm that the frontend produces a static output directory containing
   `index.html`.
2. Generate `.gz` and `.br` siblings after the frontend production build.
3. Add `embedded-spa`, `rust-embed`, and Axum dependencies.
4. Derive `RustEmbed` for the output directory.
5. Mount all API routes before the SPA fallback.
6. Construct one long-lived `EmbeddedSpa`; do not reconstruct it per request.
7. Run the direct HTTP checks.
8. If Nginx or another shared cache is in front, run the real-proxy checks.

Use this Git dependency until a crates.io release exists:

```toml
[dependencies]
axum = "0.8"
embedded-spa = { git = "https://github.com/tomcatzh/embedded-spa", tag = "v0.1.1" }
rust-embed = { version = "8", features = ["deterministic-timestamps"] }
tokio = { version = "1", features = ["macros", "net", "rt-multi-thread"] }
```

Equivalent command:

```bash
cargo add embedded-spa --git https://github.com/tomcatzh/embedded-spa --tag v0.1.1
```

Do not use the repository's Nginx example as a substitute for mounting API
routes correctly. `/api/*` must be handled before the SPA fallback.

## What this crate guarantees

- Only `GET` and `HEAD` serve embedded content.
- Existing files are served exactly; direct requests for `.gz` and `.br`
  sibling paths are hidden.
- A missing immutable asset returns a real `404`, never `index.html`.
- SPA fallback occurs only when `Accept` explicitly allows `text/html`.
- `Accept-Encoding` q-values select identity, gzip, or Brotli.
- Every final representation receives its own strong SHA-256 `ETag`.
- ETags use `rust-embed` metadata and are formatted once during construction;
  request handling does not hash content.
- Files with compressed siblings emit `Vary: Accept-Encoding`.
- MIME is inferred from the logical filename, not from `.gz` or `.br`.
- `index.html`, immutable assets, revalidated assets, and errors have separate
  cache policies.
- Invalid configured header values fail at startup.

## What this crate does not do

- It does not run Vite, npm, pnpm, Bun, or another frontend build.
- It does not compress responses at runtime.
- It does not implement API routes, authentication, WebSockets, or sessions.
- It does not configure TLS, Nginx, a CDN, or a service worker.
- It does not preserve old hashed chunks across deployments.
- It does not make embedded secrets safe. Embedded bytes are public assets.

## Minimal Axum integration

Assume Vite writes:

```text
frontend/dist/
├── index.html
├── index.html.br
├── index.html.gz
└── assets/
    ├── app-a1b2c3.js
    ├── app-a1b2c3.js.br
    ├── app-a1b2c3.js.gz
    ├── app-d4e5f6.css
    ├── app-d4e5f6.css.br
    └── app-d4e5f6.css.gz
```

Wire it into Axum:

```rust
use std::sync::LazyLock;

use axum::{
    Json, Router,
    extract::Request,
    http::StatusCode,
    response::Response,
    routing::get,
};
use embedded_spa::{EmbeddedSpa, EmbeddedSpaConfig};
use rust_embed::RustEmbed;
use serde_json::json;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Assets;

static SPA: LazyLock<EmbeddedSpa<Assets>> = LazyLock::new(|| {
    EmbeddedSpa::new(EmbeddedSpaConfig::default())
        .expect("frontend/dist must contain a valid index.html")
});

async fn serve_spa(request: Request) -> Response {
    SPA.serve(request)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

fn app() -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .fallback(|| async { StatusCode::NOT_FOUND });

    Router::new()
        .nest("/api", api)
        .fallback(serve_spa)
}
```

The nested API fallback is deliberate. A missing API route must not inherit the
HTML fallback.

### Debug builds

By default, `rust-embed` reads from the filesystem in debug builds and embeds
files in release builds. Add its `debug-embed` feature if debug executables
must also be self-contained:

```toml
rust-embed = {
  version = "8",
  features = ["debug-embed", "deterministic-timestamps"]
}
```

Production artifacts should still be built with `cargo build --release`.

## Build-time precompression

This repository includes a framework-independent Node script:

```bash
node scripts/precompress.mjs frontend/dist
```

It creates Brotli quality-11 and gzip level-9 siblings for:

```text
.css .html .js .json .svg .txt .webmanifest .xml
```

Run it after every frontend production build and before `cargo build
--release`.

Example package script:

```json
{
  "scripts": {
    "build": "vite build && node ../scripts/precompress.mjs dist"
  }
}
```

The script is a reference implementation, not a runtime dependency of this
crate. A Rust build tool, CDN pipeline, or another compressor is fine as long
as the final sibling filenames follow:

```text
logical/path.ext
logical/path.ext.gz
logical/path.ext.br
```

## HTTP contract

Default cache policy:

| Response class | `Cache-Control` |
|---|---|
| `index.html` and SPA fallback | `public, max-age=0, s-maxage=60, must-revalidate` |
| paths below `assets/` | `public, max-age=31536000, immutable` |
| other existing static files | `public, max-age=0, must-revalidate` |
| `404`, `405`, `406`, internal static errors | `no-store` |

Representation headers:

| Condition | Result |
|---|---|
| no usable compressed sibling | identity bytes |
| `Accept-Encoding: gzip` | `.gz` bytes + `Content-Encoding: gzip` |
| `Accept-Encoding: br` | `.br` bytes + `Content-Encoding: br` |
| compressed siblings exist | `Vary: Accept-Encoding` |
| matching `If-None-Match` | `304` with no body |
| no acceptable representation | `406` |

`ETag` identifies the bytes actually transferred. Identity, gzip, and Brotli
must therefore have different strong ETags. Never dynamically recompress a
response while preserving the original strong ETag.

### SPA fallback rule

Fallback is intentionally strict:

```text
exact file exists                         -> serve it
missing immutable asset                  -> 404
missing path + Accept includes text/html -> index.html
everything else                          -> 404
```

This prevents a missing JavaScript module, image, manifest, or API request from
receiving HTML with status 200.

## Configuration

Start from `EmbeddedSpaConfig::default()`:

```rust
let config = EmbeddedSpaConfig {
    index_path: "index.html".to_owned(),
    immutable_prefixes: vec!["assets/".to_owned()],
    index_cache_control:
        "public, max-age=0, s-maxage=30, must-revalidate".to_owned(),
    immutable_cache_control:
        "public, max-age=31536000, immutable".to_owned(),
    revalidate_cache_control:
        "public, max-age=0, must-revalidate".to_owned(),
    error_cache_control: "no-store".to_owned(),
    content_security_policy: Some(
        "default-src 'self'; object-src 'none'; base-uri 'none'".to_owned(),
    ),
};

let spa = EmbeddedSpa::<Assets>::new(config)?;
```

Convenience builders currently exist for the two values most often varied by
tests and applications:

```rust
let config = EmbeddedSpaConfig::default()
    .with_index_cache_control(
        "public, max-age=0, s-maxage=10, must-revalidate",
    )
    .with_content_security_policy(None::<String>);
```

Construction validates all configured response header values and verifies that
the index exists.

## Nginx in front

Let Rust remain the source of truth for `ETag`, `Cache-Control`,
`Content-Encoding`, and `Vary`. Nginx should cache and revalidate those
responses, not transform them.

```nginx
proxy_cache_path /var/cache/nginx/app
    levels=1:2
    keys_zone=app_static:10m
    max_size=256m
    inactive=7d
    use_temp_path=off;

server {
    listen 443 ssl;
    server_name game.example.com;

    location ^~ /api/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_cache off;
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # Rust already selected final precompressed bytes and their ETag.
        gzip off;

        proxy_cache app_static;
        proxy_cache_revalidate on;
        proxy_cache_lock on;
        proxy_cache_use_stale error timeout updating
            http_500 http_502 http_503 http_504;

        # Keep during rollout diagnostics; remove when no longer needed.
        add_header X-Cache-Status $upstream_cache_status always;
    }
}
```

Expected flow:

```text
first request                 Nginx MISS -> Rust 200 -> cache
same fresh representation     Nginx HIT  -> Rust receives nothing
fresh + matching validator    Nginx HIT  -> local 304
stale cached representation   Nginx sends If-None-Match to Rust
unchanged Rust response       Rust 304 -> Nginx REVALIDATED -> cached body
```

Do not enable Nginx dynamic gzip for these responses. If an intermediary
changes the representation bytes, it must also change or weaken the validator.

## Verification

Fast checks:

```bash
make check
```

This runs fixture generation, formatting, all Rust targets, Clippy with
warnings denied, and rustdoc.

Real Nginx integration:

```bash
make test-nginx
```

Requirements:

- Docker;
- `curl`;
- `gzip`;
- `brotli`;
- Rust stable.

Node.js is required only when using the optional `scripts/precompress.mjs`
helper to generate representations, not when running the committed test matrix.

The integration test starts an official Nginx container and a release fixture
origin. It verifies:

- eight MIME types;
- identity, gzip, and Brotli for every MIME type;
- decoded-body equality;
- distinct ETags per representation;
- `MISS`, `HIT`, local `304`, and `REVALIDATED`;
- origin request counts proving cache hits do not reach Rust;
- HTML SPA fallback;
- repeated missing-asset requests remain uncached.

## Acceptance checklist for coding agents

Do not report an integration complete until all applicable items pass:

- [ ] The frontend production build creates `index.html`.
- [ ] Precompression runs before the Rust release build.
- [ ] `index.html`, `.gz`, and `.br` are present in embedded assets.
- [ ] API routers have their own 404 and are mounted before SPA fallback.
- [ ] A missing hashed asset returns 404, not HTML.
- [ ] `curl -H 'Accept-Encoding: br'` returns `Content-Encoding: br`.
- [ ] identity, gzip, and Brotli ETags differ.
- [ ] Matching `If-None-Match` returns 304 with an empty body.
- [ ] Nginx shows `MISS` then `HIT`.
- [ ] A fresh Nginx `HIT` does not increment origin request count.
- [ ] A stale entry becomes `REVALIDATED`.
- [ ] `cargo build --release` runs without the frontend directory at runtime.

## Troubleshooting

### JavaScript request returns HTML

The API or asset request reached SPA fallback. Mount API routes before the
fallback and keep hashed assets under an immutable prefix such as `assets/`.

### Brotli is never selected

Confirm that `file.ext.br` was present when the Rust executable was compiled.
Also inspect the actual `Accept-Encoding` header received by Rust.

### Nginx always reports `MISS`

Check the upstream `Cache-Control`, ensure `proxy_cache` is enabled for that
location, and verify the cache directory is writable. `no-store`, `private`,
and some zero-freshness configurations intentionally prevent Nginx storage.

### ETags match across encodings

The compressed siblings were probably not embedded, or the application is
reusing one semantic version string as a strong ETag. Each final byte sequence
needs its own validator.

### Old lazy-loaded chunk returns 404 after deployment

ETag cannot solve a removed URL. Retain previous hashed assets for an overlap
window, or store immutable chunks in versioned object storage/CDN.

## Public API

- `EmbeddedSpa<A>`: validated, reusable response service for a `RustEmbed`
  provider.
- `EmbeddedSpa::new`: verifies the index and precomputes ETag header values.
- `EmbeddedSpa::serve`: synchronously converts an Axum request into a response.
- `EmbeddedSpa::asset_count`: reports embedded files including compressed
  siblings.
- `EmbeddedSpaConfig`: index path, immutable prefixes, cache policies, and CSP.
- `EmbeddedSpaError`: startup configuration failure.

Generate local API documentation:

```bash
cargo doc --no-deps --open
```

## Design constraints

- Rust 1.85 or newer.
- Axum 0.8.
- `rust-embed` 8.
- No unsafe code.
- No runtime hashing.
- No runtime compression.
- No filesystem access in release builds.
- No application-specific API behavior.

## Versioning

Before 1.0, minor versions may adjust public configuration or response-policy
details. Pin a Git tag or commit in production and read
[CHANGELOG.md](CHANGELOG.md) before upgrading.

## License

MIT. See [LICENSE](LICENSE).
