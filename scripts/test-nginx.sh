#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd "$script_dir/.." && pwd)"
origin_port="${EMBEDDED_SPA_TEST_ORIGIN_PORT:-18090}"
nginx_port="${EMBEDDED_SPA_TEST_NGINX_PORT:-18091}"
nginx_image="${EMBEDDED_SPA_TEST_NGINX_IMAGE:-nginx:1.29.8-alpine}"
container_name="embedded-spa-nginx-test-$$"
test_dir="$(mktemp -d)"
origin_pid=""

cleanup() {
  docker rm -f "$container_name" >/dev/null 2>&1 || true

  if [[ -n "$origin_pid" ]]; then
    kill "$origin_pid" >/dev/null 2>&1 || true
    wait "$origin_pid" >/dev/null 2>&1 || true
  fi

  rm -rf "$test_dir"
}
trap cleanup EXIT

fail() {
  echo "nginx integration test failed: $*" >&2
  exit 1
}

wait_for_url() {
  local url="$1"
  local attempts=60

  while (( attempts > 0 )); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    attempts=$((attempts - 1))
    sleep 0.25
  done

  return 1
}

header_value() {
  local headers="$1"
  local name="$2"
  awk -v wanted="$name" '
    tolower($1) == tolower(wanted ":") {
      $1=""
      sub(/^ /, "")
      gsub(/\r/, "")
      print
      exit
    }
  ' "$headers"
}

request() {
  local path="$1"
  local encoding="$2"
  local label="$3"
  local extra_header="${4:-}"
  local args=(
    -sS
    -D "$test_dir/$label.headers"
    -o "$test_dir/$label.body"
    -H "Accept-Encoding: $encoding"
  )

  if [[ -n "$extra_header" ]]; then
    args+=(-H "$extra_header")
  fi

  curl "${args[@]}" "http://127.0.0.1:$nginx_port$path"
}

origin_request_count() {
  curl -fsS "http://127.0.0.1:$origin_port/_test/stats" |
    sed -E 's/.*"origin_requests":([0-9]+).*/\1/'
}

assert_origin_requests() {
  local expected="$1"
  local actual
  actual="$(origin_request_count)"
  [[ "$actual" == "$expected" ]] ||
    fail "expected $expected origin requests, got ${actual:-<invalid>}"
}

assert_status() {
  local headers="$1"
  local expected="$2"
  grep -qE "^HTTP/[0-9.]+ $expected([[:space:]]|$)" "$headers" ||
    fail "expected HTTP $expected in $headers"
}

assert_header() {
  local headers="$1"
  local name="$2"
  local expected_pattern="$3"
  local value
  value="$(header_value "$headers" "$name")"
  [[ "$value" =~ $expected_pattern ]] ||
    fail "expected $name to match $expected_pattern, got ${value:-<missing>}"
}

assert_no_header() {
  local headers="$1"
  local name="$2"
  [[ -z "$(header_value "$headers" "$name")" ]] ||
    fail "expected $name to be absent in $headers"
}

assert_header_starts_with() {
  local headers="$1"
  local name="$2"
  local expected="$3"
  local value
  value="$(header_value "$headers" "$name")"
  [[ "$value" == "$expected"* ]] ||
    fail "expected $name to start with $expected, got ${value:-<missing>}"
}

for required_command in brotli cargo curl docker gzip node; do
  command -v "$required_command" >/dev/null 2>&1 ||
    fail "required command is missing: $required_command"
done

cd "$project_root"

node scripts/precompress.mjs tests/fixtures/dist
cargo build --release --example nginx_fixture_server

EMBEDDED_SPA_TEST_BIND="0.0.0.0:$origin_port" \
  ./target/release/examples/nginx_fixture_server \
  >"$test_dir/origin.log" 2>&1 &
origin_pid="$!"

wait_for_url "http://127.0.0.1:$origin_port/_test/stats" ||
  fail "origin did not become ready"

sed "s/__ORIGIN_PORT__/$origin_port/g" \
  tests/nginx/nginx.conf.template >"$test_dir/nginx.conf"

if ! docker image inspect "$nginx_image" >/dev/null 2>&1; then
  docker pull "$nginx_image"
fi

docker run --detach --rm \
  --name "$container_name" \
  --add-host host.docker.internal:host-gateway \
  --publish "127.0.0.1:$nginx_port:8080" \
  --volume "$test_dir/nginx.conf:/etc/nginx/nginx.conf:ro" \
  "$nginx_image" >/dev/null

wait_for_url "http://127.0.0.1:$nginx_port/_test/stats" ||
  fail "nginx did not become ready"
assert_origin_requests 0

request "/" "identity" "index-identity-miss"
assert_status "$test_dir/index-identity-miss.headers" 200
assert_header "$test_dir/index-identity-miss.headers" "Content-Type" '^text/html'
assert_header "$test_dir/index-identity-miss.headers" "X-Cache-Status" '^MISS$'
assert_no_header "$test_dir/index-identity-miss.headers" "Content-Encoding"
assert_origin_requests 1

request "/" "identity" "index-identity-hit"
assert_header "$test_dir/index-identity-hit.headers" "X-Cache-Status" '^HIT$'
assert_origin_requests 1

request "/" "gzip" "index-gzip-miss"
assert_header "$test_dir/index-gzip-miss.headers" "X-Cache-Status" '^MISS$'
assert_header "$test_dir/index-gzip-miss.headers" "Content-Encoding" '^gzip$'
assert_origin_requests 2

request "/" "gzip" "index-gzip-hit"
assert_header "$test_dir/index-gzip-hit.headers" "X-Cache-Status" '^HIT$'
assert_origin_requests 2

request "/" "br" "index-br-miss"
assert_header "$test_dir/index-br-miss.headers" "X-Cache-Status" '^MISS$'
assert_header "$test_dir/index-br-miss.headers" "Content-Encoding" '^br$'
assert_origin_requests 3

request "/" "br" "index-br-hit"
assert_header "$test_dir/index-br-hit.headers" "X-Cache-Status" '^HIT$'
assert_origin_requests 3

identity_etag="$(header_value "$test_dir/index-identity-hit.headers" "ETag")"
gzip_etag="$(header_value "$test_dir/index-gzip-hit.headers" "ETag")"
br_etag="$(header_value "$test_dir/index-br-hit.headers" "ETag")"
[[ -n "$identity_etag" && -n "$gzip_etag" && -n "$br_etag" ]] ||
  fail "all representations must have ETags"
[[ "$identity_etag" != "$gzip_etag" ]] ||
  fail "identity and gzip ETags must differ"
[[ "$identity_etag" != "$br_etag" ]] ||
  fail "identity and br ETags must differ"
[[ "$gzip_etag" != "$br_etag" ]] ||
  fail "gzip and br ETags must differ"

request "/" "br" "index-br-conditional" "If-None-Match: $br_etag"
assert_status "$test_dir/index-br-conditional.headers" 304
assert_header "$test_dir/index-br-conditional.headers" "X-Cache-Status" '^HIT$'
[[ ! -s "$test_dir/index-br-conditional.body" ]] ||
  fail "304 response must not contain a body"
assert_origin_requests 3

sleep 2
request "/" "br" "index-br-revalidated"
assert_status "$test_dir/index-br-revalidated.headers" 200
assert_header "$test_dir/index-br-revalidated.headers" "X-Cache-Status" '^REVALIDATED$'
assert_origin_requests 4

mime_matrix="$test_dir/mime-matrix.tsv"
{
  printf '/\ttext/html\n'
  printf '/assets/app-a1b2c3.js\ttext/javascript\n'
  printf '/assets/app-a1b2c3.css\ttext/css\n'
  printf '/assets/sample-a1b2c3.json\tapplication/json\n'
  printf '/assets/sample-a1b2c3.svg\timage/svg+xml\n'
  printf '/assets/sample-a1b2c3.txt\ttext/plain\n'
  printf '/assets/sample-a1b2c3.webmanifest\tapplication/manifest+json\n'
  printf '/assets/sample-a1b2c3.xml\ttext/xml\n'
} >"$mime_matrix"

case_number=0
while IFS=$'\t' read -r path expected_mime; do
  case_number=$((case_number + 1))
  identity_label="mime-$case_number-identity"
  request "$path" "identity" "$identity_label"
  assert_status "$test_dir/$identity_label.headers" 200
  assert_header_starts_with "$test_dir/$identity_label.headers" "Content-Type" "$expected_mime"
  assert_header "$test_dir/$identity_label.headers" "Vary" '(^|,)[[:space:]]*Accept-Encoding([[:space:]]*,|$)'
  assert_no_header "$test_dir/$identity_label.headers" "Content-Encoding"

  for encoding in gzip br; do
    label="mime-$case_number-$encoding"
    request "$path" "$encoding" "$label"
    assert_status "$test_dir/$label.headers" 200
    assert_header_starts_with "$test_dir/$label.headers" "Content-Type" "$expected_mime"
    assert_header "$test_dir/$label.headers" "Content-Encoding" "^$encoding$"

    if [[ "$encoding" == "gzip" ]]; then
      gzip -dc "$test_dir/$label.body" >"$test_dir/$label.decoded"
    else
      brotli -d -c "$test_dir/$label.body" >"$test_dir/$label.decoded"
    fi

    cmp "$test_dir/$identity_label.body" "$test_dir/$label.decoded" ||
      fail "decoded $encoding body differs for $path"
  done
done <"$mime_matrix"

request "/assets/does-not-exist.js" "br" "missing-asset"
assert_status "$test_dir/missing-asset.headers" 404
assert_header "$test_dir/missing-asset.headers" "Cache-Control" '^no-store$'
assert_header "$test_dir/missing-asset.headers" "X-Cache-Status" '^MISS$'

request "/assets/does-not-exist.js" "br" "missing-asset-again"
assert_status "$test_dir/missing-asset-again.headers" 404
assert_header "$test_dir/missing-asset-again.headers" "X-Cache-Status" '^MISS$'

request "/rooms/ABCD" "br" "spa-fallback" "Accept: text/html"
assert_status "$test_dir/spa-fallback.headers" 200
assert_header "$test_dir/spa-fallback.headers" "Content-Type" '^text/html'

origin_requests="$(origin_request_count)"
[[ "$origin_requests" =~ ^[0-9]+$ ]] ||
  fail "could not read origin request count"

echo "nginx integration test passed"
echo "  image: $nginx_image"
echo "  MIME types: 8"
echo "  representations per MIME: identity, gzip, br"
echo "  cache states: MISS, HIT, REVALIDATED"
echo "  conditional response: 304 from fresh Nginx cache"
echo "  origin requests observed: $origin_requests"
