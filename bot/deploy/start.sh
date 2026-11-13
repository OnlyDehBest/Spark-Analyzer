#!/usr/bin/env bash
cd "$(dirname "$0")"
set -a
. ./config.env
set +a
export DATA_DIR="${DATA_DIR:-$(pwd)/data}"
while true; do
  echo "[$(date '+%F %T')] avvio" | tee -a bot.log
  ./spark-resource-analyzer-bot 2>&1 | tee -a bot.log
  code=${PIPESTATUS[0]}
  echo "[$(date '+%F %T')] uscito con codice $code, riavvio tra 10 s (Ctrl+C per fermare)" | tee -a bot.log
  sleep 10
done
