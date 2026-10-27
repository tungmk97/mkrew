#!/usr/bin/env bash
# Deletes dev-generated files in this repo. Dry run unless --apply.
# Usage: sh .claude/skills/mkrew-cleanup/cleanup.sh [--stale DAYS] [--full] [--apply]
#   (default)     junk: *.locked-old* exes, target/{tmp/*,idx,measure,verify.log}, *.pending-snap,
#                 .mkrew-build/, .superpowers-baseline.log, empty unregistered .worktrees/* dirs
#   --stale DAYS  + build units under target*/<profile>/{deps,build,.fingerprint,incremental}
#                 untouched for DAYS days (cargo rebuilds them if still needed)
#   --full        + every target dir (= cargo clean, main repo and every worktree); beats --stale
# Safety: a path is deleted only if git ignores it and nothing under it is tracked.
set -u
root=$(git rev-parse --show-toplevel) || exit 1
cd "$root" || exit 1

apply=0 stale="" full=0
while [ $# -gt 0 ]; do
  case "$1" in
    --apply) apply=1 ;;
    --stale)
      stale="${2:-}"
      case "$stale" in '' | *[!0-9]*) echo "--stale needs a number of days" >&2; exit 2 ;; esac
      shift ;;
    --full) full=1 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
: >"$tmp/all"

# safe PATH: true if PATH is git-ignored and holds no tracked file.
safe() {
  git check-ignore -q -- "$1" || { echo "keep (not ignored): $1"; return 1; }
  [ -z "$(git ls-files -- "$1" | head -n 1)" ] || { echo "keep (tracked): $1"; return 1; }
}
# add GROUP PATH: queue PATH if it exists and is safe.
add() {
  [ -e "$2" ] && safe "$2" && printf '%s\0' "$2" >>"$tmp/$1"
  return 0
}

# Target dirs are checked once; everything below a safe target dir is safe too.
tdirs=""
for t in target .worktrees/*/target; do
  [ -d "$t" ] && safe "$t" && tdirs="$tdirs $t"
done

for t in $tdirs; do
  find "$t" -maxdepth 3 -name '*.locked-old*' -print0 >>"$tmp/junk"
  for p in "$t"/tmp/* "$t/idx" "$t/measure" "$t/verify.log"; do
    [ -e "$p" ] && printf '%s\0' "$p" >>"$tmp/junk"
  done
done
find . \( -name target -o -name .git \) -prune -o -name '*.pending-snap' -print |
  while IFS= read -r p; do add junk "$p"; done
add junk .mkrew-build
add junk .superpowers-baseline.log

registered=$(git worktree list --porcelain | sed -n 's/^worktree //p')
for w in .worktrees/*/; do
  [ -d "$w" ] || continue
  w=${w%/}
  printf '%s\n' "$registered" | grep -qxF "$root/$w" && continue
  if [ -z "$(find "$w" -type f | head -n 1)" ]; then
    add junk "$w"
  else
    echo "keep (unregistered worktree with files, check by hand): $w"
  fi
done

if [ "$full" = 1 ]; then
  for t in $tdirs; do printf '%s\0' "$t" >>"$tmp/full"; done
elif [ -n "$stale" ]; then
  for t in $tdirs; do
    for prof in "$t"/*/; do
      for sub in deps build .fingerprint incremental; do
        [ -d "$prof$sub" ] || continue
        find "$prof$sub" -mindepth 1 -maxdepth 1 -mtime +"$stale" -print0 >>"$tmp/stale"
      done
    done
  done
fi

for g in junk stale full; do
  [ -s "$tmp/$g" ] || continue
  n=$(tr -cd '\0' <"$tmp/$g" | wc -c)
  size=$(du -csh --files0-from="$tmp/$g" 2>/dev/null | tail -n 1 | cut -f1)
  echo "== $g: $n paths, $size"
  [ "$g" = stale ] || tr '\0' '\n' <"$tmp/$g" | sed 's/^/   /'
  cat "$tmp/$g" >>"$tmp/all"
done
[ -s "$tmp/all" ] || { echo "nothing to clean"; exit 0; }

if [ "$apply" = 0 ]; then
  echo "dry run: nothing deleted. Re-run with --apply to delete."
  exit 0
fi
# Files held by a running process (Windows) fail to delete; report and go on.
failed=0
while IFS= read -r -d '' p; do
  rm -rf -- "$p" 2>/dev/null || { failed=$((failed + 1)); echo "in use, skipped: $p"; }
done <"$tmp/all"
echo "done; $failed paths skipped"
