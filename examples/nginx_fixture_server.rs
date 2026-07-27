use std::{
    env,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use embedded_spa::{EmbeddedSpa, EmbeddedSpaConfig};
use rust_embed::RustEmbed;
use serde_json::json;
use tokio::net::TcpListener;

#[derive(RustEmbed)]
#[folder = "tests/fixtures/dist/"]
struct FixtureAssets;

#[tokio::main]
async fn main() {
    let bind = env::var("EMBEDDED_SPA_TEST_BIND").unwrap_or_else(|_| "0.0.0.0:18090".to_owned());
    let address: SocketAddr = bind
        .parse()
        .unwrap_or_else(|error| panic!("invalid test bind {bind:?}: {error}"));
    let request_count = Arc::new(AtomicU64::new(0));
    let spa = Arc::new(
        EmbeddedSpa::<FixtureAssets>::new(
            EmbeddedSpaConfig::default()
                .with_index_cache_control("public, max-age=0, s-maxage=1, must-revalidate"),
        )
        .expect("fixture SPA must be valid"),
    );
    let stats_count = Arc::clone(&request_count);
    let fallback_spa = Arc::clone(&spa);
    let fallback_count = Arc::clone(&request_count);
    let app = Router::new()
        .route("/_test/stats", get(move || stats(Arc::clone(&stats_count))))
        .fallback(move |request: Request| {
            let spa = Arc::clone(&fallback_spa);
            let request_count = Arc::clone(&fallback_count);
            async move {
                request_count.fetch_add(1, Ordering::Relaxed);
                spa.serve(request)
            }
        });
    let listener = TcpListener::bind(address)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {address}: {error}"));

    println!(
        "fixture listening on http://{address} with {} embedded files",
        spa.asset_count()
    );

    axum::serve(listener, app)
        .await
        .expect("fixture server failed");
}

async fn stats(request_count: Arc<AtomicU64>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

    (
        StatusCode::OK,
        headers,
        Json(json!({
            "origin_requests": request_count.load(Ordering::Relaxed),
        })),
    )
        .into_response()
}
