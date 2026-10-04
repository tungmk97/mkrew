#!/usr/bin/env bash
# SessionStart hook: stdout is added to Claude's context (startup, resume, clear, compact).
cd "$(dirname "$0")/../.." || exit 0
P=docs/PROGRESS.md
[ -f "$P" ] || exit 0
echo "## Mkrew build loop context (auto-injected)"
echo
echo "Follow docs/BUILD_LOOP.md: ONE task this session, then stop."
echo
sed -n '/<!-- loop-state/,/<!-- \/loop-state -->/p' "$P" | grep -E '^(status|milestone):'
echo
echo "### Open tasks (first 6)"
grep -E '^- \[(~| |!)\] T[0-9]' "$P" | head -n 6
echo
echo "### Notes for this session"
sed -n '/^## Notes for the next session/,$p' "$P" | tail -n +2 | head -n 12
echo
echo "### Repo"
git log --oneline -5 2>/dev/null
dirty=$(git status --short 2>/dev/null | head -n 10)
[ -n "$dirty" ] && { echo "Uncommitted changes:"; echo "$dirty"; }
exit 0
