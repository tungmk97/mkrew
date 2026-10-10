#!/usr/bin/env bash
# Run a superpowers skill script (sdd-workspace, task-start, task-done,
# task-brief, review-package) from the omp plugin cache with Git Bash.
#
# Usage, from the repo root in the omp tool shell:
#   sh scripts/sp.sh NAME PLAN_FILE [ARGS...]
#
# Call it with `sh`, not `bash`: on this machine `bash` on PATH is WSL (no C:/
# paths, no cargo, fails on the CRLF scripts); `sh` is Git's sh.exe. Always
# running Git Bash also keeps each workspace's plan-path marker in one form
# (/c/...), so sdd-workspace never forks a `<slug>-plans/` directory.
set -eu
command -v cygpath >/dev/null || { echo "sp.sh: not Git Bash; run: sh scripts/sp.sh ..." >&2; exit 2; }
name=${1:?usage: sh scripts/sp.sh NAME PLAN_FILE [ARGS...]}
shift
script=$(ls -d "$HOME"/.omp/plugins/cache/plugins/superpowers-dev___superpowers___*/skills/*/scripts/"$name" 2>/dev/null | sort -V | tail -n 1)
[ -n "$script" ] || { echo "sp.sh: no superpowers script named '$name'" >&2; exit 2; }
# C:\... and C:/... plan paths become /c/...; relative paths stay relative.
[ $# -eq 0 ] || set -- "$(cygpath -u "$1")" "${@:2}"
exec bash "$script" "$@"
