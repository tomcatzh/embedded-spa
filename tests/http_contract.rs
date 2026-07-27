use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use embedded_spa::{EmbeddedSpa, EmbeddedSpaConfig, EmbeddedSpaError};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "tests/fixtures/dist/"]
struct FixtureAssets;

fn spa() -> EmbeddedSpa<FixtureAssets> {
    EmbeddedSpa::new(EmbeddedSpaConfig::default()).expect("fixture must contain index.html")
}

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("test request is valid")
}

#[tokio::test]
async fn identity_gzip_and_brotli_have_distinct_strong_etags() {
    let spa = spa();
    let identity = spa.serve(request("/"));
    let gzip = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "gzip")
            .body(Body::empty())
            .unwrap(),
    );
    let brotli = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "br")
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(identity.status(), StatusCode::OK);
    assert_eq!(gzip.status(), StatusCode::OK);
    assert_eq!(brotli.status(), StatusCode::OK);
    assert_eq!(gzip.headers()[header::CONTENT_ENCODING], "gzip");
    assert_eq!(brotli.headers()[header::CONTENT_ENCODING], "br");
    assert_eq!(identity.headers()[header::VARY], "Accept-Encoding");

    let identity_etag = &identity.headers()[header::ETAG];
    let gzip_etag = &gzip.headers()[header::ETAG];
    let brotli_etag = &brotli.headers()[header::ETAG];
    assert_ne!(identity_etag, gzip_etag);
    assert_ne!(identity_etag, brotli_etag);
    assert_ne!(gzip_etag, brotli_etag);
    assert!(!identity_etag.to_str().unwrap().starts_with("W/"));
}

#[tokio::test]
async fn repeated_accept_encoding_fields_select_the_best_representation() {
    let response = spa().serve(
        Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "gzip;q=0.4")
            .header(header::ACCEPT_ENCODING, "br;q=1")
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_ENCODING], "br");
}

#[tokio::test]
async fn invalid_quality_values_are_not_clamped_into_acceptance() {
    let response = spa().serve(
        Request::builder()
            .uri("/")
            .header(
                header::ACCEPT_ENCODING,
                "identity;q=0, gzip;q=0.5, br;q=1.0000",
            )
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
}

#[tokio::test]
async fn matching_validator_returns_bodyless_304() {
    let spa = spa();
    let first = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "br")
            .body(Body::empty())
            .unwrap(),
    );
    let etag = first.headers()[header::ETAG].clone();
    let conditional = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::ACCEPT_ENCODING, "br")
            .header(header::IF_NONE_MATCH, etag)
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(conditional.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(conditional.headers()[header::CONTENT_ENCODING], "br");
    assert!(
        to_bytes(conditional.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn html_navigation_falls_back_but_missing_asset_does_not() {
    let spa = spa();
    let navigation = spa.serve(
        Request::builder()
            .uri("/rooms/ABCD")
            .header(header::ACCEPT, "text/html")
            .body(Body::empty())
            .unwrap(),
    );
    let missing_asset = spa.serve(
        Request::builder()
            .uri("/assets/missing.js")
            .header(header::ACCEPT, "text/html")
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(navigation.status(), StatusCode::OK);
    assert_eq!(navigation.headers()[header::CONTENT_TYPE], "text/html");
    assert_eq!(
        navigation.headers()[header::CACHE_CONTROL],
        "public, max-age=0, s-maxage=60, must-revalidate"
    );
    assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing_asset.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn repeated_accept_fields_can_enable_html_fallback() {
    let response = spa().serve(
        Request::builder()
            .uri("/rooms/ABCD")
            .header(header::ACCEPT, "application/json")
            .header(header::ACCEPT, "text/html")
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/html");
}

#[tokio::test]
async fn invalid_html_quality_does_not_enable_fallback() {
    let response = spa().serve(
        Request::builder()
            .uri("/rooms/ABCD")
            .header(header::ACCEPT, "text/html;q=0.9999")
            .body(Body::empty())
            .unwrap(),
    );

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn direct_compressed_paths_are_hidden() {
    let response = spa().serve(request("/index.html.br"));

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn head_preserves_representation_length_without_a_body() {
    let mut request = request("/assets/app-a1b2c3.js");
    *request.method_mut() = Method::HEAD;
    request
        .headers_mut()
        .insert(header::ACCEPT_ENCODING, "gzip".parse().unwrap());
    let response = spa().serve(request);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
    assert!(response.headers().contains_key(header::CONTENT_LENGTH));
    assert!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_index_path_fails_at_construction() {
    let error = EmbeddedSpa::<FixtureAssets>::new(EmbeddedSpaConfig {
        index_path: "../index.html".to_owned(),
        ..EmbeddedSpaConfig::default()
    })
    .err()
    .expect("unsafe index must fail");

    assert!(matches!(error, EmbeddedSpaError::InvalidIndexPath(_)));
}
