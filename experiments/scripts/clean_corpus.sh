#!/bin/bash
set -euo pipefail

# Usage: ./clean_corpus.sh [ROOT_DIR] [LOGFILE]
# Defaults: ROOT_DIR="."  LOGFILE="${HOME}/corpus-clean.log"

ROOT="${1:-.}"
LOG="${2:-$HOME/corpus-clean.log}"

# ensure log dir exists
mkdir -p "$(dirname "$LOG")"

# Launch a low-priority background job that deletes every dir named "corpus"
# Note: we use ionice + nice + nohup + bash -lc to run the find+rm pipeline in a background shell.
ionice -c3 nice -n19 nohup bash -lc "find \"$ROOT\" -type d -name corpus -print0 | \
while IFS= read -r -d '' dir; do
  printf '%s DELETING: %s\n' \"\$(date --iso-8601=seconds)\" \"\$dir\" >> \"$LOG\"
  rm -rf -- \"\$dir\" || true
done" >/dev/null 2>&1 &

PID="$!"
echo "Launched background cleanup for: $ROOT"
echo "Log: $LOG"
echo "PID: $PID"