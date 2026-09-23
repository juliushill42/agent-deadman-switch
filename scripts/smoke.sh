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

curl \
  -fsS \
  http://127.0.0.1:8794/healthz \
| jq .

curl \
  -fsS \
  -H \
  "Authorization: Bearer $DEADMAN_ADMIN_TOKEN" \
  http://127.0.0.1:8794/api/v1/agents \
| jq .

curl \
  -fsS \
  -H \
  "Authorization: Bearer $DEADMAN_ADMIN_TOKEN" \
  "http://127.0.0.1:8794/api/v1/events?limit=10" \
| jq .

echo
echo "SMOKE PASS"
