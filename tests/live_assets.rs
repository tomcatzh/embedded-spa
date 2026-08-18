#![cfg(all(feature = "live-assets", debug_assertions))]

use std::{fs, path::PathBuf};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use embedded_spa::{EmbeddedSpa, EmbeddedSpaConfig};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "target/live-assets-test/"]
#[allow_missing = true]
struct LiveAssets;

struct FixtureDirectory {
    root: PathBuf,
}

impl FixtureDirectory {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/live-assets-test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("assets")).expect("live fixture directory must be writable");

        Self { root }
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        fs::write(self.root.join(path), bytes).expect("live fixture file must be writable");
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.root.join(path)).expect("live fixture file must be removable");
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("test request is valid")
}

#[tokio::test]
async fn one_spa_instance_tracks_disk_content_etags_and_file_membership() {
    let fixture = FixtureDirectory::new();
    fixture.write("index.html", b"version one");
    fixture.write("assets/app.js", b"console.log('one')");

    let spa = EmbeddedSpa::<LiveAssets>::new(EmbeddedSpaConfig::default())
        .expect("live fixture must contain index.html");
    assert_eq!(spa.asset_count(), 2);

    let first = spa.serve(request("/"));
    assert_eq!(first.status(), StatusCode::OK);
    let first_etag = first.headers()[header::ETAG].clone();
    assert_eq!(
        to_bytes(first.into_body(), usize::MAX).await.unwrap(),
        "version one"
    );

    fixture.write("index.html", b"version two");
    let changed = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::IF_NONE_MATCH, &first_etag)
            .body(Body::empty())
            .unwrap(),
    );
    assert_eq!(changed.status(), StatusCode::OK);
    let changed_etag = changed.headers()[header::ETAG].clone();
    assert_ne!(changed_etag, first_etag);
    assert_eq!(
        to_bytes(changed.into_body(), usize::MAX).await.unwrap(),
        "version two"
    );

    let unchanged = spa.serve(
        Request::builder()
            .uri("/")
            .header(header::IF_NONE_MATCH, &changed_etag)
            .body(Body::empty())
            .unwrap(),
    );
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
    assert!(
        to_bytes(unchanged.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );

    fixture.write("robots.txt", b"User-agent: *\nDisallow:");
    assert_eq!(spa.asset_count(), 3);
    let added = spa.serve(request("/robots.txt"));
    assert_eq!(added.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(added.into_body(), usize::MAX).await.unwrap(),
        "User-agent: *\nDisallow:"
    );

    fixture.remove("assets/app.js");
    assert_eq!(spa.asset_count(), 2);
    assert_eq!(
        spa.serve(request("/assets/app.js")).status(),
        StatusCode::NOT_FOUND
    );
}
