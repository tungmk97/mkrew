#!/usr/bin/env bash
# Mkrew autonomous build loop.
# Runs one fresh agent session per task (clean context each time) until the loop status in
# docs/PROGRESS.md is no longer RUNNING, a STOP file appears, or no progress is made.
#
# Usage:
#   bash scripts/autobuild.sh [--agent claude|omp] [--max-iter N] [--until M3]
#                             [--timeout-min 90] [--max-turns 200] [--model NAME]
#
# Stop gracefully at any time:  touch .mkrew-build/STOP   (current session finishes first)
# Extra flags: CLAUDE_FLAGS="..."  OMP_FLAGS="..."  (environment variables)
set -uo pipefail
cd "$(dirname "$0")/.."

AGENT=claude
MAX_ITER=40
UNTIL=""
TIMEOUT_MIN=90
MAX_TURNS=200
MODEL=""
MAX_NO_PROGRESS=3

while [ $# -gt 0 ]; do
  case "$1" in
    --agent) AGENT="$2"; shift 2 ;;
    --max-iter) MAX_ITER="$2"; shift 2 ;;
    --until) UNTIL="$2"; shift 2 ;;
    --timeout-min) TIMEOUT_MIN="$2"; shift 2 ;;
    --max-turns) MAX_TURNS="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option: $1"; exit 2 ;;
  esac
done

STATE_DIR=.mkrew-build
LOG_DIR="$STATE_DIR/logs"
LOOP_LOG="$STATE_DIR/loop.log"
mkdir -p "$LOG_DIR"
rm -f "$STATE_DIR/STOP"

log() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" | tee -a "$LOOP_LOG"; }
field() { grep -m1 "^$1:" docs/PROGRESS.md | awk '{print $2}'; }
milestone_num() { echo "$1" | tr -dc '0-9'; }

command -v "$AGENT" >/dev/null 2>&1 || { echo "agent command '$AGENT' not found in PATH"; exit 2; }

if [ ! -d .git ]; then
  git init -q && git add -A && git commit -qm "chore: bootstrap mkrew build kit" && log "initialized git repository"
fi

PROMPT='You are running one headless iteration of the Mkrew autonomous build.
Follow docs/BUILD_LOOP.md exactly (skill: mkrew-next): read docs/PROGRESS.md, do exactly ONE task
end to end (implement, test, verify, review, update PROGRESS, commit), then stop.
There is no human available: never ask questions; record assumptions in docs/HUMAN_CHECKS.md.'

run_agent() {
  local logfile="$1"
  local -a cmd
  case "$AGENT" in
    claude)
      # Skip user-level settings (global hooks/plugins) so headless runs only use this repo's config.
      cmd=(claude -p "$PROMPT" --setting-sources project,local --permission-mode acceptEdits --max-turns "$MAX_TURNS")
      [ -n "$MODEL" ] && cmd+=(--model "$MODEL")
      # shellcheck disable=SC2206
      [ -n "${CLAUDE_FLAGS:-}" ] && cmd+=(${CLAUDE_FLAGS})
      ;;
    omp)
      # Verify flags with `omp --help` on your version; override via OMP_FLAGS.
      # shellcheck disable=SC2206
      local -a extra=(${OMP_FLAGS:---auto-approve})
      cmd=(omp -p "${extra[@]}")
      [ -n "$MODEL" ] && cmd+=(--model "$MODEL")
      cmd+=("$PROMPT")
      ;;
    *) echo "unsupported agent: $AGENT"; return 2 ;;
  esac
  if command -v timeout >/dev/null 2>&1; then
    timeout --foreground "$((TIMEOUT_MIN * 60))" "${cmd[@]}" >"$logfile" 2>&1
  else
    "${cmd[@]}" >"$logfile" 2>&1
  fi
}

log "loop start: agent=$AGENT max_iter=$MAX_ITER until=${UNTIL:-none}"
no_progress=0
iter=0
while [ "$iter" -lt "$MAX_ITER" ]; do
  status=$(field status)
  milestone=$(field milestone)

  if [ -f "$STATE_DIR/STOP" ]; then log "STOP file found — stopping."; break; fi
  if [ "$status" != "RUNNING" ]; then log "status is $status — stopping. See docs/HUMAN_CHECKS.md"; break; fi
  if [ -n "$UNTIL" ] && [ "$(milestone_num "$milestone")" -gt "$(milestone_num "$UNTIL")" ]; then
    log "reached milestone $milestone (past --until $UNTIL) — stopping."; break
  fi

  iter=$((iter + 1))
  before=$(git rev-parse HEAD 2>/dev/null || echo none)
  logfile="$LOG_DIR/iter-$(date '+%Y%m%d-%H%M%S')-$iter.log"
  log "iteration $iter ($milestone) → $logfile"
  run_agent "$logfile"
  rc=$?
  after=$(git rev-parse HEAD 2>/dev/null || echo none)

  if [ "$before" != "$after" ]; then
    no_progress=0
    log "  rc=$rc  new commits: $(git log --oneline "$before..$after" 2>/dev/null | wc -l | tr -d ' ')  last: $(git log -1 --pretty=%s)"
  else
    no_progress=$((no_progress + 1))
    log "  rc=$rc  no new commit ($no_progress/$MAX_NO_PROGRESS)"
    if [ "$no_progress" -ge "$MAX_NO_PROGRESS" ]; then
      log "no progress in $MAX_NO_PROGRESS iterations — stopping. Check the last logs in $LOG_DIR"
      break
    fi
  fi
  sleep 3
done

log "loop end after $iter iteration(s); status=$(field status) milestone=$(field milestone)"
printf '\a'
bash scripts/status.sh
