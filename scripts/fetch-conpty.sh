#!/usr/bin/env bash
# Windows only: puts the pinned ConPTY pair (conpty.dll + OpenConsole.exe, from Microsoft's
# Microsoft.Windows.Console.ConPTY NuGet package, MIT) next to the Mkrew binaries. portable-pty
# loads a conpty.dll found next to the exe, and that newer ConPTY passes omp's TSP escape
# sequences through (ADR 0107).
# Usage: scripts/fetch-conpty.sh [--dest DIR]   (default: cargo's target dir, /debug and /release)
# The package is cached in ${LOCALAPPDATA:-$HOME/.cache}/mkrew/conpty; the package and both files
# are checked by SHA-256. Safe to run again. Does nothing off Windows.
set -euo pipefail

VERSION=1.24.260710001
URL="https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/$VERSION"
NUPKG_SHA=175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e
# name|entry in the package|SHA-256 of the file
FILES=(
  "conpty.dll|runtimes/win-x64/native/conpty.dll|39fba2713e2495117b1591ae8c32a3b904bea7aa66069cf7815e2844c76d75d8"
  "OpenConsole.exe|build/native/runtimes/x64/OpenConsole.exe|b7fd936c2668b87b9ecf7b3366dc6568afc1c6f981874cba3e955a1c35cf8160"
)

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN* | Windows_NT) ;;
  *)
    echo "fetch-conpty: not Windows, nothing to do"
    exit 0
    ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
# cargo's real target dir (shared worktree config, CARGO_TARGET_DIR); JSON escapes \ as \\
tdir=$(cd "$root" && cargo metadata --format-version 1 --no-deps 2>/dev/null |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | sed 's/\\\\/\//g') || tdir=
[ -n "$tdir" ] || tdir="$root/target"
if command -v cygpath >/dev/null; then tdir=$(cygpath -u "$tdir"); fi
dests=("$tdir/debug" "$tdir/release")
case "${1:-}" in
  "") ;;
  --dest)
    [ $# -eq 2 ] || { echo "usage: fetch-conpty.sh [--dest DIR]" >&2; exit 2; }
    dests=("$2")
    ;;
  *)
    echo "usage: fetch-conpty.sh [--dest DIR]" >&2
    exit 2
    ;;
esac

sha() { sha256sum "$1" | cut -d' ' -f1; }

base=${LOCALAPPDATA:-$HOME/.cache}
if command -v cygpath >/dev/null; then base=$(cygpath -u "$base"); fi
cache="$base/mkrew/conpty"
nupkg="$cache/$VERSION.nupkg"

if [ -f "$nupkg" ] && [ "$(sha "$nupkg")" = "$NUPKG_SHA" ]; then
  echo "fetch-conpty: cache hit $nupkg"
else
  echo "fetch-conpty: downloading $URL"
  mkdir -p "$cache"
  curl -fsSL --retry 3 -o "$nupkg.part" "$URL"
  if [ "$(sha "$nupkg.part")" != "$NUPKG_SHA" ]; then
    rm -f "$nupkg.part"
    echo "fetch-conpty: the downloaded package does not match the pinned SHA-256" >&2
    exit 1
  fi
  mv -f "$nupkg.part" "$nupkg"
fi

command -v unzip >/dev/null || { echo "fetch-conpty: unzip not found (needed to read the package)" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in "${FILES[@]}"; do
  IFS='|' read -r name entry want <<<"$f"
  unzip -p "$nupkg" "$entry" >"$tmp/$name"
  if [ "$(sha "$tmp/$name")" != "$want" ]; then
    echo "fetch-conpty: $entry in the package has an unexpected SHA-256" >&2
    exit 1
  fi
done

for d in "${dests[@]}"; do
  mkdir -p "$d"
  current=1
  for f in "${FILES[@]}"; do
    IFS='|' read -r name _ want <<<"$f"
    [ -f "$d/$name" ] && [ "$(sha "$d/$name")" = "$want" ] || current=0
  done
  if [ "$current" = 1 ]; then
    echo "fetch-conpty: $d up to date"
    continue
  fi
  for f in "${FILES[@]}"; do
    IFS='|' read -r name _ _ <<<"$f"
    cp "$tmp/$name" "$d/$name" || {
      echo "fetch-conpty: cannot write $d/$name (a daemon or session running from there keeps it open; try: mk daemon stop)" >&2
      exit 1
    }
  done
  echo "fetch-conpty: copied to $d"
done
