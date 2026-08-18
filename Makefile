.PHONY: fixtures fmt check test test-nginx package

fixtures:
	node scripts/precompress.mjs tests/fixtures/dist

fmt:
	cargo fmt --all

check:
	cargo fmt --all -- --check
	cargo test --all-targets
	cargo test --all-targets --all-features
	cargo clippy --all-targets -- -D warnings
	cargo clippy --all-targets --all-features -- -D warnings
	cargo doc --no-deps

test:
	cargo test --all-targets
	cargo test --all-targets --all-features

test-nginx:
	./scripts/test-nginx.sh

package: check
	cargo package
