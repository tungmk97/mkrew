#!/usr/bin/env bash
# M0 spike measurements (SPEC §15, T0.8): idle RAM/CPU, key latency, output-flood CPU.
# Opens real windows, so it needs a desktop session. On Windows the release build needs
# fxc.exe (Windows SDK) on PATH or GPUI_FXC_PATH.
# Usage: bash scripts/measure-spike.sh [flood-MiB]   (default 50). Logs: target/measure/*.log
set -euo pipefail
cd "$(dirname "$0")/.."

flood_mib="${1:-50}"
out=target/measure
mkdir -p "$out"

cargo build --release -p spike-term
bin=target/release/spike-term
[ -f "$bin.exe" ] && bin="$bin.exe"

# run <name> <seconds> <spike-term args...>: runs the window (killed after <seconds> unless the
# child exits first), then prints the last interval lines and the run totals.
run() {
  local name=$1 secs=$2
  shift 2
  printf '\n=== %s: spike-term %s ===\n' "$name" "$*"
  timeout "$secs" "$bin" "$@" 2>"$out/$name.log" >/dev/null || true
  grep '^stats t=' "$out/$name.log" | tail -n 3 || true
  grep '^stats total' "$out/$name.log" | tail -n 1 || true
}

# Idle: default shell, no input, no output. RAM/CPU = the late interval lines.
run idle 20 --stats
# Key latency: a synthetic key every 500 ms into the idle shell (echo → painted frame).
run latency 20 --stats --probe-keys
# Flood: the window runs `spike-term --flood N`; exits when the child is done. No key probe:
# the flood child never echoes input, so UI responsiveness is read from `frame_gap_max`.
# Only the UI process is sampled (not conhost/OpenConsole or the child).
run flood 300 --stats -- "$bin" --flood "$flood_mib"
