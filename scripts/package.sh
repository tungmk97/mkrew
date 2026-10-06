#!/usr/bin/env bash
# Release archive for Linux/macOS (SPEC §4): dist/mkrew-<version>-<os>-<arch>.tar.gz holding
# bin/{mkrew,mk,mkrew-daemon}, agents/*.toml (sample profiles) and README.md.
# Usage: scripts/package.sh [--no-build] [--bin-dir DIR] [--out DIR]
# Prints the archive path. Nothing is published.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

bin_dir="$root/target/release"
out="$root/dist"
build=1
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) build=0 ;;
    --bin-dir) bin_dir="${2:?--bin-dir needs a directory}"; shift ;;
    --out) out="${2:?--out needs a directory}"; shift ;;
    *) echo "package: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done

version="$(tr -d '\r' < "$root/Cargo.toml" | sed -n 's/^version = "\(.*\)"$/\1/p' | head -n 1)"
[ -n "$version" ] || { echo "package: no version in Cargo.toml" >&2; exit 1; }
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
name="mkrew-$version-$os-$(uname -m)"

if [ "$build" = 1 ]; then
  (cd "$root" && cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon)
fi

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name/bin" "$stage/$name/agents"
for bin in mkrew mk mkrew-daemon; do
  cp "$bin_dir/$bin" "$stage/$name/bin/$bin"
  chmod 755 "$stage/$name/bin/$bin"
done
cp "$root"/agents/*.toml "$stage/$name/agents/"
cp "$root/README.md" "$stage/$name/"

mkdir -p "$out"
out="$(cd "$out" && pwd)"
# Relative archive name: GNU tar reads `C:/…` (Git Bash) as a remote host.
(cd "$out" && tar -czf "$name.tar.gz" -C "$stage" "$name")
echo "$out/$name.tar.gz"
