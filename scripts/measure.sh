#!/usr/bin/env bash
# SPEC §15 measurements (T8.9), release builds: UI first frame, idle CPU and memory with six fake
# sessions (opens a real window, needs a desktop session), the six-pane flood stress test, and
# git status on a 10,000-file repo. Results: docs/decisions/0009-non-functional-measurements.md.
# On Windows the release UI build needs fxc.exe (Windows SDK) on PATH or GPUI_FXC_PATH.
# Usage: bash scripts/measure.sh   Logs: target/measure/*.log
set -euo pipefail
cd "$(dirname "$0")/.."

out=target/measure
mkdir -p "$out"

# The tests start the daemon and fake agent from the target dir.
cargo build --release -p mkrew-daemon -p mkrew-fakeagent

cargo test --release -p mkrew-ui --test measure -- --ignored --nocapture 2>&1 | tee "$out/window.log"
cargo test --release -p mkrew-ui --test stress -- --nocapture 2>&1 | tee "$out/flood.log"
cargo test --release -p mkrew-daemon --lib status_of_a_10000_file_repo_meets_the_budget \
  -- --nocapture 2>&1 | tee "$out/git.log"

printf '\n=== §15 results ===\n'
grep -h -E '^(measure|stress|git\.status)' "$out/window.log" "$out/flood.log" "$out/git.log" ||
  { echo "measure: no result lines found" >&2; exit 1; }
