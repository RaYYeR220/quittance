# Toolchain for the browser verifier: zcash_primitives pulls in libsecp256k1,
# which needs clang to compile for wasm32.
FROM rust:1.95-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends clang llvm \
 && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown \
 && cargo install wasm-bindgen-cli --version 0.2.129 --locked
WORKDIR /src
