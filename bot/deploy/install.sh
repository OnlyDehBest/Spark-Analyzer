#!/usr/bin/env bash
set -euo pipefail
DIR=/opt/spark-resource-analyzer-bot
SCREEN=sra-bot
cd "$(dirname "$0")"

if ! command -v screen >/dev/null; then
  echo "Installo screen…"
  if command -v apt-get >/dev/null; then apt-get update -q && apt-get install -y -q screen
  elif command -v dnf >/dev/null; then dnf install -y -q screen
  elif command -v yum >/dev/null; then yum install -y -q screen
  else echo "Installa screen a mano e rilancia lo script"; exit 1; fi
fi

mkdir -p "$DIR/data"
install -m 755 spark-resource-analyzer-bot "$DIR/spark-resource-analyzer-bot"
install -m 755 start.sh "$DIR/start.sh"
if [ ! -f "$DIR/config.env" ]; then
  install -m 600 config.env.example "$DIR/config.env"
fi
chmod 600 "$DIR/config.env"

if ! grep -Eq '^DISCORD_TOKEN=.+' "$DIR/config.env"; then
  echo
  echo "Installato in $DIR, ma manca il token del bot."
  echo "1) nano $DIR/config.env   e incolla il token dopo DISCORD_TOKEN="
  echo "2) bash $(pwd)/install.sh  per avviarlo"
  exit 0
fi

if screen -list | grep -q "\.$SCREEN"; then
  echo "Riavvio il bot…"
  screen -S "$SCREEN" -X quit || true
  sleep 1
fi
screen -dmS "$SCREEN" "$DIR/start.sh"
sleep 3
if screen -list | grep -q "\.$SCREEN"; then
  echo "Bot avviato nello screen «$SCREEN»."
  echo "Vedere i log:   screen -r $SCREEN   (uscire senza fermarlo: Ctrl+A poi D)"
  echo "Ultime righe:   tail -n 30 $DIR/bot.log"
else
  echo "Il bot si è fermato subito. Ultime righe del log:"
  tail -n 30 "$DIR/bot.log" || true
  exit 1
fi
