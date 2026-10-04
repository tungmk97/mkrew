#!/usr/bin/env bash
# Full verification: fmt check, clippy (-D warnings), tests. Exit non-zero on first failure.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f Cargo.toml ]; then
  echo "verify: no Cargo.toml yet (workspace not created) — nothing to check."
  exit 0
fi

step() { printf '\n=== %s ===\n' "$1"; }

step "cargo fmt --check"
cargo fmt --all -- --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
INSTA_UPDATE=no cargo test --workspace

printf '\nverify: OK\n'
