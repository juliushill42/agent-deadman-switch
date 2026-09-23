#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  pwd
)"

cd "$ROOT"

set -a
. ./.env
set +a

echo
echo "DEADMAN ADMIN TOKEN"
echo "$DEADMAN_ADMIN_TOKEN"
echo
echo "DEADMAN AGENT SHARED SECRET"
echo "$DEADMAN_AGENT_SHARED_SECRET"
echo
echo "Dashboard:"
echo "http://127.0.0.1:4176"
echo
echo "Control API:"
echo "http://127.0.0.1:8794"
echo
