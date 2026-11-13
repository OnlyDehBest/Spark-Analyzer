#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-https://github.com/OnlyDehBest/Spark-Analyzer.git}"
BRANCH="${BRANCH:-main}"
SRC=/opt/spark-resource-analyzer-src
DIR=/opt/spark-resource-analyzer-bot

if [ "$(id -u)" -ne 0 ]; then
  echo "Lancia lo script come root"
  exit 1
fi

echo "==> Pacchetti di sistema"
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -q
  apt-get install -y -q git curl screen build-essential pkg-config libssl-dev ca-certificates
elif command -v dnf >/dev/null; then
  dnf install -y -q git curl screen gcc gcc-c++ make pkgconf-pkg-config openssl-devel ca-certificates
elif command -v yum >/dev/null; then
  yum install -y -q git curl screen gcc gcc-c++ make pkgconfig openssl-devel ca-certificates
else
  echo "Sistema non riconosciuto: installa git, curl, screen e gcc a mano"
  exit 1
fi

echo "==> Rust"
if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
fi
rustup update stable --no-self-update >/dev/null 2>&1 || true

echo "==> Codice ($BRANCH)"
if [ -d "$SRC/.git" ]; then
  git -C "$SRC" fetch -q origin "$BRANCH"
  git -C "$SRC" checkout -q -B "$BRANCH" "origin/$BRANCH"
else
  git clone -q --depth 1 --branch "$BRANCH" "$REPO" "$SRC"
fi

echo "==> Compilazione del bot (la prima volta richiede qualche minuto)"
cd "$SRC"
cargo build --release -p spark-resource-analyzer-bot

echo "==> Installazione in $DIR"
STAGE="$(mktemp -d)"
cp bot/deploy/install.sh bot/deploy/start.sh bot/deploy/config.env.example "$STAGE/"
cp target/release/spark-resource-analyzer-bot "$STAGE/"
mkdir -p "$DIR"
if [ ! -f "$DIR/config.env" ]; then
  install -m 600 "$STAGE/config.env.example" "$DIR/config.env"
fi

if ! grep -Eq '^DISCORD_TOKEN=.+' "$DIR/config.env"; then
  TOKEN="${DISCORD_TOKEN:-}"
  if [ -z "$TOKEN" ] && [ -r /dev/tty ]; then
    echo
    read -rsp "Incolla il token del bot (non viene mostrato) e premi Invio: " TOKEN < /dev/tty
    echo
  fi
  TOKEN="$(printf '%s' "$TOKEN" | tr -d '[:space:]')"
  if [ -z "$TOKEN" ]; then
    echo "Nessun token inserito. Mettilo in $DIR/config.env e rilancia questo comando."
    exit 1
  fi
  if ! printf '%s' "$TOKEN" | grep -Eq '^[A-Za-z0-9._-]+$'; then
    echo "Il token contiene caratteri non validi: ricopialo dal Developer Portal"
    exit 1
  fi
  awk -v t="$TOKEN" 'BEGIN{done=0} /^DISCORD_TOKEN=/{print "DISCORD_TOKEN=" t; done=1; next} {print} END{if(!done) print "DISCORD_TOKEN=" t}' \
    "$DIR/config.env" > "$DIR/config.env.new"
  install -m 600 "$DIR/config.env.new" "$DIR/config.env"
  rm -f "$DIR/config.env.new"
fi

bash "$STAGE/install.sh"
rm -rf "$STAGE"
