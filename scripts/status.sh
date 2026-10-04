#!/usr/bin/env bash
# Human-friendly summary of the autonomous build.
cd "$(dirname "$0")/.."
P=docs/PROGRESS.md
status=$(grep -m1 '^status:' "$P" | awk '{print $2}')
milestone=$(grep -m1 '^milestone:' "$P" | awk '{print $2}')
done_n=$(grep -cE '^- \[x\] T[0-9]' "$P")
todo_n=$(grep -cE '^- \[ \] T[0-9]' "$P")
blocked_n=$(grep -cE '^- \[!\] T[0-9]' "$P")
current=$(grep -m1 -E '^- \[~\] T[0-9]' "$P" || true)
next=$(grep -m1 -E '^- \[ \] T[0-9]' "$P" || true)
echo "Mkrew build — status: $status | milestone: $milestone"
echo "Tasks in current milestone: done $done_n · todo $todo_n · blocked $blocked_n"
[ -n "$current" ] && echo "In progress: ${current#- \[~\] }"
[ -n "$next" ] && echo "Next:        ${next#- \[ \] }"
echo
echo "Milestone tags: $(git tag --list 'm*-done' 2>/dev/null | tr '\n' ' ')"
echo "Last commits:"; git log --oneline -5 2>/dev/null | sed 's/^/  /'
open_checks=$(awk '/^## Waiting/{f=1;next}/^## Done/{f=0}f && /^- /' docs/HUMAN_CHECKS.md | wc -l)
echo
echo "Open items in docs/HUMAN_CHECKS.md: $open_checks"
if [ -f .mkrew-build/loop.log ]; then echo; echo "Loop log (last 5):"; tail -n 5 .mkrew-build/loop.log | sed 's/^/  /'; fi
