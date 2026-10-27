#!/bin/sh
# Builds the browser verifier into web/public/wasm (or $1).
set -eu
cd "$(dirname "$0")/.."
OUT="${1:-web/public/wasm}"
docker build -q -t quittance-wasm-build -f scripts/wasm.Dockerfile scripts >/dev/null
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$(pwd):/src" \
  -v quittance-cargo-registry:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/src/target-wasm-linux \
  -e CC_wasm32_unknown_unknown=clang \
  -e AR_wasm32_unknown_unknown=llvm-ar \
  -e RUSTFLAGS='--cfg getrandom_backend="wasm_js"'   -e CARGO_PROFILE_RELEASE_LTO=fat   -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1   -e CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
  quittance-wasm-build sh -c "
    cargo build -p quittance-wasm --target wasm32-unknown-unknown --release &&
    wasm-bindgen --target web --out-dir /src/$OUT --out-name quittance \
      /src/target-wasm-linux/wasm32-unknown-unknown/release/quittance_wasm.wasm"
ls -la "$OUT"
