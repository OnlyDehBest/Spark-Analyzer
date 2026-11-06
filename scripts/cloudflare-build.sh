#!/usr/bin/env bash
set -euo pipefail
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
fi
exec "$(dirname "$0")/build-site.sh"
