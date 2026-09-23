#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  pwd
)"

cd "$ROOT"

echo "[1/5] Shell syntax"

while IFS= read -r -d '' file; do
  bash -n "$file"
done < <(
  find . \
    -path './.git' -prune -o \
    -path './.runtime' -prune -o \
    -path './target' -prune -o \
    -path './web/node_modules' -prune -o \
    -name '*.sh' \
    -print0
)

echo "[2/5] Rust format"
cargo fmt --all -- --check

echo "[3/5] Rust tests"
cargo test --workspace

echo "[4/5] Rust release build"
cargo build \
  --workspace \
  --release

echo "[5/5] Web build"
(
  cd web
  npm run build
)

echo
echo "VERIFY PASS"
