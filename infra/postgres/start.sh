#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  pwd
)"

cd "$ROOT"

[ -f .env ] || {
  echo "Missing .env"
  exit 1
}

set -a
. ./.env
set +a

PGBIN="$(pg_config --bindir)"
PGDATA="${PGDATA:-$ROOT/.runtime/postgres/data}"
PGSOCKET="$ROOT/.runtime/postgres/socket"
PGLOG="$ROOT/.runtime/postgres/postgres.log"
PGPORT="${PGPORT:-55435}"

mkdir -p \
  "$PGDATA" \
  "$PGSOCKET" \
  "$(dirname "$PGLOG")"

if [ ! -f "$PGDATA/PG_VERSION" ]; then
  "$PGBIN/initdb" \
    -D "$PGDATA" \
    --auth-local=trust \
    --auth-host=scram-sha-256 \
    >/dev/null
fi

if ! "$PGBIN/pg_ctl" \
  -D "$PGDATA" \
  status \
  >/dev/null 2>&1
then
  "$PGBIN/pg_ctl" \
    -D "$PGDATA" \
    -l "$PGLOG" \
    -o "-p $PGPORT -h 127.0.0.1 -k '$PGSOCKET'" \
    start \
    >/dev/null
fi

for _ in $(seq 1 30); do
  if "$PGBIN/pg_isready" \
    -h "$PGSOCKET" \
    -p "$PGPORT" \
    >/dev/null 2>&1
  then
    break
  fi

  sleep 1
done

"$PGBIN/pg_isready" \
  -h "$PGSOCKET" \
  -p "$PGPORT" \
  >/dev/null 2>&1 || {
    echo "PostgreSQL failed."
    tail -n 100 "$PGLOG" || true
    exit 1
  }

SUPERUSER="$(id -un)"

if ! "$PGBIN/psql" \
  -h "$PGSOCKET" \
  -p "$PGPORT" \
  -U "$SUPERUSER" \
  -d postgres \
  -tAc \
  "SELECT 1 FROM pg_roles WHERE rolname='agent_deadman'" \
  | grep -qx 1
then
  "$PGBIN/psql" \
    -h "$PGSOCKET" \
    -p "$PGPORT" \
    -U "$SUPERUSER" \
    -d postgres \
    -v ON_ERROR_STOP=1 \
    -c \
    "CREATE ROLE agent_deadman LOGIN PASSWORD '${POSTGRES_PASSWORD}'" \
    >/dev/null
fi

if ! "$PGBIN/psql" \
  -h "$PGSOCKET" \
  -p "$PGPORT" \
  -U "$SUPERUSER" \
  -d postgres \
  -tAc \
  "SELECT 1 FROM pg_database WHERE datname='agent_deadman'" \
  | grep -qx 1
then
  "$PGBIN/createdb" \
    -h "$PGSOCKET" \
    -p "$PGPORT" \
    -U "$SUPERUSER" \
    -O agent_deadman \
    agent_deadman
fi

echo \
  "PostgreSQL ready 127.0.0.1:$PGPORT"
