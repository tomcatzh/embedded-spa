use std::{env, net::SocketAddr, sync::LazyLock};

use axum::{Json, Router, extract::Request, http::StatusCode, response::Response, routing::get};
use embedded_spa::{EmbeddedSpa, EmbeddedSpaConfig};
use rust_embed::RustEmbed;
use serde_json::json;
use tokio::net::TcpListener;

#[derive(RustEmbed)]
#[folder = "examples/public/"]
struct Assets;

static SPA: LazyLock<EmbeddedSpa<Assets>> = LazyLock::new(|| {
    EmbeddedSpa::new(EmbeddedSpaConfig::default()).expect("examples/public must contain index.html")
});

#[tokio::main]
async fn main() {
    let bind =
        env::var("EMBEDDED_SPA_EXAMPLE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let address: SocketAddr = bind
        .parse()
        .unwrap_or_else(|error| panic!("invalid bind address {bind:?}: {error}"));
    let api = Router::new()
        .route("/health", get(|| async { Json(json!({ "ok": true })) }))
        .fallback(|| async { StatusCode::NOT_FOUND });
    let app = Router::new()
        .nest("/api", api)
        .fallback(|request: Request| async move { serve_spa(request) });
    let listener = TcpListener::bind(address)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {address}: {error}"));

    println!("minimal example listening on http://{address}");
    axum::serve(listener, app).await.expect("server failed");
}

fn serve_spa(request: Request) -> Response {
    SPA.serve(request)
}
