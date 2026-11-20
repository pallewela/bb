#!/usr/bin/env bash
# End-to-end test of every bb command against real Bitbucket Cloud.
#
# What it does:
#   1. Creates a throwaway private repository `bb-e2e-<timestamp>` in $WORKSPACE.
#   2. Pushes commits and branches to it over SSH, opens pull requests, comments, reviews,
#      posts build statuses, merges and declines them, and runs/cancels Pipelines.
#   3. Checks every bb command's output and exit code with asserts.
#   4. Deletes the throwaway repositories on exit (set KEEP=1 to keep them for inspection).
#
# It never touches any other repository, and never touches your saved login: the
# login/logout tests use a temporary config directory (see section 1).
#
# Requirements:
#   - bb logged in (`bb auth login`) or BITBUCKET_EMAIL/BITBUCKET_TOKEN set. The token needs the
#     repository (read, write, admin, delete), pull request (read, write), pipeline (read, write),
#     read:workspace:bitbucket and read:user scopes.
#   - git with SSH access to bitbucket.org (`ssh -T git@ssh.bitbucket.org` works), jq, timeout.
#   - Pipelines must be allowed in the workspace. Section 9 uses roughly 10 build minutes;
#     set SKIP_PIPELINES=1 to skip it.
#
# Usage:
#   scripts/e2e.sh                     # uses `bb` from PATH
#   BB=target/debug/bb scripts/e2e.sh  # test a development build
#   KEEP=1 scripts/e2e.sh              # keep the test repository afterwards
#   VERBOSE=1 scripts/e2e.sh           # print each command before running it
#   VERBOSE=10 scripts/e2e.sh          # ...and its stdin input, exit code, stdout and stderr
#
# Exit status: 0 if every assert passed, 1 otherwise.

WORKSPACE="dupelk"

BB="${BB:-bb}"
case "$BB" in */*) BB="$(cd "$(dirname "$BB")" && pwd)/$(basename "$BB")" ;; esac
RUN_ID="$(date +%Y%m%d%H%M%S)"
REPO="bb-e2e-$RUN_ID"            # main test repository
REPO2="bb-e2e-$RUN_ID-tmp"       # second repository, for `repo create -c` and `repo delete`
FULL="$WORKSPACE/$REPO"
WEB="https://bitbucket.org/$FULL"
TAG="E2E-$RUN_ID"                # unique text in PR titles, for `pr list -S`

for tool in git jq timeout; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool" >&2; exit 1; }
done
# Absolute path, so the bb() wrapper below doesn't call itself (infinite recursion -> segfault).
BB="$(command -v "$BB")" || { echo "bb not found (set BB=path/to/bb)" >&2; exit 1; }

TMP="$(mktemp -d "${TMPDIR:-/tmp}/bb-e2e.XXXXXX")"
chmod 700 "$TMP"
ERRF="$TMP/stderr"
OPENED="$TMP/opened-url"
CLONE="$TMP/clone"      # working clone: commits, branches, PRs are made here
CLONE2="$TMP/clone2"    # second clone, for `pr checkout`

# --web / browser-opening commands call `open` (macOS) or `xdg-open` (Linux). Put stubs first on
# PATH that record the URL instead of opening a browser, so the URL can be asserted.
mkdir -p "$TMP/bin"
for opener in open xdg-open; do
  printf '#!/bin/sh\nfor a; do last="$a"; done\nprintf "%%s\\n" "$last" > "%s"\n' "$OPENED" > "$TMP/bin/$opener"
  chmod +x "$TMP/bin/$opener"
done
export PATH="$TMP/bin:$PATH"

# Commits made by this script use a throwaway identity, not your git config.
export GIT_AUTHOR_NAME="bb e2e" GIT_AUTHOR_EMAIL="bb-e2e@example.invalid"
export GIT_COMMITTER_NAME="bb e2e" GIT_COMMITTER_EMAIL="bb-e2e@example.invalid"

# ---------------------------------------------------------------------------------------------
# Test helpers
# ---------------------------------------------------------------------------------------------

PASS=0
FAIL=0
FAILED=()
CREATED_REPOS=()
TOKEN=""

# Verbose output (VERBOSE=1: commands; VERBOSE=10: also their stdin, exit code, stdout and stderr).
# Colors, as in scripts/skill-e2e.sh: cyan = command, yellow = input (stdin), gray = exit and output.
# Input is shown on fd 3 (the script's stdout), so it appears even inside $(...).
exec 3>&1
mask() { if [ -n "$TOKEN" ]; then printf '%s' "${1//"$TOKEN"/<token>}"; else printf '%s' "$1"; fi; }
# with_stdin CMD...: run CMD; at VERBOSE>=10, if it reads stdin (an argument `-` or --with-token),
# show that input first (token masked) and then pass it on.
with_stdin() {
  local a reads=""
  for a in "$@"; do case "$a" in -|--with-token) reads=1 ;; esac; done
  if [ -z "$reads" ] || ! [ "${VERBOSE:-0}" -ge 10 ] 2>/dev/null || [ -t 0 ]; then "$@"; return; fi
  local in; in="$(cat; echo x)"; in="${in%x}"
  mask "$in" | while IFS= read -r a || [ -n "$a" ]; do printf '  \033[33m  in:   %s\033[0m\n' "$a" >&3; done
  printf '%s' "$in" | "$@"
}
bb() { with_stdin "$BB" "$@"; }

# t DESCRIPTION COMMAND...: run COMMAND; keep its stdout in OUT, stderr in ERR, exit code in RC.
# The asserts below check these and are labelled with DESCRIPTION.
t() {
  DESC="$1"; shift
  if [ -n "${VERBOSE:-}" ]; then
    printf '  \033[36m$ %s\033[0m\n' "$(mask "$(printf '%q ' "$@")")"
  fi
  rm -f "$OPENED"
  OUT="$("$@" 2>"$ERRF")"
  RC=$?
  ERR="$(cat "$ERRF")"
  if [ "${VERBOSE:-0}" -ge 10 ] 2>/dev/null; then
    local o e
    o="$(mask "$OUT")"; e="$(mask "$ERR")"
    printf '  \033[90m  exit: %s\033[0m\n' "$RC"
    [ -n "$o" ] && printf '%s\n' "$o" | sed $'s/^/  \033[90m  out:  /; s/$/\033[0m/'
    [ -n "$e" ] && printf '%s\n' "$e" | sed $'s/^/  \033[90m  err:  /; s/$/\033[0m/'
  fi
}

# Run a command in another directory (bb finds the repository from the cwd's git remote).
in_dir() { local d="$1"; shift; (cd "$d" && "$@"); }

section() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

# Print the last command's output on failure, with the API token masked.
show() {
  local o="$OUT" e="$ERR"
  if [ -n "$TOKEN" ]; then o="${o//"$TOKEN"/<token>}"; e="${e//"$TOKEN"/<token>}"; fi
  printf '       exit: %s\n' "$RC"
  [ -n "$o" ] && printf '%s\n' "$o" | head -15 | sed 's/^/       out: /'
  [ -n "$e" ] && printf '%s\n' "$e" | head -10 | sed 's/^/       err: /'
}

pass() { PASS=$((PASS + 1)); printf '  \033[32mPASS\033[0m %s\n' "$1"; }
fail() { FAIL=$((FAIL + 1)); FAILED+=("$1"); printf '  \033[31mFAIL\033[0m %s\n' "$1"; show; }

# assert_rc N: the command exited with code N.
assert_rc() { if [ "$RC" -eq "$1" ]; then pass "$DESC (exit $1)"; else fail "$DESC: expected exit $1, got $RC"; fi; }
# assert_out REGEX / refute_out REGEX: stdout matches / does not match an extended regex.
assert_out() { if grep -Eq -- "$1" <<<"$OUT"; then pass "$DESC: stdout has /$1/"; else fail "$DESC: stdout lacks /$1/"; fi; }
refute_out() { if grep -Eq -- "$1" <<<"$OUT"; then fail "$DESC: stdout unexpectedly has /$1/"; else pass "$DESC: stdout lacks /$1/"; fi; }
# assert_err REGEX / refute_err REGEX: the same for stderr.
assert_err() { if grep -Eq -- "$1" <<<"$ERR"; then pass "$DESC: stderr has /$1/"; else fail "$DESC: stderr lacks /$1/"; fi; }
refute_err() { if grep -Eq -- "$1" <<<"$ERR"; then fail "$DESC: stderr unexpectedly has /$1/"; else pass "$DESC: stderr lacks /$1/"; fi; }
# assert_eq ACTUAL EXPECTED LABEL: two strings are equal.
assert_eq() { if [ "$1" == "$2" ]; then pass "$DESC: $3 = $2"; else fail "$DESC: $3: expected '$2', got '$1'"; fi; }
# assert_json JQ_FILTER: the jq filter is true for stdout.
assert_json() { if jq -e "$1" >/dev/null 2>&1 <<<"$OUT"; then pass "$DESC: json $1"; else fail "$DESC: json not $1"; fi; }
# assert_opened REGEX: a --web command "opened" a URL matching REGEX (recorded by the stub opener).
assert_opened() {
  local url; url="$(cat "$OPENED" 2>/dev/null)"
  if grep -Eq -- "$1" <<<"$url"; then pass "$DESC: opened $url"; else fail "$DESC: opened '$url', expected /$1/"; fi
}
# rows: number of non-header table rows in stdout.
rows() { local n; n="$(grep -c . <<<"$OUT")"; echo $((n > 0 ? n - 1 : 0)); }

# Latest commit hash of a branch in the working clone.
sha() { git -C "$CLONE" rev-parse "$1"; }

# Commit one file in the working clone: commit_file PATH CONTENT MESSAGE
commit_file() {
  mkdir -p "$CLONE/$(dirname "$1")"
  printf '%s\n' "$2" >"$CLONE/$1"
  git -C "$CLONE" add "$1" && git -C "$CLONE" commit -qm "$3"
}

# Start a new branch from the latest main and push one commit to it: new_branch NAME FILE
new_branch() {
  git -C "$CLONE" checkout -q main && git -C "$CLONE" pull -q --ff-only origin main &&
    git -C "$CLONE" checkout -q -b "$1" && commit_file "$2" "$1 content" "Add $2" &&
    git -C "$CLONE" push -q -u origin "$1" 2>/dev/null
}

# Post a build status for a commit (shown by `bb pr checks`): status SHA KEY STATE
status() {
  bb api "/repositories/$FULL/commit/$1/statuses/build" -f key="$2" -f state="$3" \
    -f name="E2E $2" -f url="https://example.com/$2" >/dev/null
}

pr_state() { bb --json pr view -R "$FULL" "$1" | jq -r .state; }

cleanup() {
  cd / || return
  if [ "${KEEP:-0}" = 1 ]; then
    [ ${#CREATED_REPOS[@]} -gt 0 ] && echo "KEEP=1: kept ${CREATED_REPOS[*]}"
  else
    for r in "${CREATED_REPOS[@]}"; do bb repo delete "$r" --yes >/dev/null 2>&1; done
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

summary() {
  printf '\n\033[1m== Summary\033[0m\n  %d passed, %d failed\n' "$PASS" "$FAIL"
  for f in "${FAILED[@]}"; do printf '  - %s\n' "$f"; done
  [ "$FAIL" -eq 0 ]
}

# ---------------------------------------------------------------------------------------------
section "0. Basics: version, help, usage errors"
# ---------------------------------------------------------------------------------------------

# `bb --version` prints the version.
t "bb --version" bb --version
assert_rc 0; assert_out '^bb [0-9]+\.[0-9]+\.[0-9]+'

# `bb --help` lists the top-level commands.
t "bb --help" bb --help
assert_rc 0; assert_out 'pr'; assert_out 'workflow'; assert_out 'browse'

# clap usage errors exit 2, as in gh: an unknown command, and two merge strategies at once.
t "unknown command" bb no-such-command
assert_rc 2
t "pr merge -m -s (conflicting strategies)" bb pr merge 1 -m -s
assert_rc 2

# ---------------------------------------------------------------------------------------------
section "1. auth: status, token, login, logout"
# ---------------------------------------------------------------------------------------------

# `bb auth status` shows where the credentials come from and checks the token with the API.
t "auth status" bb auth status
assert_rc 0; assert_out 'Token: +valid'
EMAIL="$(sed -nE 's/^Email: +//p' <<<"$OUT")"
assert_eq "$([ -n "$EMAIL" ] && echo yes)" yes "email shown"

# `bb auth token` prints the API token (kept in memory only; never printed by this script).
t "auth token" bb auth token
assert_rc 0
TOKEN="$OUT"
assert_eq "$([ ${#TOKEN} -gt 20 ] && echo yes)" yes "token is non-empty"

# `bb auth status -t` also prints the token.
t "auth status -t" bb auth status -t
assert_rc 0
assert_eq "$(grep -cF -- "$TOKEN" <<<"$OUT")" 1 "token shown once"

# A wrong token is reported as invalid, with a non-zero exit.
t "auth status (bad token)" env BITBUCKET_EMAIL="$EMAIL" BITBUCKET_TOKEN=not-a-real-token "$BB" auth status
assert_out 'Token: +invalid'; assert_eq "$([ "$RC" -ne 0 ] && echo yes)" yes "non-zero exit"

# login/logout run against a temporary config directory with the env vars removed, so the real
# saved login is never touched. CAUTION: `logout` (and a repeated `login`) delete the keychain
# entry for the email in hosts.json, so the logout test points hosts.json at a dummy email first.
CFG="$TMP/config"
iso() { with_stdin env -u BITBUCKET_EMAIL -u BITBUCKET_TOKEN XDG_CONFIG_HOME="$CFG" "$BB" "$@"; }
login_stdin() { printf '%s' "$1" | iso auth login --email "$EMAIL" --with-token --insecure-storage; }

# With no saved login and no env vars, commands exit 4 ("authentication required"), as in gh.
t "no credentials -> exit 4" iso repo list "$WORKSPACE"
assert_rc 4

# `bb auth login --with-token` rejects a wrong token and saves nothing.
t "auth login (bad token)" login_stdin not-a-real-token
assert_rc 1; assert_err 'nothing saved'
assert_eq "$([ -e "$CFG/bb/hosts.json" ] && echo saved || echo none)" none "hosts.json"

# `bb auth login --with-token --insecure-storage` verifies the token and saves it in plaintext.
t "auth login --insecure-storage" login_stdin "$TOKEN"
assert_rc 0; assert_err 'Token saved to .*plaintext'
assert_eq "$(stat -c %a "$CFG/bb/hosts.json" 2>/dev/null || stat -f %Lp "$CFG/bb/hosts.json")" 600 "hosts.json mode"

# `bb auth status` now reports the plaintext file as the source, and API calls work with it.
t "auth status (file login)" iso auth status
assert_rc 0; assert_out 'Credentials: .*hosts\.json \(plaintext\)'; assert_out 'Token: +valid'
t "workspace list (file login)" iso workspace list
assert_rc 0; assert_out "$WORKSPACE"

# `bb auth logout` removes the saved login (dummy email: see CAUTION above).
printf '{"email":"bb-e2e@example.invalid","token":"dummy"}\n' >"$CFG/bb/hosts.json"
t "auth logout" iso auth logout
assert_rc 0; assert_err 'Logged out'
assert_eq "$([ -e "$CFG/bb/hosts.json" ] && echo exists || echo gone)" gone "hosts.json"
t "auth logout (again)" iso auth logout
assert_rc 0; assert_err 'No saved login'

# ---------------------------------------------------------------------------------------------
section "2. org / workspace: list"
# ---------------------------------------------------------------------------------------------

# `bb org list` (gh's name) lists the workspaces you can access.
t "org list" bb org list
assert_rc 0; assert_out '^SLUG +ROLE'; assert_out "^$WORKSPACE "

# `bb workspace ls` / `bb ws list`: Bitbucket's name and the short alias do the same.
t "workspace ls" bb workspace ls
assert_rc 0; assert_out "^$WORKSPACE "
t "ws list" bb ws list
assert_rc 0; assert_out "^$WORKSPACE "

# `--admin` only lists workspaces you administer.
t "org list --admin" bb org list --admin
assert_rc 0; refute_out ' member$'

# `-L 1` limits the list to one workspace, and no "Showing N of M" note is printed.
t "org list -L 1" bb org list -L 1
assert_rc 0; assert_eq "$(rows)" 1 "rows"; refute_err 'Showing'

# `--json` prints the raw API objects as a JSON array.
t "org list --json" bb org list --json
assert_rc 0; assert_json "map(.workspace.slug) | index(\"$WORKSPACE\") != null"

# ---------------------------------------------------------------------------------------------
section "3. repo: create, view, list, clone"
# ---------------------------------------------------------------------------------------------

# `bb repo create WS/REPO -d TEXT` creates a private repository and prints its URL.
t "repo create" bb repo create "$FULL" -d "bb e2e test $RUN_ID"
assert_rc 0; assert_out "^$WEB\$"
[ "$RC" -eq 0 ] && CREATED_REPOS+=("$FULL")
if [ "$RC" -ne 0 ]; then summary; echo "cannot continue without the test repository" >&2; exit 1; fi

# `bb repo view WS/REPO` shows the repository's name, description, visibility and clone URLs.
t "repo view" bb repo view "$FULL"
assert_rc 0; assert_out "name: +$FULL"; assert_out "description: +bb e2e test $RUN_ID"
assert_out 'visibility: +private'; assert_out 'clone \(ssh\): +git@(ssh\.)?bitbucket.org'

# `-R WS/REPO` (global flag) selects the repository too, also as a bitbucket.org URL.
t "repo view -R URL" in_dir "$TMP" bb repo view -R "$WEB"
assert_rc 0; assert_out "name: +$FULL"

# `--json` prints the API object.
t "repo view --json" bb repo view "$FULL" --json
assert_rc 0; assert_json '.is_private == true'; assert_json ".full_name == \"$FULL\""

# `bb repo view -w` opens the repository in the browser.
t "repo view -w" bb repo view "$FULL" -w
assert_rc 0; assert_opened "^$WEB\$"

# Set the repository's language with `bb api -X PUT` so `repo list -l` has something to match.
t "api -X PUT language" bb api "/repositories/$FULL" -X PUT -f language=rust
assert_rc 0; assert_json '.language == "rust"'

# `bb repo list WS` lists the workspace's repositories, most recently updated first.
t "repo list" bb repo list "$WORKSPACE"
assert_rc 0; assert_out '^NAME +VISIBILITY +LANGUAGE +UPDATED'; assert_out "^$FULL +private +rust "

# `bb repo ls` is the same command.
t "repo ls -L 0" bb repo ls "$WORKSPACE" -L 0
assert_rc 0; assert_out "^$FULL "

# Filters: --visibility, -l/--language (case-insensitive), --fork / --source.
t "repo list --visibility private" bb repo list "$WORKSPACE" --visibility private -L 0
assert_rc 0; assert_out "^$FULL "
t "repo list --visibility public" bb repo list "$WORKSPACE" --visibility public -L 0
assert_rc 0; refute_out "^$FULL "
t "repo list -l Rust" bb repo list "$WORKSPACE" -l Rust -L 0
assert_rc 0; assert_out "^$FULL "
t "repo list -l go" bb repo list "$WORKSPACE" -l go -L 0
assert_rc 0; refute_out "^$FULL "
t "repo list --source" bb repo list "$WORKSPACE" --source -L 0
assert_rc 0; assert_out "^$FULL "
t "repo list --fork" bb repo list "$WORKSPACE" --fork -L 0
assert_rc 0; refute_out "^$FULL "

# `-L 1` lists exactly one repository; `--json` prints an array.
t "repo list -L 1" bb repo list "$WORKSPACE" -L 1
assert_rc 0; assert_eq "$(rows)" 1 "rows"
t "repo list --json" bb repo list "$WORKSPACE" --json -L 0
assert_rc 0; assert_json "map(.full_name) | index(\"$FULL\") != null"

# `bb repo clone WS/REPO DIR` clones over SSH (an empty repository at this point).
t "repo clone" bb repo clone "$FULL" "$CLONE"
assert_rc 0
assert_eq "$(git -C "$CLONE" remote get-url origin 2>/dev/null)" "git@ssh.bitbucket.org:$FULL.git" "origin"

# Seed the repository: main gets a README and a bitbucket-pipelines.yml (used in section 9).
# No pipeline runs on push: Pipelines are only enabled in section 9.
git -C "$CLONE" checkout -q -B main
printf 'line 1\nline 2\nline 3\nline 4\n' >"$CLONE/README.md"
cat >"$CLONE/bitbucket-pipelines.yml" <<'YAML'
pipelines:
  branches:
    main:
      - step:
          name: build
          script:
            - echo "BB_E2E main build"
  custom:
    ok:
      - step:
          name: hello
          script:
            - echo "BB_E2E hello ok"
    fail:
      - step:
          name: passes
          script:
            - echo "BB_E2E first step"
      - step:
          name: breaks
          script:
            - echo "BB_E2E about to fail"
            - exit 3
    vars:
      - variables:
          - name: GREETING
          - name: COUNT
      - step:
          name: echo-vars
          script:
            - echo "BB_E2E greeting=$GREETING count=$COUNT"
    slow:
      - step:
          name: sleeper
          script:
            - sleep 300
YAML
git -C "$CLONE" add README.md bitbucket-pipelines.yml
git -C "$CLONE" commit -qm "Initial commit"
git -C "$CLONE" push -q -u origin main 2>/dev/null || { echo "git push over SSH failed" >&2; exit 1; }
MAIN_SHA="$(sha main)"

# `bb repo view -b BRANCH` shows the branch's latest commit.
t "repo view -b main" bb repo view "$FULL" -b main
assert_rc 0; assert_out 'main branch: +main'; assert_out "commit: +${MAIN_SHA:0:12} .*Initial commit"

# `bb repo view` with no argument uses the repository of the current directory.
t "repo view (cwd)" in_dir "$CLONE" bb repo view
assert_rc 0; assert_out "name: +$FULL"

# `bb repo view -w -b main` opens the branch's source view.
t "repo view -w -b main" bb repo view "$FULL" -w -b main
assert_rc 0; assert_opened "^$WEB/src/main/\$"

# `bb repo clone ... -- GIT_ARGS` passes extra arguments to git clone (here a shallow clone).
t "repo clone -- --depth 1" bb repo clone "$FULL" "$TMP/shallow" -- --depth 1
assert_rc 0
assert_eq "$(git -C "$TMP/shallow" rev-parse --is-shallow-repository 2>/dev/null)" true "shallow"

# ---------------------------------------------------------------------------------------------
section "4. api: raw authenticated requests"
# ---------------------------------------------------------------------------------------------

# `bb api PATH` sends GET to https://api.bitbucket.org/2.0/PATH and prints the JSON response.
t "api /user" bb api /user
assert_rc 0; assert_json '.uuid | startswith("{")'
USERNAME="$(jq -r .username <<<"$OUT")"; USER_UUID="$(jq -r .uuid <<<"$OUT")"

# A full URL works too.
t "api full URL" bb api "https://api.bitbucket.org/2.0/repositories/$FULL"
assert_rc 0; assert_json ".full_name == \"$FULL\""

# Fields make the request a POST (as in gh); with -X GET they go into the query string.
t "api GET -f pagelen=1" bb api "/repositories/$WORKSPACE" -X GET -f pagelen=1
assert_rc 0; assert_json '.pagelen == 1'

# `-i` prints the status line and headers before the body.
t "api -i" bb api "/repositories/$FULL" -i
assert_rc 0; assert_out '^HTTP/[0-9.]+ 200'; assert_out '^[Cc]ontent-[Tt]ype:'

# `--silent` prints nothing; `-H` adds a header.
t "api --silent -H" bb api "/repositories/$FULL" --silent -H 'Accept: application/json'
assert_rc 0; assert_eq "$OUT" "" "stdout"

# `--verbose` prints request/response headers to stderr, but never the Authorization header.
t "api --verbose" bb api /user --verbose
assert_rc 0; assert_err 'GET'; refute_err '^> [Aa]uthorization'
assert_eq "$(grep -cF -- "$TOKEN" <<<"$ERR")" 0 "token in stderr"

# `--paginate` follows `next` links and prints one combined array.
t "api --paginate" bb api "/repositories/$FULL/refs/branches?pagelen=1" --paginate
assert_rc 0; assert_json 'type == "array" and length >= 1'

# `-X POST` with fields sends a JSON body; `--input FILE` sends a raw JSON body.
# (Build statuses are created here; `pr checks` uses them in section 7.)
t "api POST -f" bb api "/repositories/$FULL/commit/$MAIN_SHA/statuses/build" -X POST \
  -f key=e2e-main -f state=SUCCESSFUL -f url=https://example.com/main
assert_rc 0; assert_json '.state == "SUCCESSFUL" and .key == "e2e-main"'
printf '{"key":"e2e-input","state":"SUCCESSFUL","url":"https://example.com/input"}' >"$TMP/status.json"
t "api --input" bb api "/repositories/$FULL/commit/$MAIN_SHA/statuses/build" --input "$TMP/status.json"
assert_rc 0; assert_json '.key == "e2e-input"'

# `-F` sends typed fields: integers, true/false/null become JSON values; `@FILE` reads a file.
printf 'from a file' >"$TMP/field.txt"
t "api -F typed fields" bb api "/repositories/$FULL/commit/$MAIN_SHA/statuses/build" \
  -F key=e2e-typed -F state=SUCCESSFUL -F url=https://example.com/typed -F description=@"$TMP/field.txt"
assert_rc 0; assert_json '.description == "from a file"'

# API errors exit 1 with `HTTP <code>: <message>`.
t "api 404" bb api "/repositories/$FULL/refs/branches/no-such-branch"
assert_rc 1; assert_err 'HTTP 404'

# ---------------------------------------------------------------------------------------------
section "5. browse: URLs (printed with -n, or opened)"
# ---------------------------------------------------------------------------------------------

# `bb browse -n` prints the repository URL instead of opening it.
t "browse -n" in_dir "$CLONE" bb browse -n
assert_rc 0; assert_eq "$OUT" "$WEB" "url"

# Without -n the URL is opened in the browser.
t "browse" in_dir "$CLONE" bb browse
assert_rc 0; assert_opened "^$WEB\$"

# A number is a pull request; 7-40 hex characters a commit.
t "browse 12 -n" in_dir "$CLONE" bb browse 12 -n
assert_eq "$OUT" "$WEB/pull-requests/12" "url"
t "browse SHA -n" in_dir "$CLONE" bb browse "${MAIN_SHA:0:7}" -n
assert_eq "$OUT" "$WEB/commits/${MAIN_SHA:0:7}" "url"

# A path opens the file on the current branch; `PATH:LINE` and `PATH:START-END` select lines.
t "browse PATH:LINE -n" in_dir "$CLONE" bb browse README.md:2 -n
assert_eq "$OUT" "$WEB/src/main/README.md#lines-2" "url"
t "browse PATH:START-END -n" in_dir "$CLONE" bb browse README.md:1-3 -n
assert_eq "$OUT" "$WEB/src/main/README.md#lines-1:3" "url"

# `--blame` opens the blame view of a file (and needs a file).
t "browse --blame -n" in_dir "$CLONE" bb browse README.md --blame -n
assert_eq "$OUT" "$WEB/annotate/main/README.md" "url"
t "browse --blame without a path" in_dir "$CLONE" bb browse --blame -n
assert_rc 1; assert_err 'needs a file path'

# `-b BRANCH`, `-c` (the last commit), `--commit=SHA` choose the revision.
t "browse -b -n" in_dir "$CLONE" bb browse -b some-branch -n
assert_eq "$OUT" "$WEB/src/some-branch/" "url"
t "browse -c -n" in_dir "$CLONE" bb browse -c -n
assert_eq "$OUT" "$WEB/src/$MAIN_SHA/" "url"
t "browse --commit=SHA -n" in_dir "$CLONE" bb browse --commit=abc1234 -n
assert_eq "$OUT" "$WEB/src/abc1234/" "url"

# `-s` settings, `-a` Pipelines (the gh Actions analog), `-w` wiki.
t "browse -s -n" in_dir "$CLONE" bb browse -s -n
assert_eq "$OUT" "$WEB/admin" "url"
t "browse -a -n" in_dir "$CLONE" bb browse -a -n
assert_eq "$OUT" "$WEB/pipelines" "url"
t "browse -w -n" in_dir "$CLONE" bb browse -w -n
assert_eq "$OUT" "$WEB/wiki" "url"

# Outside a clone, -R selects the repository and a path opens on its main branch (via the API).
t "browse -R PATH -n" in_dir "$TMP" bb browse -R "$FULL" README.md -n
assert_eq "$OUT" "$WEB/src/main/README.md" "url"

# ---------------------------------------------------------------------------------------------
section "6. pr: create"
# ---------------------------------------------------------------------------------------------

# feature-a has two commits; its PR is used for most of the pr tests.
git -C "$CLONE" checkout -q -b feature-a
commit_file notes/a.txt "alpha line 1" "Add notes file"
commit_file data.lock "lock contents" "Add lock file"
git -C "$CLONE" push -q -u origin feature-a 2>/dev/null
SHA_A="$(sha feature-a)"

# `bb pr create` with neither --title nor --fill is an error.
t "pr create (no title)" in_dir "$CLONE" bb pr create
assert_rc 1; assert_err 'title is required'

# `--dry-run` prints what would be created. `-f/--fill` with several commits takes the title
# from the branch name and lists the commits in the body (as in gh).
t "pr create -f --dry-run" in_dir "$CLONE" bb pr create -f --dry-run
assert_rc 0; assert_out "Would create pull request in $FULL: feature-a → \\(main branch\\)"
assert_out '^title: Feature a$'; assert_out '^- Add notes file$'; assert_out '^- Add lock file$'

# `--fill-first` uses the first commit's message.
t "pr create --fill-first --dry-run" in_dir "$CLONE" bb pr create --fill-first --dry-run
assert_rc 0; assert_out '^title: Add notes file$'

# `--fill-verbose` puts every commit's subject and body in the description.
t "pr create --fill-verbose --dry-run --json" in_dir "$CLONE" bb pr create --fill-verbose --dry-run --json
assert_rc 0; assert_json '.title == "Feature a"'; assert_json '.description | test("Add notes file") and test("Add lock file")'
assert_json '.source.branch.name == "feature-a"'

# `-d` (draft) and `-B BASE` show up in the dry run.
t "pr create -d -B main --dry-run" in_dir "$CLONE" bb pr create -t "Draft title" -d -B main --dry-run
assert_rc 0; assert_out 'Would create pull request \(draft\) in .*feature-a → main'

# `-w` opens Bitbucket's create page instead.
t "pr create -w" in_dir "$CLONE" bb pr create -w
assert_rc 0; assert_opened "^$WEB/pull-requests/new\\?source=feature-a\$"

# `bb pr create -t TITLE -b BODY` creates the PR; with --json the API object is printed.
t "pr create -t -b --json" in_dir "$CLONE" bb pr create -t "$TAG feature A" -b "Body of A" --json
assert_rc 0; assert_json '.state == "OPEN"'; assert_json '.draft == false'
PR_A="$(jq -r .id <<<"$OUT")"
URL_A="$(jq -r .links.html.href <<<"$OUT")"

# `bb pr new` (alias) with `-d` creates a draft PR; `-F FILE` reads the body from a file.
# Without --json, the PR's URL is printed.
new_branch feature-b b.txt
printf 'Body of B from a file\n' >"$TMP/body-b.md"
t "pr new -d -F FILE" in_dir "$CLONE" bb pr new -t "$TAG draft B" -d -F "$TMP/body-b.md"
assert_rc 0; assert_out "^$WEB/pull-requests/[0-9]+\$"
PR_B="$(sed -nE 's#.*/pull-requests/([0-9]+)$#\1#p' <<<"$OUT")"
assert_eq "$(bb --json pr view -R "$FULL" "$PR_B" | jq -r '.draft, .description' | tr '\n' '|')" "true|Body of B from a file|" "draft, body"
git -C "$CLONE" checkout -q feature-a

# ---------------------------------------------------------------------------------------------
section "7. pr: list, view, diff, comment, review, checks, checkout"
# ---------------------------------------------------------------------------------------------

# `bb pr list` lists open PRs: id, title, branch, author, state.
t "pr list" in_dir "$CLONE" bb pr list
assert_rc 0; assert_out '^ID +TITLE +BRANCH +AUTHOR +STATE'
assert_out "^#$PR_A +$TAG feature A +feature-a "; assert_out "^#$PR_B .*OPEN \\(draft\\)"

# `pr ls`, and filters: -H head branch, -B base branch, -S search, -A author, -d drafts only.
t "pr ls -H feature-a" in_dir "$CLONE" bb pr ls -H feature-a
assert_rc 0; assert_out "^#$PR_A "; refute_out "^#$PR_B "
t "pr list -B main" in_dir "$CLONE" bb pr list -B main
assert_rc 0; assert_out "^#$PR_A "; assert_out "^#$PR_B "
t "pr list -S" in_dir "$CLONE" bb pr list -S "draft B"
assert_rc 0; assert_out "^#$PR_B "; refute_out "^#$PR_A "
t "pr list -A" in_dir "$CLONE" bb pr list -A "$USERNAME"
assert_rc 0; assert_out "^#$PR_A "
t "pr list -A (someone else)" in_dir "$CLONE" bb pr list -A no-such-user-e2e
assert_rc 0; assert_err 'No pull requests found'
t "pr list -d" in_dir "$CLONE" bb pr list -d
assert_rc 0; assert_out "^#$PR_B "; refute_out "^#$PR_A "

# `-L 1` limits the list; `--json` prints the API objects; `-w` opens the list in the browser.
t "pr list -L 1" in_dir "$CLONE" bb pr list -L 1
assert_rc 0; assert_eq "$(rows)" 1 "rows"
t "pr list --json" in_dir "$CLONE" bb pr list --json
assert_rc 0; assert_json "map(.id) | sort == ([$PR_A, $PR_B] | sort)"
t "pr list -w" in_dir "$CLONE" bb pr list -w
assert_rc 0; assert_opened "^$WEB/pull-requests/\\?state=OPEN\$"

# `-R` lists another repository's PRs from anywhere.
t "pr list -R" in_dir "$TMP" bb pr list -R "$FULL"
assert_rc 0; assert_out "^#$PR_A "

# `bb pr view` with no argument shows the current branch's PR.
t "pr view (current branch)" in_dir "$CLONE" bb pr view
assert_rc 0; assert_out "^$TAG feature A #$PR_A\$"; assert_out '^OPEN • .* • feature-a → main$'
assert_out '^Body of A$'; assert_out "^$URL_A\$"

# A PR can be given as a number, a branch name, or a URL (the URL picks the repository).
t "pr view ID" in_dir "$CLONE" bb pr view "$PR_A"
assert_rc 0; assert_out "#$PR_A\$"
t "pr view BRANCH" in_dir "$CLONE" bb pr view feature-b
assert_rc 0; assert_out "^$TAG draft B #$PR_B\$"; assert_out '^OPEN \(draft\)'
t "pr view URL" in_dir "$TMP" bb pr view "$URL_A"
assert_rc 0; assert_out "#$PR_A\$"
t "pr view --json" in_dir "$CLONE" bb pr view "$PR_A" --json
assert_rc 0; assert_json ".id == $PR_A and .source.branch.name == \"feature-a\""
t "pr view -w" in_dir "$CLONE" bb pr view "$PR_A" -w
assert_rc 0; assert_opened "^$URL_A\$"
t "pr view (no such branch)" in_dir "$CLONE" bb pr view no-such-branch
assert_rc 1; assert_err 'no pull requests found for branch'

# `bb pr diff` prints the unified diff.
t "pr diff" in_dir "$CLONE" bb pr diff "$PR_A"
assert_rc 0; assert_out '^diff --git a/notes/a.txt b/notes/a.txt'; assert_out '^\+alpha line 1'

# `--name-only` lists changed files; `-e GLOB` excludes files (also from the full diff).
t "pr diff --name-only" in_dir "$CLONE" bb pr diff "$PR_A" --name-only
assert_rc 0; assert_out '^notes/a.txt$'; assert_out '^data.lock$'
t "pr diff --name-only -e" in_dir "$CLONE" bb pr diff "$PR_A" --name-only -e '*.lock'
assert_rc 0; assert_out '^notes/a.txt$'; refute_out 'data.lock'
t "pr diff -e" in_dir "$CLONE" bb pr diff "$PR_A" -e '*.lock'
assert_rc 0; assert_out 'notes/a.txt'; refute_out 'data.lock'

# `--patch` prints one patch per commit; `--color always|never` controls ANSI colours.
t "pr diff --patch" in_dir "$CLONE" bb pr diff "$PR_A" --patch
assert_rc 0; assert_out '^Subject: .*Add notes file'; assert_out '^Subject: .*Add lock file'
t "pr diff --color always" in_dir "$CLONE" bb pr diff "$PR_A" --color always
assert_rc 0; assert_out $'\e\\['
t "pr diff --color never" in_dir "$CLONE" bb pr diff "$PR_A" --color never
assert_rc 0; refute_out $'\e\\['
t "pr diff -w" in_dir "$CLONE" bb pr diff "$PR_A" -w
assert_rc 0; assert_opened "^$URL_A/diff\$"

# `bb pr comment -b TEXT` adds a comment; `bb pr comments` lists them.
t "pr comment -b" in_dir "$CLONE" bb pr comment "$PR_A" -b "first comment $RUN_ID"
assert_rc 0; assert_out "^Commented on pull request #$PR_A\$"
t "pr comments" in_dir "$CLONE" bb pr comments "$PR_A"
assert_rc 0; assert_out "^first comment $RUN_ID\$"

# `--edit-last` replaces your latest comment (needs the read:user scope).
t "pr comment --edit-last" in_dir "$CLONE" bb pr comment "$PR_A" -b "edited comment $RUN_ID" --edit-last
assert_rc 0; assert_out "^Edited your last comment on pull request #$PR_A\$"
t "pr comments (after edit)" in_dir "$CLONE" bb pr comments "$PR_A"
assert_out "^edited comment $RUN_ID\$"; refute_out "first comment"

# `--edit-last` with no comment of yours fails, unless `--create-if-none`.
t "pr comment --edit-last (none)" in_dir "$CLONE" bb pr comment "$PR_B" -b "x" --edit-last
assert_rc 1; assert_err 'no comments'
t "pr comment --edit-last --create-if-none" in_dir "$CLONE" bb pr comment "$PR_B" -b "created $RUN_ID" --edit-last --create-if-none
assert_rc 0; assert_out "^Commented on pull request #$PR_B\$"

# `--file PATH --line N` makes an inline comment on a line of the new version.
t "pr comment inline" in_dir "$CLONE" bb pr comment "$PR_A" -b "inline note" --file notes/a.txt --line 1
assert_rc 0
t "pr comments (inline)" in_dir "$CLONE" bb pr comments "$PR_A"
assert_out ' notes/a.txt:1$'; assert_out '^inline note$'

# `-F -` reads the comment from stdin.
comment_stdin() { printf 'from stdin %s' "$RUN_ID" | bb pr comment "$PR_A" -F -; }
t "pr comment -F -" in_dir "$CLONE" comment_stdin
assert_rc 0

# `--delete-last` deletes your latest comment, but only with --yes.
t "pr comment --delete-last (no --yes)" in_dir "$CLONE" bb pr comment "$PR_A" --delete-last
assert_rc 1; assert_err 'without --yes'
t "pr comment --delete-last --yes" in_dir "$CLONE" bb pr comment "$PR_A" --delete-last --yes
assert_rc 0; assert_out "^Deleted your last comment on pull request #$PR_A\$"
t "pr comments (after delete)" in_dir "$CLONE" bb pr comments "$PR_A"
refute_out "from stdin"; assert_out "edited comment"

# `bb pr view -c` also shows the comments; `bb pr comments --json` prints them as JSON.
t "pr view -c" in_dir "$CLONE" bb pr view "$PR_A" -c
assert_rc 0; assert_out "^edited comment $RUN_ID\$"
t "pr comments --json" in_dir "$CLONE" bb pr comments "$PR_A" --json
assert_rc 0; assert_json 'type == "array" and length >= 2'
t "pr comment -w" in_dir "$CLONE" bb pr comment "$PR_A" -w
assert_rc 0; assert_opened "^$URL_A\$"

# `bb pr review`: one of -a/-r/-c is required; -r and -c need a body.
t "pr review (no action)" in_dir "$CLONE" bb pr review "$PR_A"
assert_rc 2
t "pr review -r (no body)" in_dir "$CLONE" bb pr review "$PR_A" -r
assert_rc 1; assert_err 'body'
t "pr review -c -b" in_dir "$CLONE" bb pr review "$PR_A" -c -b "review comment $RUN_ID"
assert_rc 0; assert_out "^Reviewed pull request #$PR_A\$"

# Approve / request changes. Bitbucket may refuse these on your own PR; if so, the commands
# must still reach the API and report its error, so both outcomes are accepted here.
approved() { bb --json pr view -R "$FULL" "$PR_A" | jq -r --arg u "$USER_UUID" '[.participants[] | select(.user.uuid == $u) | .approved] | any'; }
t "pr review -a" in_dir "$CLONE" bb pr review "$PR_A" -a
if [ "$RC" -eq 0 ]; then
  assert_out "^Approved pull request #$PR_A\$"; assert_eq "$(approved)" true "approved"
  # `bb pr unapprove` removes the approval; `bb pr approve` is the Bitbucket-style shortcut.
  t "pr unapprove" in_dir "$CLONE" bb pr unapprove "$PR_A"
  assert_rc 0; assert_out "^Removed approval from pull request #$PR_A\$"; assert_eq "$(approved)" false "approved"
  t "pr approve" in_dir "$CLONE" bb pr approve "$PR_A"
  assert_rc 0; assert_out "^Approved pull request #$PR_A\$"
  t "pr unapprove (again)" in_dir "$CLONE" bb pr unapprove "$PR_A"
  assert_rc 0
else
  assert_err 'HTTP 4[0-9][0-9]'
fi
t "pr review -r -b" in_dir "$CLONE" bb pr review "$PR_A" -r -b "please change $RUN_ID"
if [ "$RC" -eq 0 ]; then
  assert_out "^Requested changes on pull request #$PR_A\$"
  # `bb pr request-changes --undo` withdraws it; without --undo it requests changes again.
  t "pr request-changes --undo" in_dir "$CLONE" bb pr request-changes "$PR_A" --undo
  assert_rc 0; assert_out "^Withdrew change request on pull request #$PR_A\$"
  t "pr request-changes" in_dir "$CLONE" bb pr request-changes "$PR_A"
  assert_rc 0; assert_out "^Requested changes on pull request #$PR_A\$"
  t "pr request-changes --undo (again)" in_dir "$CLONE" bb pr request-changes "$PR_A" --undo
  assert_rc 0
else
  assert_err 'HTTP 4[0-9][0-9]'
fi

# `bb pr checks` shows the build statuses of the PR's head commit. Exit codes follow gh:
# 0 all passed (or none), 8 some pending, 1 some failed.
t "pr checks (none)" in_dir "$CLONE" bb pr checks "$PR_A"
assert_rc 0; assert_err 'No checks reported'
status "$SHA_A" e2e-build INPROGRESS
t "pr checks (pending)" in_dir "$CLONE" bb pr checks "$PR_A"
assert_rc 8; assert_out 'E2E e2e-build'; assert_err 'pending'

# `--watch` waits until no check is pending: mark the check successful while it watches.
watch_until_success() {
  (sleep 5; status "$SHA_A" e2e-build SUCCESSFUL) &
  timeout 120 "$BB" pr checks "$PR_A" --watch -i 2
}
t "pr checks --watch" in_dir "$CLONE" watch_until_success
assert_rc 0; assert_out 'SUCCESSFUL'
t "pr checks (passed)" in_dir "$CLONE" bb pr checks "$PR_A"
assert_rc 0; assert_out 'E2E e2e-build'

# `--watch --fail-fast` stops at the first failed check even while another is still pending.
status "$SHA_A" e2e-lint INPROGRESS
status "$SHA_A" e2e-test FAILED
t "pr checks --watch --fail-fast" in_dir "$CLONE" timeout 60 "$BB" pr checks "$PR_A" --watch --fail-fast -i 2
assert_rc 1; assert_err 'failed'
t "pr checks (failed)" in_dir "$CLONE" bb pr checks "$PR_A"
assert_rc 1
t "pr checks --json" in_dir "$CLONE" bb pr checks "$PR_A" --json
assert_json 'map(.key) | sort == ["e2e-build", "e2e-lint", "e2e-test"]'
status "$SHA_A" e2e-lint SUCCESSFUL
status "$SHA_A" e2e-test SUCCESSFUL
t "pr checks -w" in_dir "$CLONE" bb pr checks "$PR_A" -w
assert_rc 0; assert_opened "^$URL_A\$"

# `bb pr checkout` fetches the PR's source branch into another clone and switches to it.
git clone -q "git@ssh.bitbucket.org:$FULL.git" "$CLONE2" 2>/dev/null
head2() { git -C "$CLONE2" rev-parse HEAD; }
branch2() { git -C "$CLONE2" branch --show-current; }
t "pr checkout ID" in_dir "$CLONE2" bb pr checkout "$PR_A"
assert_rc 0; assert_eq "$(branch2)" feature-a "branch"; assert_eq "$(head2)" "$SHA_A" "HEAD"

# `bb pr co` (alias) with `-b NAME` uses another local branch name; a branch name works as PR.
t "pr co -b" in_dir "$CLONE2" bb pr co feature-a -b review-a
assert_rc 0; assert_eq "$(branch2)" review-a "branch"; assert_eq "$(head2)" "$SHA_A" "HEAD"

# With a diverged local branch (local-only commit, missing the PR's commits), checkout refuses (fast-forward only) unless `-f` resets it.
git -C "$CLONE2" reset -q --hard origin/main && git -C "$CLONE2" commit -q --allow-empty -m "local only"
t "pr checkout (diverged)" in_dir "$CLONE2" bb pr checkout "$PR_A" -b review-a
assert_eq "$([ "$RC" -ne 0 ] && echo yes)" yes "non-zero exit"
t "pr checkout -f" in_dir "$CLONE2" bb pr checkout "$PR_A" -b review-a -f
assert_rc 0; assert_eq "$(head2)" "$SHA_A" "HEAD"

# `--detach` checks out the PR's head commit without a branch.
t "pr checkout --detach" in_dir "$CLONE2" bb pr checkout "$PR_A" --detach
assert_rc 0; assert_eq "$(branch2)" "" "branch"; assert_eq "$(head2)" "$SHA_A" "HEAD"

# ---------------------------------------------------------------------------------------------
section "8. pr: merge (merge commit, squash, rebase, fast-forward, squash-fast-forward) and close"
# ---------------------------------------------------------------------------------------------

latest_message() { bb api "/repositories/$FULL/commits/main?pagelen=1" | jq -r '.values[0].message'; }
branch_gone() { bb api "/repositories/$FULL/refs/branches/$1" >/dev/null 2>&1 && echo exists || echo gone; }

# `bb pr merge -m -t SUBJECT -b BODY -d` merges with a merge commit, sets its message, deletes
# the source branch on Bitbucket, and (being on that branch) switches to main and deletes it locally.
t "pr merge -m -t -b -d" in_dir "$CLONE" bb pr merge "$PR_A" -m -t "Merge A subject $RUN_ID" -b "Merge A body" -d
assert_rc 0; assert_out "^Merged pull request #$PR_A\$"; assert_out '^Deleted local branch feature-a$'
assert_eq "$(pr_state "$PR_A")" MERGED "state"
assert_eq "$(latest_message | head -1)" "Merge A subject $RUN_ID" "merge commit subject"
assert_eq "$(git -C "$CLONE" branch --show-current)" main "local branch"
assert_eq "$(branch_gone feature-a)" gone "remote feature-a"

# Merging an already-merged PR fails with the API's error.
t "pr merge (already merged)" in_dir "$CLONE" bb pr merge "$PR_A" -m
assert_rc 1; assert_err 'HTTP 4'

# `bb pr close -c COMMENT -d` comments, declines the PR, and deletes its branch on Bitbucket and locally.
t "pr close -c -d" in_dir "$CLONE" bb pr close "$PR_B" -c "closing $RUN_ID" -d
assert_rc 0; assert_out "^Declined pull request #$PR_B\$"; assert_out '^Deleted branch feature-b on Bitbucket$'
assert_eq "$(pr_state "$PR_B")" DECLINED "state"
assert_eq "$(branch_gone feature-b)" gone "remote feature-b"
assert_eq "$(git -C "$CLONE" rev-parse -q --verify feature-b >/dev/null && echo exists || echo gone)" gone "local feature-b"
t "pr comments (closing comment)" in_dir "$CLONE" bb pr comments "$PR_B"
assert_out "^closing $RUN_ID\$"

# `bb pr decline` (Bitbucket's word) is the same as close. -H/-B on create pick the branches.
new_branch feature-f f.txt
git -C "$CLONE" checkout -q main
t "pr create -H -B" in_dir "$CLONE" bb pr create -H feature-f -B main -t "$TAG F" --json
assert_rc 0; assert_json '.source.branch.name == "feature-f" and .destination.branch.name == "main"'
PR_F="$(jq -r .id <<<"$OUT")"
t "pr decline" in_dir "$CLONE" bb pr decline "$PR_F"
assert_rc 0; assert_out "^Declined pull request #$PR_F\$"; assert_eq "$(pr_state "$PR_F")" DECLINED "state"

# `-s` squashes the PR's commits into one commit on main.
new_branch feature-c c.txt
commit_file c2.txt "second" "Second commit on C" && git -C "$CLONE" push -q 2>/dev/null
t "pr create (C)" in_dir "$CLONE" bb pr create -t "$TAG C" --json
PR_C="$(jq -r .id <<<"$OUT")"
t "pr merge -s" in_dir "$CLONE" bb pr merge "$PR_C" -s -t "Squash C $RUN_ID"
assert_rc 0; assert_out "^Merged pull request #$PR_C\$"; assert_eq "$(pr_state "$PR_C")" MERGED "state"
assert_eq "$(latest_message | head -1)" "Squash C $RUN_ID" "squash commit subject"

# `--squash-fast-forward` squashes onto main without a merge commit, if the repository allows it.
new_branch feature-g g.txt
t "pr create (G)" in_dir "$CLONE" bb pr create -t "$TAG G" --json
PR_G="$(jq -r .id <<<"$OUT")"
t "pr merge --squash-fast-forward" in_dir "$CLONE" bb pr merge "$PR_G" --squash-fast-forward -t "Squash FF G $RUN_ID"
if [ "$RC" -eq 0 ]; then
  assert_out "^Merged pull request #$PR_G\$"; assert_eq "$(pr_state "$PR_G")" MERGED "state"
  assert_eq "$(latest_message | head -1)" "Squash FF G $RUN_ID" "squash-fast-forward commit subject"
else
  assert_err 'HTTP 4[0-9][0-9]'  # strategy not enabled for this repository
  bb pr close -R "$FULL" "$PR_G" >/dev/null 2>&1  # so no PR is left open
fi

# `-r` rebases the commits onto main (Bitbucket's rebase_fast_forward), if the repository allows it.
new_branch feature-d d.txt
t "pr create (D)" in_dir "$CLONE" bb pr create -t "$TAG D" --json
PR_D="$(jq -r .id <<<"$OUT")"
t "pr merge -r" in_dir "$CLONE" bb pr merge "$PR_D" -r
if [ "$RC" -eq 0 ]; then
  assert_out "^Merged pull request #$PR_D\$"; assert_eq "$(pr_state "$PR_D")" MERGED "state"
  assert_eq "$(latest_message | head -1)" "Add d.txt" "rebased commit on main"
else
  assert_err 'HTTP 4[0-9][0-9]'  # strategy not enabled for this repository
  bb pr close -R "$FULL" "$PR_D" >/dev/null 2>&1  # so no PR is left open
fi

# `--fast-forward` moves main to the PR's head commit without a merge commit.
new_branch feature-e e.txt
SHA_E="$(sha feature-e)"
t "pr create (E)" in_dir "$CLONE" bb pr create -t "$TAG E" --json
PR_E="$(jq -r .id <<<"$OUT")"
t "pr merge --fast-forward" in_dir "$CLONE" bb pr merge "$PR_E" --fast-forward
assert_rc 0; assert_out "^Merged pull request #$PR_E\$"
assert_eq "$(bb api "/repositories/$FULL/refs/branches/main" | jq -r .target.hash)" "$SHA_E" "main head"
git -C "$CLONE" checkout -q main && git -C "$CLONE" pull -q --ff-only origin main

# `--state` takes gh's values (open, closed, merged, all) plus Bitbucket's declined/superseded.
# closed = merged + declined + superseded, as gh's closed includes merged PRs.
t "pr list -s merged" in_dir "$CLONE" bb pr list -s merged
assert_out "^#$PR_A "; assert_out "^#$PR_C "; refute_out "^#$PR_B "
t "pr list -s declined" in_dir "$CLONE" bb pr list -s declined
assert_out "^#$PR_B "; assert_out "^#$PR_F "; refute_out "^#$PR_A "
t "pr list -s closed" in_dir "$CLONE" bb pr list -s closed
assert_out "^#$PR_A "; assert_out "^#$PR_B "
t "pr list -s all --json" in_dir "$CLONE" bb pr list -s all --json
assert_json "map(.id) | (index($PR_A) != null and index($PR_B) != null and index($PR_E) != null)"
t "pr list -s open" in_dir "$CLONE" bb pr list -s open
assert_rc 0; assert_err 'No pull requests found'
t "pr list -s bogus" in_dir "$CLONE" bb pr list -s bogus
assert_rc 2
t "pr list -s closed -w" in_dir "$CLONE" bb pr list -s closed -w
assert_opened "\\?state=ALL\$"

# A merged PR's branch still resolves to that PR (the most recent PR for the branch).
t "pr view (merged PR by branch)" in_dir "$CLONE" bb pr view feature-e
assert_rc 0; assert_out "#$PR_E\$"; assert_out '^MERGED'

# ---------------------------------------------------------------------------------------------
section "9. run / pipeline / workflow: Bitbucket Pipelines"
# ---------------------------------------------------------------------------------------------

if [ "${SKIP_PIPELINES:-0}" = 1 ]; then
  echo "  (skipped: SKIP_PIPELINES=1)"
else
  # Enable Pipelines for the repository (a typed -F field sends a JSON boolean).
  t "api enable pipelines" bb api "/repositories/$FULL/pipelines_config" -X PUT -F enabled=true
  assert_rc 0; assert_json '.enabled == true'

  # build number of a `--json` start; watch a pipeline until it finishes (max 15 minutes).
  started() { jq -r .build_number <<<"$OUT"; }
  watch() { timeout 900 "$BB" run watch "$1" -i 5 "${@:2}"; }

  # `bb workflow run NAME` runs a custom pipeline (gh's workflow) on the main branch by default.
  t "workflow run ok" in_dir "$CLONE" bb workflow run ok
  assert_rc 0; assert_out '^Started pipeline #[0-9]+$'; assert_out "$WEB/pipelines/results/[0-9]+"
  OK_RUN="$(sed -nE 's/^Started pipeline #([0-9]+)$/\1/p' <<<"$OUT")"

  # `bb run watch N --exit-status` follows the steps until the pipeline finishes; exit 0 on success.
  t "run watch --exit-status (ok)" in_dir "$CLONE" watch "$OK_RUN" --exit-status
  assert_rc 0; assert_out 'hello'

  # `bb run view N` shows the pipeline and its steps.
  t "run view" in_dir "$CLONE" bb run view "$OK_RUN"
  assert_rc 0; assert_out "^Pipeline #$OK_RUN +SUCCESSFUL"; assert_out '^Ref: +main'
  assert_out '^hello +SUCCESSFUL'; assert_out "^URL: +$WEB/pipelines/results/$OK_RUN"
  t "run view --exit-status" in_dir "$CLONE" bb run view "$OK_RUN" --exit-status
  assert_rc 0
  t "run view --json" in_dir "$CLONE" bb run view "$OK_RUN" --json
  assert_json ".pipeline.build_number == $OK_RUN and (.steps | length) == 1"

  # `--log` prints step logs; `-j STEP` limits to one step (unknown steps are an error).
  t "run view --log" in_dir "$CLONE" bb run view "$OK_RUN" --log
  assert_rc 0; assert_out 'BB_E2E hello ok'
  t "run view -j --log" in_dir "$CLONE" bb run view "$OK_RUN" -j HELLO --log
  assert_rc 0; assert_out 'BB_E2E hello ok'
  t "run view -j (no such step)" in_dir "$CLONE" bb run view "$OK_RUN" -j nope
  assert_rc 1; assert_err 'no step'
  # `bb pipeline logs` is Bitbucket-style for the same thing.
  t "pipeline logs --step" in_dir "$CLONE" bb pipeline logs "$OK_RUN" --step hello
  assert_rc 0; assert_out 'BB_E2E hello ok'
  t "run view -w" in_dir "$CLONE" bb run view "$OK_RUN" -w
  assert_rc 0; assert_opened "^$WEB/pipelines/results/$OK_RUN"

  # `bb pipeline run --pipeline NAME -r BRANCH` (Bitbucket-style) runs a custom pipeline that fails.
  t "pipeline run --pipeline fail" in_dir "$CLONE" bb pipeline run --pipeline fail -r main --json
  assert_rc 0
  FAIL_RUN="$(started)"
  t "run watch --exit-status (failing)" in_dir "$CLONE" watch "$FAIL_RUN" --exit-status
  assert_rc 1; assert_err 'FAILED'
  t "run view --exit-status (failed)" in_dir "$CLONE" bb run view "$FAIL_RUN" --exit-status
  assert_rc 1; assert_out '^breaks +FAILED'; assert_out '^passes +SUCCESSFUL'
  # `--log-failed` prints only the failed steps' logs.
  t "run view --log-failed" in_dir "$CLONE" bb run view "$FAIL_RUN" --log-failed
  assert_rc 0; assert_out 'BB_E2E about to fail'; refute_out 'BB_E2E first step'
  t "pipeline logs --failed" in_dir "$CLONE" bb pipeline logs "$FAIL_RUN" --failed
  assert_rc 0; assert_out 'BB_E2E about to fail'

  # Pipeline variables: -f KEY=VALUE as a string, -F KEY=@FILE read from a file.
  printf '42' >"$TMP/count.txt"
  t "workflow run -f -F" in_dir "$CLONE" bb workflow run vars -r main -f GREETING=hello-e2e -F COUNT=@"$TMP/count.txt" --json
  assert_rc 0
  VARS_RUN="$(started)"
  t "run watch (vars)" in_dir "$CLONE" watch "$VARS_RUN" --exit-status
  assert_rc 0
  t "run view --log (vars)" in_dir "$CLONE" bb run view "$VARS_RUN" --log
  assert_out 'BB_E2E greeting=hello-e2e count=42'

  # `bb run cancel N` stops a running pipeline; `bb pipeline stop` is the Bitbucket-style alias.
  t "workflow run slow" in_dir "$CLONE" bb workflow run slow --json
  SLOW_RUN="$(started)"
  t "run cancel" in_dir "$CLONE" bb run cancel "$SLOW_RUN"
  assert_rc 0; assert_out "^Stopping pipeline #$SLOW_RUN\$"
  t "run watch (cancelled)" in_dir "$CLONE" watch "$SLOW_RUN" --exit-status
  assert_rc 1; assert_err 'STOPPED'
  t "workflow run slow (2)" in_dir "$CLONE" bb workflow run slow --json
  SLOW_RUN2="$(started)"
  t "pipeline stop" in_dir "$CLONE" bb pipeline stop "$SLOW_RUN2"
  assert_rc 0; assert_out "^Stopping pipeline #$SLOW_RUN2\$"
  watch "$SLOW_RUN2" >/dev/null 2>&1

  # `bb workflow run` with no name runs the branch's default pipeline (`branches: main`).
  t "workflow run (default pipeline)" in_dir "$CLONE" bb workflow run --json
  assert_rc 0
  MAIN_RUN="$(started)"
  t "run watch (default pipeline)" in_dir "$CLONE" watch "$MAIN_RUN" --exit-status
  assert_rc 0; assert_out 'build'

  # `bb run view` / `bb run watch` with no id use the latest pipeline of the current branch.
  t "run view (latest on branch)" in_dir "$CLONE" bb run view
  assert_rc 0; assert_out "^Pipeline #$MAIN_RUN "

  # `bb run list` lists recent pipelines, newest first; `run ls` and `pipeline list` are the same.
  t "run list" in_dir "$CLONE" bb run list
  assert_rc 0; assert_out '^# +STATUS +REF +TRIGGER +CREATOR +CREATED'
  for n in "$OK_RUN" "$FAIL_RUN" "$VARS_RUN" "$SLOW_RUN" "$MAIN_RUN"; do assert_out "^#$n "; done
  t "pipeline ls -L 2" in_dir "$CLONE" bb pipeline ls -L 2
  assert_rc 0; assert_eq "$(rows)" 2 "rows"; refute_err 'Showing'

  # Filters, with gh's names: -s status, -w workflow (custom pipeline), -b branch, -c commit,
  # -u user, -e event.
  t "run list -s success" in_dir "$CLONE" bb run list -s success
  assert_out "^#$OK_RUN "; refute_out "^#$FAIL_RUN "
  t "run list -s failure" in_dir "$CLONE" bb run list -s failure
  assert_out "^#$FAIL_RUN "; refute_out "^#$OK_RUN "
  t "run list -s cancelled" in_dir "$CLONE" bb run list -s cancelled
  assert_out "^#$SLOW_RUN "; refute_out "^#$OK_RUN "
  t "run list -s neutral (unsupported)" in_dir "$CLONE" bb run list -s neutral
  assert_rc 1; assert_err 'unsupported --status'
  t "run list -w ok" in_dir "$CLONE" bb run list -w ok
  assert_out "^#$OK_RUN "; refute_out "^#$FAIL_RUN "
  t "run list -b main" in_dir "$CLONE" bb run list -b main
  assert_out "^#$OK_RUN "
  t "run list -b other" in_dir "$CLONE" bb run list -b no-such-branch
  refute_out "^#$OK_RUN "
  t "run list -c SHA" in_dir "$CLONE" bb run list -c "$(git -C "$CLONE" rev-parse --short main)"
  assert_rc 0; assert_out "^#$OK_RUN "
  t "run list -u @me" in_dir "$CLONE" bb run list -u @me
  assert_rc 0; assert_out "^#$OK_RUN "
  t "run list -e workflow_dispatch" in_dir "$CLONE" bb run list -e workflow_dispatch
  assert_rc 0; assert_out "^#$OK_RUN "
  t "run list -e push" in_dir "$CLONE" bb run list -e push
  refute_out "^#$OK_RUN "
  t "run list --json" in_dir "$CLONE" bb run list --json
  assert_json "map(.build_number) | index($OK_RUN) != null"
fi

# ---------------------------------------------------------------------------------------------
section "10. repo: create -c, delete"
# ---------------------------------------------------------------------------------------------

# `bb repo create -c` creates a repository and clones it into the current directory.
t "repo create -c" in_dir "$TMP" bb repo create "$WORKSPACE/$REPO2" -c
assert_rc 0; assert_out "^https://bitbucket.org/$WORKSPACE/$REPO2\$"
[ "$RC" -eq 0 ] && CREATED_REPOS+=("$WORKSPACE/$REPO2")
assert_eq "$([ -d "$TMP/$REPO2/.git" ] && echo yes)" yes "cloned"

# `bb repo delete` (current repository) refuses without --yes, then deletes it.
t "repo delete (no --yes)" in_dir "$TMP/$REPO2" bb repo delete
assert_rc 1; assert_err 'without --yes'
t "repo delete --yes" in_dir "$TMP/$REPO2" bb repo delete --yes
assert_rc 0; assert_out "^Deleted $WORKSPACE/$REPO2\$"
t "repo view (deleted)" bb repo view "$WORKSPACE/$REPO2"
assert_rc 1; assert_err 'HTTP 404'

summary
