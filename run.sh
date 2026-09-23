#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")"
  pwd
)"

cd "$ROOT"

[ -f .env ] || {
  echo "Run ./bootstrap.sh first."
  exit 1
}

set -a
. ./.env
set +a

mkdir -p .runtime/logs

./infra/postgres/start.sh
./infra/kafka/start.sh

cleanup() {
  jobs -pr \
    | xargs -r kill \
      >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

echo "Starting control plane..."

cargo run \
  --release \
  -p deadman-control \
  >.runtime/logs/control.log \
  2>&1 &

CONTROL_PID=$!

for _ in $(seq 1 60); do
  if curl \
    -fsS \
    http://127.0.0.1:8794/healthz \
    >/dev/null 2>&1
  then
    break
  fi

  if ! kill -0 \
    "$CONTROL_PID" \
    >/dev/null 2>&1
  then
    cat \
      .runtime/logs/control.log

    exit 1
  fi

  sleep 1
done

curl \
  -fsS \
  http://127.0.0.1:8794/healthz \
  >/dev/null || {
    cat \
      .runtime/logs/control.log

    exit 1
  }

echo "Starting local deadman agent..."

cargo run \
  --release \
  -p deadman-agent \
  >.runtime/logs/agent.log \
  2>&1 &

echo "Starting dashboard..."

(
  cd web
  npm run preview
) >.runtime/logs/web.log \
  2>&1 &

sleep 3

echo
echo "===================================================="
echo " AGENT DEADMAN SWITCH — RUNNING"
echo "===================================================="
echo
echo "Dashboard:"
echo "  http://127.0.0.1:4176"
echo
echo "Control API:"
echo "  http://127.0.0.1:8794"
echo
echo "Tokens:"
echo "  ./scripts/show-tokens.sh"
echo
echo "Agents:"
echo "  ./scripts/agents.sh"
echo
echo 'Watermark: ":"'
echo
echo "===================================================="

wait
