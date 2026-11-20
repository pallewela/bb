#!/usr/bin/env bash
# Integration test of the agent skill's recipes (skills/bitbucket-cli/SKILL.md) against real
# Bitbucket Cloud. tests/skill/run.sh covers the recipes in depth against a mocked `bb`; this
# checks the same recipes still hold against the live API.
#
# What it does:
#   1. Creates a throwaway private repository `bb-skill-<timestamp>` in $WORKSPACE.
#   2. Pushes branches `feature/login` and `feature/login-v2` and opens a PR for each.
#   3. Runs the recipes with the real `bb`: PR lookup, checks, summary, comment upsert, merge
#      guards and a real merge, and PR reuse.
#   4. Deletes the repository on exit (KEEP=1 keeps it).
#
# Requirements: as scripts/e2e.sh (bb logged in with repository admin/delete and pull request
# scopes plus read:user, git with SSH access to bitbucket.org, jq). No Pipelines minutes are used.
#
# Usage:
#   scripts/skill-e2e.sh                     # uses `bb` from PATH
#   BB=target/debug/bb scripts/skill-e2e.sh  # test a development build
#   KEEP=1 scripts/skill-e2e.sh              # keep the test repository afterwards
#   VERBOSE=1 scripts/skill-e2e.sh           # print each recipe call and the bb commands it runs
#   VERBOSE=10 scripts/skill-e2e.sh          # ...with the input (stdin) and output of each
#
# Exit status: 0 if every assert passed, 1 otherwise.
set -u
WORKSPACE="${WORKSPACE:-dupelk}"

HERE="$(cd "$(dirname "$0")" && pwd)"
SKILL="$HERE/../skills/bitbucket-cli/SKILL.md"
BB="${BB:-bb}"
case "$BB" in */*) BB="$(cd "$(dirname "$BB")" && pwd)/$(basename "$BB")" ;; esac
BB="$(command -v "$BB")" || { echo "bb not found (set BB=path/to/bb)" >&2; exit 1; }
for tool in git jq; do command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }; done

RUN_ID="$(date +%Y%m%d%H%M%S)"
FULL="$WORKSPACE/bb-skill-$RUN_ID"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/bb-skill-e2e.XXXXXX")"
CLONE="$TMP/clone"
CREATED=""
export GIT_AUTHOR_NAME="bb e2e" GIT_AUTHOR_EMAIL="bb-e2e@example.invalid"
export GIT_COMMITTER_NAME="bb e2e" GIT_COMMITTER_EMAIL="bb-e2e@example.invalid"

# Verbosity: 0 (default), 1 (commands), 10 (commands with their input and output). Trace output
# goes to fd 3, the script's stdout, so it shows even inside $(...) and never mixes into what the
# asserts read. Colors: cyan = command, yellow = input (stdin), gray = exit code and output.
case "${VERBOSE:-0}" in '' | *[!0-9]*) V=1 ;; *) V="${VERBOSE:-0}" ;; esac
exec 3>&1
trace() { printf '  \033[%sm%s\033[0m\n' "$1" "$2" >&3; }          # trace COLOR TEXT
trace_lines() { local l; while IFS= read -r l || [ -n "$l" ]; do trace "$1" "$2$l"; done; }  # stdin
cmd_line() { trace 36 "$1\$ $2"; }
show_io() {   # show_io INDENT RC OUT_FILE ERR_FILE
  trace 90 "$1exit: $2"
  if [ -s "$3" ]; then { jq -ce . "$3" 2>/dev/null || cat "$3"; } | trace_lines 90 "$1out: "; fi
  if [ -s "$4" ]; then trace_lines 90 "$1err: " <"$4"; fi
}

# The recipes call `bb`; point it at $BB (and trace it when VERBOSE is set).
bb() {
  [ "$V" -ge 1 ] && cmd_line "  " "bb $(printf '%q ' "$@")"
  [ "$V" -ge 10 ] || { "$BB" "$@"; return; }
  local rc in="" out err a
  out="$(mktemp "$TMP/bb-out.XXXXXX")"; err="$(mktemp "$TMP/bb-err.XXXXXX")"
  for a in "$@"; do [ "$a" = - ] && in="$(mktemp "$TMP/bb-in.XXXXXX")"; done
  if [ -n "$in" ]; then   # the command reads stdin (--input - / -F -): show it, then pass it on
    cat >"$in"
    { jq -ce . "$in" 2>/dev/null || cat "$in"; } | trace_lines 33 "    in:  "
    "$BB" "$@" <"$in" >"$out" 2>"$err"; rc=$?
  else
    "$BB" "$@" >"$out" 2>"$err"; rc=$?
  fi
  show_io "    " "$rc" "$out" "$err"
  cat "$out"; cat "$err" >&2
  rm -f "$in" "$out" "$err"
  return "$rc"
}

awk '/^```bash$/ { getline first; if (first ~ /^# recipe: /) { on = 1; print first }; next }
     /^```$/ { on = 0 } on' "$SKILL" >"$TMP/recipes.sh"
# shellcheck source=/dev/null
. "$TMP/recipes.sh"

cleanup() {
  cd / || return
  if [ -n "$CREATED" ]; then
    if [ "${KEEP:-0}" = 1 ]; then echo "KEEP=1: kept $FULL"; else bb repo delete "$FULL" --yes >/dev/null 2>&1; fi
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

PASS=0
FAIL=0
pass() { PASS=$((PASS + 1)); printf '  \033[32mPASS\033[0m %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m %s\n       exit %s, out: %s\n       err: %s\n' "$1" "$RC" "$OUT" "$ERR"; }
# t DESC CMD...: run CMD inside the clone; stdout/stderr/exit in OUT/ERR/RC.
t() {
  DESC="$1"; shift
  [ "$V" -ge 1 ] && cmd_line "" "$(printf '%q ' "$@")"
  OUT="$(cd "$CLONE" && "$@" 2>"$TMP/err")"; RC=$?; ERR="$(cat "$TMP/err")"
  if [ "$V" -ge 10 ]; then
    printf '%s' "$OUT" >"$TMP/out"
    show_io "" "$RC" "$TMP/out" "$TMP/err"
  elif [ "$V" -ge 1 ]; then
    trace 90 "exit: $RC"
  fi
  return 0
}
assert_rc() { [ "$RC" -eq "$1" ] && pass "$DESC (exit $1)" || fail "$DESC: expected exit $1"; }
assert_out() { [ "$OUT" = "$1" ] && pass "$DESC: stdout = $1" || fail "$DESC: expected stdout '$1'"; }
assert_out_has() { grep -Eq -- "$1" <<<"$OUT" && pass "$DESC: stdout has /$1/" || fail "$DESC: stdout lacks /$1/"; }
assert_err_has() { grep -Eq -- "$1" <<<"$ERR" && pass "$DESC: stderr has /$1/" || fail "$DESC: stderr lacks /$1/"; }
status() { bb api "/repositories/$FULL/commit/$1/statuses/build" -f key=build -f state="$2" -f name=Build -f url=https://example.com/build >/dev/null; }
state_of() { bb pr view -R "$FULL" "$1" --json | jq -r .state; }

echo "== setup: $FULL"
bb repo create "$FULL" -d "bb skill e2e $RUN_ID" >/dev/null || { echo "repo create failed" >&2; exit 1; }
CREATED=1
bb repo clone "$FULL" "$CLONE" >/dev/null 2>&1 || { echo "repo clone failed" >&2; exit 1; }
git -C "$CLONE" checkout -q -B main
echo hello >"$CLONE/README.md"
git -C "$CLONE" add README.md && git -C "$CLONE" commit -qm "Initial commit"
git -C "$CLONE" push -q -u origin main 2>/dev/null || { echo "git push over SSH failed" >&2; exit 1; }
for b in feature/login feature/login-v2; do
  git -C "$CLONE" checkout -q -b "$b" main
  echo "$b" >"$CLONE/${b//\//-}.txt"
  git -C "$CLONE" add . && git -C "$CLONE" commit -qm "Add $b"
  git -C "$CLONE" push -q -u origin "$b" 2>/dev/null || { echo "push $b failed" >&2; exit 1; }
done
LOGIN_SHA="$(git -C "$CLONE" rev-parse feature/login)"
git -C "$CLONE" checkout -q main

echo "== bb_ensure_pr"
t "creates the PR for feature/login" bb_ensure_pr feature/login main "Login page" "Adds login"
assert_rc 0; assert_out_has "^https://bitbucket.org/$FULL/pull-requests/[0-9]+$"
PR1="${OUT##*/}"
t "second call reuses it" bb_ensure_pr feature/login main "Login page"
assert_rc 0; assert_out "https://bitbucket.org/$FULL/pull-requests/$PR1"; assert_err_has "already open"
t "creates the PR for feature/login-v2" bb_ensure_pr feature/login-v2 main "Login page v2"
assert_rc 0
PR2="${OUT##*/}"

echo "== bb_pr_for_branch"
t "feature/login -> its own PR, not feature/login-v2's" bb_pr_for_branch feature/login
assert_rc 0; assert_out "$PR1"
t "feature/login-v2" bb_pr_for_branch feature/login-v2
assert_rc 0; assert_out "$PR2"
t "branch without a PR" bb_pr_for_branch main
assert_rc 1

echo "== bb_checks_state / bb_pr_summary"
t "no checks yet" bb_checks_state "$PR1"
assert_rc 0; assert_out none
status "$LOGIN_SHA" INPROGRESS
t "in progress" bb_checks_state "$PR1"
assert_rc 0; assert_out pending
t "summary" bb_pr_summary "$PR1"
assert_rc 0; assert_out_has '^state: OPEN$'; assert_out_has '^checks: pending$'

echo "== bb_upsert_comment"
t "first call posts a comment" bb_upsert_comment "$PR1" '[agent:e2e]' 'first'
assert_rc 0
t "second call edits it" bb_upsert_comment "$PR1" '[agent:e2e]' 'second'
assert_rc 0; assert_err_has 'updated comment'
t "one marked comment, with the new text" bb pr comments -R "$FULL" "$PR1" --json
n="$(jq '[.[] | select(.deleted != true and (.content.raw | startswith("[agent:e2e]")))] | length' <<<"$OUT")"
txt="$(jq -r '[.[] | select(.deleted != true and (.content.raw | startswith("[agent:e2e]")))][0].content.raw' <<<"$OUT")"
[ "$n" = 1 ] && pass "$DESC: count = 1" || fail "$DESC: count = $n"
grep -q second <<<"$txt" && pass "$DESC: text updated" || fail "$DESC: text is '$txt'"

echo "== bb_safe_merge"
t "pending checks: no merge" bb_safe_merge "$PR1" --merge
assert_rc 8
[ "$(state_of "$PR1")" = OPEN ] && pass "$DESC: still OPEN" || fail "$DESC: PR state changed"
status "$LOGIN_SHA" FAILED
t "failed checks: no merge" bb_safe_merge "$PR1" --merge
assert_rc 1; assert_err_has failing
[ "$(state_of "$PR1")" = OPEN ] && pass "$DESC: still OPEN" || fail "$DESC: PR state changed"
status "$LOGIN_SHA" SUCCESSFUL
t "green checks: merge" bb_safe_merge "$PR1" --merge
assert_rc 0
[ "$(state_of "$PR1")" = MERGED ] && pass "$DESC: MERGED" || fail "$DESC: not merged"
[ "$(state_of "$PR2")" = OPEN ] && pass "$DESC: the other PR is untouched" || fail "$DESC: PR $PR2 changed"
t "merging it again is refused" bb_safe_merge "$PR1" --merge
assert_rc 1; assert_err_has 'MERGED, not OPEN'
t "feature/login now has no open PR" bb_pr_for_branch feature/login
assert_rc 1

echo "== bb_failed_logs"
t "no pipelines in this repository" bb_failed_logs feature/login-v2
assert_rc 1

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
