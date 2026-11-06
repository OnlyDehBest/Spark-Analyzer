$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
rustup target add wasm32-unknown-unknown
$meta = cargo metadata --format-version 1 | ConvertFrom-Json
$version = ($meta.packages | Where-Object { $_.name -eq "wasm-bindgen" } | Select-Object -First 1).version
$have = ""
if (Get-Command wasm-bindgen -ErrorAction SilentlyContinue) { $have = wasm-bindgen --version }
if ($have -notmatch [regex]::Escape($version)) { cargo install wasm-bindgen-cli --version $version --locked }
cargo build -p sparkscope-web --lib --release --target wasm32-unknown-unknown
if (Test-Path site) { Remove-Item -Recurse -Force site }
cargo run -p sparkscope-web --example build_site -- site
wasm-bindgen target/wasm32-unknown-unknown/release/sparkscope_web.wasm --target web --out-dir site/pkg --no-typescript
Write-Host "Sito pronto nella cartella site\"
