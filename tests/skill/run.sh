#!/usr/bin/env bash
# Deterministic tests for the agent skill in skills/bitbucket-cli/SKILL.md.
#
# The skill's workflows are shell functions in ```bash blocks that start with `# recipe: NAME`.
# This script extracts them from SKILL.md, runs them against tests/skill/mock-bb (a fake `bb`
# that answers from tests/skill/fixtures/repo.json and logs every call), and asserts that each
# one picks the right pull request / pipeline and makes exactly the writes it should — usually none.
#
# With BB_REAL=path/to/bb it also replays every command the recipes ran against the real CLI,
# without credentials, to check that the real `bb` accepts each command line (exit code != 2).
#
# Usage: tests/skill/run.sh   (needs bash and jq; `cargo test` runs it via tests/skill_recipes.rs)
# Exit status: 0 if every assert passed, 1 otherwise.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
SKILL="$HERE/../../skills/bitbucket-cli/SKILL.md"
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

TMP="$(mktemp -d "${TMPDIR:-/tmp}/bb-skill.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"
ln -s "$HERE/mock-bb" "$TMP/bin/bb"
export PATH="$TMP/bin:$PATH"
export BB_MOCK_LOG="$TMP/calls.log"
FIXTURE="$HERE/fixtures/repo.json"
export BB_MOCK_FIXTURE="$FIXTURE"
ALL="$TMP/all-calls.log"
: >"$ALL"

# Extract the recipes: every ```bash block whose first line is `# recipe: ...`.
awk '/^```bash$/ { getline first; if (first ~ /^# recipe: /) { on = 1; print first }; next }
     /^```$/ { on = 0 } on' "$SKILL" >"$TMP/recipes.sh"
RECIPES="$(sed -n 's/^# recipe: //p' "$TMP/recipes.sh")"
# shellcheck source=/dev/null
. "$TMP/recipes.sh"

PASS=0
FAIL=0
pass() { PASS=$((PASS + 1)); [ -n "${VERBOSE:-}" ] && echo "  ok   $1"; return 0; }
fail() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

# t DESCRIPTION COMMAND...: run COMMAND with a fresh call log; keep stdout/stderr/exit in OUT/ERR/RC.
t() {
  DESC="$1"; shift
  : >"$BB_MOCK_LOG"
  "$@" >"$TMP/out" 2>"$TMP/err"; RC=$?
  OUT="$(cat "$TMP/out")"; ERR="$(cat "$TMP/err")"
  cat "$BB_MOCK_LOG" >>"$ALL"
  [ -n "${VERBOSE:-}" ] && echo "- $DESC" && sed 's/^/      /' "$BB_MOCK_LOG"
  return 0
}
assert_rc() { [ "$RC" -eq "$1" ] && pass "$DESC: exit $1" || fail "$DESC: expected exit $1, got $RC (stderr: $ERR)"; }
assert_out() { [ "$OUT" = "$1" ] && pass "$DESC: stdout = $1" || fail "$DESC: expected stdout '$1', got '$OUT'"; }
assert_out_has() { grep -Eq -- "$1" <<<"$OUT" && pass "$DESC: stdout has /$1/" || fail "$DESC: stdout lacks /$1/: '$OUT'"; }
refute_out_has() { grep -Eq -- "$1" <<<"$OUT" && fail "$DESC: stdout has /$1/: '$OUT'" || pass "$DESC: stdout lacks /$1/"; }
assert_err_has() { grep -Eq -- "$1" <<<"$ERR" && pass "$DESC: stderr has /$1/" || fail "$DESC: stderr lacks /$1/: '$ERR'"; }
writes() { grep '^WRITE' "$BB_MOCK_LOG" | cut -f2- | sed 's/ $//'; }
calls() { grep -c . "$BB_MOCK_LOG"; }
# assert_writes [CMD...]: the exact write commands made, in order (none if no arguments).
assert_writes() {
  local want got
  want="$(printf '%s\n' "$@")"; got="$(writes)"
  [ $# -eq 0 ] && want=""
  [ "$got" = "$want" ] && pass "$DESC: writes = [${want//$'\n'/; }]" || fail "$DESC: expected writes [${want//$'\n'/; }], got [${got//$'\n'/; }]"
}
assert_calls() { [ "$(calls)" -eq "$1" ] && pass "$DESC: $1 bb call(s)" || fail "$DESC: expected $1 bb calls, got $(calls)"; }
# with_fixture JQ_FILTER COMMAND...: run COMMAND against a modified copy of the fixture.
with_fixture() {
  local f="$TMP/fixture-$RANDOM.json"
  jq "$1" "$FIXTURE" >"$f"; shift
  BB_MOCK_FIXTURE="$f" "$@"
}
logged_out() { with_fixture '.logged_in = false' "$@"; }

echo "recipes: $(echo $RECIPES)"

# Bitbucket's list lags after writes: `pr list` still shows #12 as OPEN, `pr view` says MERGED.
STALE='.list_prs = .prs | (.prs[] | select(.id == 12) | .state) = "MERGED"'

echo "bb_pr_is_open"
t "#12 is open from feature/login" bb_pr_is_open 12 feature/login
assert_rc 0; assert_calls 1
t "#12 into main" bb_pr_is_open 12 feature/login main
assert_rc 0
t "#12 is not from feature/login-v2" bb_pr_is_open 12 feature/login-v2
assert_rc 1
t "#12 is not into release/1.0" bb_pr_is_open 12 feature/login release/1.0
assert_rc 1
t "#20 is merged" bb_pr_is_open 20 docs/typo
assert_rc 1
t "not logged in" logged_out bb_pr_is_open 12 feature/login
assert_rc 4

echo "bb_pr_for_branch"
t "feature/login: open PR, not the older merged one or feature/login-v2" bb_pr_for_branch feature/login
assert_rc 0; assert_out 12; assert_writes; assert_calls 2
t "stale list still shows #12 open: pr view wins, no PR" with_fixture "$STALE" bb_pr_for_branch feature/login
assert_rc 1; assert_out ""; assert_err_has "no open pull request"
t "feature/login-v2" bb_pr_for_branch feature/login-v2
assert_rc 0; assert_out 13
t "docs/typo has only a merged PR: refuse" bb_pr_for_branch docs/typo
assert_rc 1; assert_out ""; assert_err_has "no open pull request"
t "...whereas bb's own branch resolution would pick the merged PR (rule 3)" bb pr view docs/typo --json
assert_rc 0; assert_out_has '"state": "MERGED"'
t "hotfix/crash has two open PRs: refuse to guess" bb_pr_for_branch hotfix/crash
assert_rc 1; assert_out ""; assert_err_has "2 open pull requests.*14 15"
t "hotfix/crash into release/1.0" bb_pr_for_branch hotfix/crash release/1.0
assert_rc 0; assert_out 15
t "unknown branch" bb_pr_for_branch no/such-branch
assert_rc 1; assert_out ""
t "not logged in: exit 4, nothing else tried" logged_out bb_pr_for_branch feature/login
assert_rc 4; assert_out ""; assert_calls 1

echo "bb_checks_state"
t "passing" bb_checks_state 12
assert_rc 0; assert_out passing; assert_writes
t "failing (exit 1 with JSON)" bb_checks_state 23
assert_rc 0; assert_out failing; assert_err_has "FAILED Tests"
t "pending (exit 8)" bb_checks_state 24
assert_rc 0; assert_out pending
t "--watch waits until done" bb_checks_state 24 --watch
assert_rc 0; assert_out passing; assert_writes
t "no checks reported" bb_checks_state 25
assert_rc 0; assert_out none
t "missing PR (exit 1 without JSON) is an error, not 'failing'" bb_checks_state 999
assert_rc 1; assert_out ""
t "not logged in" logged_out bb_checks_state 12
assert_rc 4; assert_out ""

echo "bb_pr_summary"
t "summary of #12 is read-only" bb_pr_summary 12
assert_rc 0; assert_writes
assert_out_has '^#12 Login page$'; assert_out_has '^state: OPEN$'; assert_out_has '^approvals: 1$'; assert_out_has '^checks: passing$'
t "draft and change requests are shown" bb_pr_summary 22
assert_out_has '^changes requested: 1$'
t "draft" bb_pr_summary 21
assert_out_has '^state: OPEN \(draft\)$'

echo "bb_failed_logs"
t "feature/red: latest pipeline failed, show its logs" bb_failed_logs feature/red
assert_rc 0; assert_out_has 'rejects_bad_password'; assert_err_has '#41'; assert_writes
t "feature/login: latest passed, old failure #38 not reported" bb_failed_logs feature/login
assert_rc 0; assert_out ""; assert_err_has '#39 .*passed'; assert_calls 1
t "feature/slow: still running" bb_failed_logs feature/slow
assert_rc 0; assert_out ""; assert_err_has 'still running'
t "no pipelines" bb_failed_logs no/such-branch
assert_rc 1

echo "bb_ensure_pr"
t "an open PR exists: reuse it, no write" bb_ensure_pr feature/login main "Login page"
assert_rc 0; assert_out "https://bitbucket.org/acme/widgets/pull-requests/12"; assert_writes
t "same head, other base: reuse the right one" bb_ensure_pr hotfix/crash release/1.0 "Hotfix"
assert_out "https://bitbucket.org/acme/widgets/pull-requests/15"; assert_writes
t "only a merged PR for docs/typo: create a new one after a dry run" bb_ensure_pr docs/typo main "Fix another typo" "Body"
assert_rc 0; assert_out "https://bitbucket.org/acme/widgets/pull-requests/99"
assert_writes "bb pr create -H docs/typo -B main -t Fix\\ another\\ typo -b Body"
grep -q -- '--dry-run' "$BB_MOCK_LOG" && pass "$DESC: dry run first" || fail "$DESC: no dry run"
t "stale list still shows #12 open: don't reuse the merged PR, create one" \
  with_fixture "$STALE" bb_ensure_pr feature/login main "Login page again"
assert_rc 0; assert_out "https://bitbucket.org/acme/widgets/pull-requests/99"
assert_writes "bb pr create -H feature/login -B main -t Login\\ page\\ again -b ''"
t "not logged in: no create attempted" logged_out bb_ensure_pr feature/new main "New"
assert_rc 4; assert_calls 1

echo "bb_upsert_comment"
t "updates the agent's own marked comment (#100), not #101 or another user's #102" \
  bb_upsert_comment 12 '[agent:ci-summary]' 'new summary'
assert_rc 0
assert_writes "bb api -X PUT https://api.bitbucket.org/2.0/repositories/acme/widgets/pullrequests/12/comments/100 --input -"
t "only someone else's marked comment: post a new one" bb_upsert_comment 13 '[agent:ci-summary]' 'summary'
assert_rc 0
assert_writes "bb pr comment 13 -b \$'[agent:ci-summary]\\n\\nsummary'"
t "not logged in: no write" logged_out bb_upsert_comment 12 '[agent:ci-summary]' 'x'
assert_rc 4; assert_writes

echo "bb_safe_merge"
t "open, approved, green: merge by explicit id and strategy" bb_safe_merge 12 --squash
assert_rc 0; assert_writes "bb pr merge 12 --squash"
t "no strategy: refuse before calling bb" bb_safe_merge 12
assert_rc 2; assert_calls 0
t "merged PR: refuse" bb_safe_merge 20 --squash
assert_rc 1; assert_writes; assert_err_has "MERGED, not OPEN"
t "draft: refuse" bb_safe_merge 21 --merge
assert_rc 1; assert_writes; assert_err_has draft
t "changes requested: refuse" bb_safe_merge 22 --merge
assert_rc 1; assert_writes; assert_err_has "changes requested"
t "failing checks: refuse" bb_safe_merge 23 --merge
assert_rc 1; assert_writes; assert_err_has failing
t "pending checks: exit 8, no merge" bb_safe_merge 24 --merge
assert_rc 8; assert_writes
t "no checks: refuse by default" bb_safe_merge 25 --merge
assert_rc 1; assert_writes; assert_err_has "allow-no-checks"
t "no checks with --allow-no-checks: merge" bb_safe_merge 25 --merge --allow-no-checks
assert_rc 0; assert_writes "bb pr merge 25 --merge"
t "unknown PR: no merge" bb_safe_merge 999 --squash
assert_rc 1; assert_writes
t "not logged in: exit 4, no merge" logged_out bb_safe_merge 12 --squash
assert_rc 4; assert_writes

echo "coverage"
for r in $RECIPES; do
  DESC="recipe $r"
  grep -q "^t .* $r " "$0" && pass "$DESC is tested" || fail "$DESC has no test in tests/skill/run.sh"
done
DESC="SKILL.md"
[ -n "$RECIPES" ] && pass "$DESC: recipes extracted" || fail "$DESC: no recipes found"

if [ -n "${BB_REAL:-}" ]; then
  echo "real bb accepts every command the recipes ran ($BB_REAL)"
  mkdir -p "$TMP/home" "$TMP/cfg" "$TMP/cwd"
  cut -f2- "$ALL" | sort -u | while IFS= read -r line; do
    eval "set -- $line"; shift   # drop the leading `bb`; the log was written with printf %q
    (cd "$TMP/cwd" && env -i HOME="$TMP/home" XDG_CONFIG_HOME="$TMP/cfg" PATH=/usr/bin:/bin \
      "$BB_REAL" -R acme/widgets "$@" </dev/null >/dev/null 2>"$TMP/real-err"); rc=$?
    DESC="bb $*"
    if [ "$rc" -eq 2 ]; then fail "$DESC: usage error: $(head -3 "$TMP/real-err")"; else pass "$DESC (exit $rc)"; fi
    echo "$PASS $FAIL" >"$TMP/counts"
  done
  [ -f "$TMP/counts" ] && read -r PASS FAIL <"$TMP/counts"
fi

echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
