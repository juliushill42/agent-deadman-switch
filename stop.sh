#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")"
  pwd
)"

pkill -f \
  'target/release/deadman-control' \
  >/dev/null 2>&1 || true

pkill -f \
  'target/release/deadman-agent' \
  >/dev/null 2>&1 || true

pkill -f \
  'vite preview.*4176' \
  >/dev/null 2>&1 || true

"$ROOT/infra/kafka/stop.sh"
"$ROOT/infra/postgres/stop.sh"

echo \
  "Agent Deadman Switch stopped."
