#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown
version=$(cargo metadata --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="wasm-bindgen"))')
command -v wasm-bindgen >/dev/null && wasm-bindgen --version | grep -q "$version" || cargo install wasm-bindgen-cli --version "$version" --locked
cargo build -p sparkscope-web --lib --release --target wasm32-unknown-unknown
rm -rf site
cargo run -p sparkscope-web --example build_site -- site
wasm-bindgen target/wasm32-unknown-unknown/release/sparkscope_web.wasm --target web --out-dir site/pkg --no-typescript
echo "Sito pronto nella cartella site/"
