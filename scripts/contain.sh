#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  pwd
)"

cd "$ROOT"

[ "$#" -ge 1 ] || {
  echo "Usage: $0 AGENT_UUID [REASON]"
  exit 1
}

set -a
. ./.env
set +a

AGENT_ID="$1"
REASON="${2:-manual containment}"

jq -n \
  --arg reason "$REASON" \
  '{
    directive: "contain",
    reason: $reason
  }' \
| curl \
    -fsS \
    -X POST \
    -H \
    "Authorization: Bearer $DEADMAN_ADMIN_TOKEN" \
    -H \
    "Content-Type: application/json" \
    --data-binary @- \
    "http://127.0.0.1:8794/api/v1/agents/$AGENT_ID/directive" \
| jq .
