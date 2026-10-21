#!/usr/bin/env bash
# Release archive for Linux/macOS (SPEC §4): dist/mkrew-<version>-<os>-<arch>.tar.gz holding
# bin/{mkrew,mk,mkrew-daemon}, agents/*.toml (sample profiles) and README.md.
# --app (macOS): dist/mkrew-<version>-macos-<arch>.zip holding an ad-hoc signed Mkrew.app.
# Usage: scripts/package.sh [--no-build] [--bin-dir DIR] [--out DIR] [--app]
# Prints the archive path. Nothing is published.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

bin_dir="$root/target/release"
out="$root/dist"
build=1
app=0
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) build=0 ;;
    --app) app=1 ;;
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

if [ "$app" = 1 ]; then
  [ "$(uname -s)" = Darwin ] || { echo "package: --app needs macOS" >&2; exit 1; }
  bundle="$stage/Mkrew.app"
  mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/agents"
  for bin in mkrew mk mkrew-daemon; do
    cp "$bin_dir/$bin" "$bundle/Contents/MacOS/$bin"
    chmod 755 "$bundle/Contents/MacOS/$bin"
  done
  cp "$root"/agents/*.toml "$bundle/Contents/Resources/agents/"
  cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.mkrew.app</string>
  <key>CFBundleName</key><string>Mkrew</string>
  <key>CFBundleExecutable</key><string>mkrew</string>
  <key>NSMicrophoneUsageDescription</key><string>Voice input turns your speech into text.</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
  # Ad-hoc signature (ADR 0071): no Developer ID; the first open needs right-click → Open.
  codesign --force --deep -s - "$bundle"
  mkdir -p "$out"
  out="$(cd "$out" && pwd)"
  zip="$out/mkrew-$version-macos-$(uname -m).zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$bundle" "$zip"
  echo "$zip"
  exit 0
fi
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
