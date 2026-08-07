#!/usr/bin/env bash
#
# Wait for a running `generate-data` to finish, verify what it produced, then
# commit and push the regenerated language packs.
#
# Regenerating packs and committing the result is a recurring chore (see
# `git log -- 'out/*_for_*/language_data.rkyv'`), and the risky parts are always
# the same: committing a half-written pack, or sweeping unrelated code changes
# into a multi-GB data commit. This does the waiting and the checking.
#
#   ./scripts/commit-regenerated-packs.sh
#   REASON="new subtitles corpus" ./scripts/commit-regenerated-packs.sh
#   DRY_RUN=1 ./scripts/commit-regenerated-packs.sh   # verify + stage, stop before commit
#
# Environment:
#   REASON        extra paragraph for the commit body (why the packs changed)
#   PID           generate-data PID; default auto-detect, and if nothing is
#                 running it skips straight to verifying what's on disk
#   DRY_RUN       non-zero: stage and report, but don't commit or push
#   POLL_SECONDS  how often to check whether generate-data is still alive (60)
#
# It only ever stages out/, and aborts rather than committing if a check fails.
#
# Caveat worth knowing: this attaches to an already-running process, so it can
# see *that* generate-data exited but not *how*. A crash partway through looks
# the same as a clean finish. The integrity checks cover torn writes -- each
# pack records its own byte size in language_data.hash -- but a run that died
# cleanly between languages will still look finished. The script prints the
# language pairs it is about to commit; if that list is short, it stopped early.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

DRY_RUN="${DRY_RUN:-0}"
POLL_SECONDS="${POLL_SECONDS:-60}"
REASON="${REASON:-}"

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf '\nABORT: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- 0. preflight

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" = "main" ]; then
  die "on main; refusing to auto-commit packs here"
fi

# Modified *tracked* files outside out/ mean the test run below would be
# checking code that isn't what gets committed. Bail. (Untracked files are
# only reported -- `git add out/` can't stage them, so they're harmless.)
DIRTY_CODE="$(git status --porcelain | grep -vE '^\?\?' | grep -vE '^.. out/' || true)"
if [ -n "$DIRTY_CODE" ]; then
  printf '%s\n' "$DIRTY_CODE"
  die "tracked changes outside out/ (shown above); commit or stash them first"
fi

UNTRACKED="$(git status --porcelain | grep -E '^\?\?' | grep -vE '^\?\? out/' || true)"
[ -z "$UNTRACKED" ] || printf 'note: untracked files present (will not be committed):\n%s\n' "$UNTRACKED"

# ------------------------------------------------------------------- 1. wait

PID="${PID:-$(pgrep -x generate-data | head -1 || true)}"

if [ -z "$PID" ]; then
  say "no generate-data running -- verifying and committing what's already on disk"
elif ! kill -0 "$PID" 2>/dev/null; then
  die "PID $PID is not running"
else
  say "waiting on generate-data (PID $PID), polling every ${POLL_SECONDS}s"
  printf 'started waiting at %s\n' "$(date '+%F %T')"
  while kill -0 "$PID" 2>/dev/null; do
    sleep "$POLL_SECONDS"
  done
  printf 'generate-data exited at %s\n' "$(date '+%F %T')"
  # Give the filesystem a moment to settle after the final write.
  sleep 5
fi

# -------------------------------------------------------------- 2. integrity

say "verifying pack integrity"

# generate-data writes "<xxh3>;<size_in_bytes>" next to each pack, after the
# pack itself. A truncated pack (killed mid-write) disagrees with its recorded
# size; a pack whose hash file never got written is missing outright.
FAILED=0
for rkyv in out/*_for_*/language_data.rkyv; do
  hash_file="${rkyv%/*}/language_data.hash"
  pair="$(basename "$(dirname "$rkyv")")"
  if [ ! -f "$hash_file" ]; then
    printf '  %-22s MISSING language_data.hash\n' "$pair"; FAILED=1; continue
  fi
  recorded="$(cat "$hash_file")"
  recorded_size="${recorded##*;}"
  actual_size="$(stat -f %z "$rkyv")"
  if [ "$recorded_size" != "$actual_size" ]; then
    printf '  %-22s SIZE MISMATCH recorded=%s actual=%s\n' "$pair" "$recorded_size" "$actual_size"
    FAILED=1
  else
    printf '  %-22s ok (%s bytes)\n' "$pair" "$actual_size"
  fi
done
[ "$FAILED" -eq 0 ] || die "pack integrity check failed (see above) -- not committing"

# Leftover .partial files mean a corpus-cleaning stage was interrupted.
PARTIALS="$(ls out/cleaned_*.partial.jsonl 2>/dev/null || true)"
if [ -n "$PARTIALS" ]; then
  printf '%s\n' "$PARTIALS"
  die "leftover partial files (shown above), run looks incomplete"
fi

# ------------------------------------------------------------------ 3. tests

say "running test suite against the new packs"
cargo test --workspace || die "tests failed against the regenerated packs -- not committing"

# ------------------------------------------------------------------ 4. stage

say "staging out/"
git add out/

STAGED_OUTSIDE="$(git diff --cached --name-only | grep -v '^out/' || true)"
if [ -n "$STAGED_OUTSIDE" ]; then
  printf '%s\n' "$STAGED_OUTSIDE"
  die "something outside out/ got staged (shown above)"
fi

STAGED="$(git diff --cached --name-only)"
[ -n "$STAGED" ] || die "nothing staged -- generate-data produced no changes?"

# Which language pairs did this run actually rewrite?
PAIRS="$(printf '%s\n' "$STAGED" | sed -n 's|^out/\([a-z-]*_for_[a-z]*\)/.*|\1|p' | sort -u | tr '\n' ' ')"

say "about to commit"
printf 'branch:    %s\n' "$BRANCH"
printf 'files:     %s\n' "$(printf '%s\n' "$STAGED" | wc -l | tr -d ' ')"
printf 'pairs:     %s\n' "${PAIRS:-none}"
printf '\n%s\n' "$(git diff --cached --stat | tail -25)"

if [ "$DRY_RUN" != "0" ]; then
  say "DRY_RUN set -- staged but not committing or pushing"
  exit 0
fi

# ---------------------------------------------------------- 5. commit + push

MESSAGE="Regenerate language packs

Regenerated by generate-data: ${PAIRS:-none}"
[ -z "$REASON" ] || MESSAGE="$MESSAGE

$REASON"

git commit -m "$MESSAGE"

# Note this pushes the *branch*, so any other local commits go up too.
say "pushing to origin/$BRANCH (large LFS upload, this takes a while)"
git log --oneline "origin/$BRANCH..HEAD" 2>/dev/null || true
git push origin "HEAD:$BRANCH"

say "done"
git log --oneline -1
