#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  pwd
)"

PGBIN="$(pg_config --bindir)"
PGDATA="${PGDATA:-$ROOT/.runtime/postgres/data}"

if [ -f "$PGDATA/PG_VERSION" ]; then
  "$PGBIN/pg_ctl" \
    -D "$PGDATA" \
    stop \
    -m fast \
    >/dev/null 2>&1 || true
fi
