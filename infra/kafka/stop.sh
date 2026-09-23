#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  pwd
)"

PIDFILE="$ROOT/.runtime/kafka/kafka.pid"

if [ -f "$PIDFILE" ]; then
  PID="$(cat "$PIDFILE")"

  kill "$PID" \
    >/dev/null 2>&1 || true

  rm -f "$PIDFILE"
fi
