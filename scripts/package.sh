#!/usr/bin/env bash
# Release archive for Linux/macOS (SPEC §4): dist/mkrew-<version>-<os>-<arch>.tar.gz holding
# bin/{mkrew,mk,mkrew-daemon}, agents/*.toml (sample profiles) and README.md.
# --app (macOS): dist/mkrew-<version>-macos-<arch>.zip holding Mkrew.app with its icon, signed
# with a Developer ID and notarized when MKREW_SIGN_IDENTITY, MKREW_NOTARY_APPLE_ID,
# MKREW_NOTARY_PASSWORD and MKREW_NOTARY_TEAM_ID are set (ADR 0100), else ad-hoc signed.
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
  # Profiles of Windows shells only (cmd, powershell, git-bash) stay out of the Mac bundle.
  for profile in "$root"/agents/*.toml; do
    case "$(basename "$profile")" in
      cmd.toml|powershell.toml|git-bash.toml) ;;
      *) cp "$profile" "$bundle/Contents/Resources/agents/" ;;
    esac
  done
  # App icon: packaging/macos/mkrew-1024.png (icon/mkrew-1-lanes/tile-dark.svg) at the sizes
  # iconutil wants.
  iconset="$stage/Mkrew.iconset"
  mkdir -p "$iconset"
  icon="$root/packaging/macos/mkrew-1024.png"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" > /dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
  done
  iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/Mkrew.icns"
  cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.mkrew.app</string>
  <key>CFBundleName</key><string>Mkrew</string>
  <key>CFBundleExecutable</key><string>mkrew</string>
  <key>CFBundleIconFile</key><string>Mkrew</string>
  <key>NSMicrophoneUsageDescription</key><string>Voice input turns your speech into text.</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
  # Developer ID (ADR 0100): inside out, no --deep. The helper programs first, then the bundle
  # with the hardened runtime and the microphone entitlement. Without an identity: ad-hoc
  # (ADR 0071), which Gatekeeper still blocks on the first open.
  identity="${MKREW_SIGN_IDENTITY:-}"
  if [ -n "$identity" ]; then
    for bin in mk mkrew-daemon; do
      codesign --force --options runtime --timestamp --sign "$identity" "$bundle/Contents/MacOS/$bin"
    done
    codesign --force --options runtime --timestamp \
      --entitlements "$root/packaging/macos/mkrew.entitlements" --sign "$identity" "$bundle"
    codesign --verify --strict --verbose=2 "$bundle"
  else
    echo "package: MKREW_SIGN_IDENTITY not set, signing ad-hoc" >&2
    codesign --force --deep -s - "$bundle"
  fi
  mkdir -p "$out"
  out="$(cd "$out" && pwd)"
  zip="$out/mkrew-$version-macos-$(uname -m).zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$bundle" "$zip"
  if [ -n "$identity" ] && [ -n "${MKREW_NOTARY_APPLE_ID:-}" ] && [ -n "${MKREW_NOTARY_PASSWORD:-}" ] && [ -n "${MKREW_NOTARY_TEAM_ID:-}" ]; then
    xcrun notarytool submit "$zip" --apple-id "$MKREW_NOTARY_APPLE_ID" \
      --password "$MKREW_NOTARY_PASSWORD" --team-id "$MKREW_NOTARY_TEAM_ID" \
      --wait --output-format json > "$stage/notary.json" || true
    grep -Eq '"status" *: *"Accepted"' "$stage/notary.json" || {
      cat "$stage/notary.json" >&2
      # Apple's reason for the rejection is in the submission log, not in the submit answer.
      id="$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$stage/notary.json" | head -n 1)"
      [ -z "$id" ] || xcrun notarytool log "$id" --apple-id "$MKREW_NOTARY_APPLE_ID" \
        --password "$MKREW_NOTARY_PASSWORD" --team-id "$MKREW_NOTARY_TEAM_ID" >&2 || true
      echo "package: Apple did not accept the notarization" >&2
      exit 1
    }
    # Stapling changes the bundle, so the zip is made again after it.
    xcrun stapler staple "$bundle"
    rm -f "$zip"
    ditto -c -k --keepParent "$bundle" "$zip"
  else
    echo "package: notarization skipped (identity or MKREW_NOTARY_* not all set)" >&2
  fi
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
