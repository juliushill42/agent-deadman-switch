#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")"
  pwd
)"

cd "$ROOT"

mkdir -p .runtime

if [ ! -f .env ]; then
  ADMIN_TOKEN="$(
    openssl rand -hex 32
  )"

  AGENT_SECRET="$(
    openssl rand -hex 32
  )"

  POSTGRES_PASSWORD="$(
    openssl rand -hex 24
  )"

  cat > .env <<ENV
DEADMAN_BIND=127.0.0.1:8794
DEADMAN_CONTROL_URL=http://127.0.0.1:8794

DEADMAN_ADMIN_TOKEN=$ADMIN_TOKEN
DEADMAN_AGENT_SHARED_SECRET=$AGENT_SECRET

PGPORT=55435
POSTGRES_PASSWORD=$POSTGRES_PASSWORD
DATABASE_URL=postgres://agent_deadman:$POSTGRES_PASSWORD@127.0.0.1:55435/agent_deadman

KAFKA_PORT=29112
KAFKA_CONTROLLER_PORT=29113
KAFKA_BROKERS=127.0.0.1:29112
KAFKA_TOPIC=agent-deadman-events

RUST_LOG=info
ENV

  chmod 600 .env
fi

set -a
. ./.env
set +a

echo "[1/7] PostgreSQL"
./infra/postgres/start.sh

echo "[2/7] Schema"
psql \
  "$DATABASE_URL" \
  -v ON_ERROR_STOP=1 \
  -f migrations/0001_init.sql \
  >/dev/null

echo "[3/7] Kafka KRaft"
./infra/kafka/start.sh

echo "[4/7] Web dependencies"
(
  cd web

  npm install \
    --no-audit \
    --no-fund
)

echo "[5/7] Rust format"
cargo fmt --all

echo "[6/7] Verification"
./scripts/verify.sh

echo "[7/7] Complete"

echo
echo "Dashboard:"
echo "http://127.0.0.1:4176"
echo
echo "Control API:"
echo "http://127.0.0.1:8794"
echo

if [ "${BUILD_ONLY:-0}" != "1" ]; then
  exec ./run.sh
fi
