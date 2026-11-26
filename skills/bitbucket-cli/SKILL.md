---
name: bitbucket-cli
description: Work with Bitbucket Cloud from an agent using the `bb` CLI — find the right pull request for a branch, read PR status, reviews and CI checks, triage failed Pipelines logs, comment, create or merge pull requests, and list or move repositories between projects without unintended writes. Use when the git remote is bitbucket.org, or the user mentions Bitbucket, a Bitbucket PR, or Pipelines. Not for GitHub (`gh`) or Bitbucket Server/Data Center.
---

# Bitbucket Cloud with `bb`

`bb` is a `gh`-style CLI for Bitbucket Cloud: same command names, flags and exit codes as `gh`
where Bitbucket allows it. Run it inside a clone of the repository, or pass `-R WORKSPACE/REPO`.

## Ground rules

1. **Read with `--json`, parse with `jq`.** `--json` prints the raw Bitbucket API JSON (lists
   print a JSON array). Human tables are for people: their columns and wording can change.
   Notes such as "Showing 30 of 120…" go to stderr and never into `--json` output.
2. **Reads are free; writes need the user's go-ahead.** Run any read-only command (table below)
   whenever it helps. Before a write — create, merge, close, approve, review, comment, pipeline
   run/cancel, repo create/edit/delete, project create/edit/delete, `bb api` with a method other
   than GET — be sure the user asked for that action on that object. Never merge, close or delete
   because it "seems done".
3. **Name the PR by id for every write.** `bb pr merge` with no argument, or with a branch name,
   resolves to *the open PR for that branch, else the most recently updated PR of any state* —
   possibly an old merged one. Resolve the id first (`bb_pr_for_branch` below), check it, then pass
   it explicitly: `bb pr merge 42 --squash`.
4. **Match exactly.** Filter `jq` results on `.source.branch.name == $branch` — never a substring or
   title match: `feature/login` and `feature/login-v2` are different PRs. If more than one PR
   matches, stop and ask (or narrow by base branch); don't pick one.
   **Confirm with `bb pr view ID --json` before acting on a list result.** `pr list` (and its
   `-s`/`-H` filters) can lag for a while after a write: a PR merged seconds ago may still be
   listed as open. `pr view` is current.
5. **Never prompt, never open a browser.** Don't use `bb auth login` (it prompts), `-w/--web`, or
   `bb browse` from an agent. Pass bodies with `-b TEXT` or `-F FILE` (`-F -` reads stdin) —
   there is no editor mode.
6. **Get everything when completeness matters.** Lists stop at 30 items (`run list`: 20). Use
   `-L 0` when you need all of them (e.g. "is there any open PR for this branch?").
7. **Keep secrets out of output.** Never run `bb auth token` or `bb auth status -t`; they print
   the API token.
8. **Check exit codes; don't parse error text.**

| Exit | Meaning | What to do |
|------|---------|------------|
| 0 | Success (`pr checks`: all passed, or none reported) | Continue |
| 1 | Error, *or* `pr checks`: a check failed, *or* `run view/watch --exit-status`: pipeline failed | For `pr checks --json`, stdout is still a JSON array when checks failed; no JSON means a real error |
| 2 | Usage error (bad flag or argument) | Fix the command; don't retry as-is |
| 4 | Not logged in / no usable credentials | Stop. Ask the user to run `bb auth login` or set `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN` |
| 8 | `pr checks`: some checks still in progress | Wait (`bb pr checks ID --watch`) or report "pending" |

## Commands

Read-only — safe to run any time:

    bb auth status                              bb repo view [--json]        bb org list
    bb pr list [-s open|merged|closed|all] [-H HEAD] [-B BASE] [-A USER] [-S TEXT] [-L N] --json
    bb pr view ID --json                        bb pr comments ID --json
    bb pr diff ID [--name-only] [-e GLOB]       bb pr checks ID [--watch] --json
    bb pr create ... --dry-run                  bb run list [-b BRANCH] [-s failure] [-L N] --json
    bb run view N --json                        bb run view N --log-failed
    bb project list [--owner WS] -L 0 --json    bb project view KEY [--owner WS] --json
    bb repo list [WS] [-p KEY] -L 0 --json
    bb api PATH                                 (GET only: no -X, -f, -F or --input)

Writes — only when the user asked for this action:

    bb pr create -H HEAD -B BASE -t TITLE -b BODY [-d] [-r ACCOUNT_ID]
    bb pr merge ID --merge|--squash|--rebase|--fast-forward [-d]     (always pass a strategy)
    bb pr close ID [-c COMMENT]                 bb pr review ID -a | -r -b TEXT | -c -b TEXT
    bb pr approve ID / unapprove ID             bb pr comment ID -b TEXT [--file PATH --line N]
    bb run cancel N                             bb workflow run [NAME] -r BRANCH [-f K=V]
    bb repo create / delete --yes               bb api PATH -X POST|PUT|DELETE ...
    bb repo edit REPO [-p KEY] [-d TEXT]        (`--visibility` also needs --accept-visibility-change-consequences)
    bb project create KEY --title NAME [-d TEXT]     bb project edit KEY [--title NAME] [-d TEXT]
    bb project delete KEY --yes                 (Bitbucket refuses while the project has repositories)

`ls` works for every `list`. `pr decline` = `pr close`; `pipeline` = `run`.

## JSON fields you will need

Pull request (`pr view --json`, items of `pr list --json`):
`.id`, `.title`, `.state` (`OPEN`, `MERGED`, `DECLINED`, `SUPERSEDED`), `.draft`,
`.source.branch.name`, `.destination.branch.name`, `.author.display_name`,
`.participants[] | {approved, state, role}` (`state` is `"changes_requested"` when a reviewer
requested changes), `.links.html.href`, `.updated_on`.

Check (`pr checks --json` items): `.state` (`SUCCESSFUL`, `FAILED`, `INPROGRESS`, `STOPPED`),
`.name`, `.key`, `.url`.

Pipeline (`run list --json` items, newest first): `.build_number`, `.state.name`
(`PENDING`, `IN_PROGRESS`, `COMPLETED`), `.state.result.name` (`SUCCESSFUL`, `FAILED`, `ERROR`,
`STOPPED`), `.target.ref_name`. `run view N --json` prints `{pipeline, steps}`.

Repository (`repo view --json`, items of `repo list --json`): `.full_name`, `.project.key`,
`.is_private`, `.mainbranch.name`, `.links.clone[] | select(.name == "ssh") | .href`.

Project (`project view --json`, items of `project list --json`): `.key`, `.name`,
`.description`, `.is_private`, `.links.html.href`.

Comment (`pr comments --json` items): `.id`, `.content.raw`, `.user.account_id`, `.deleted`,
`.inline.path`, `.links.self.href`.

## Workflows

The shell functions below are tested (`tests/skill/run.sh`) exactly as written here. Define the
ones you need in your shell, or follow the same steps by hand. They print results on stdout and
explanations on stderr, and return `bb`'s exit code on failure (so 4 still means "not logged in").

### Find the pull request for a branch

Use this before any action on "the PR for my branch". It fails rather than guess when there is no
open PR (e.g. only a merged one) or more than one. Each candidate from `pr list` is re-checked with
`pr view`, because the list can still show a just-merged PR as open (rule 4).

```bash
# recipe: bb_pr_is_open
# Usage: bb_pr_is_open ID BRANCH [BASE]  -> exit 0 if `pr view` says ID is OPEN, from BRANCH (into BASE).
bb_pr_is_open() {
  local id="${1:-}" branch="${2:-}" base="${3:-}" pr
  pr="$(bb pr view "$id" --json)" || return
  jq -e --arg b "$branch" --arg base "$base" '.state == "OPEN" and .source.branch.name == $b
    and ($base == "" or .destination.branch.name == $base)' >/dev/null <<<"$pr"
}
```

```bash
# recipe: bb_pr_for_branch
# Usage: bb_pr_for_branch BRANCH [BASE]  -> prints the id of the one OPEN PR from BRANCH.
bb_pr_for_branch() {
  local branch="${1:-}" base="${2:-}" prs id open="" rc
  prs="$(bb pr list -s open -H "$branch" ${base:+-B "$base"} -L 0 --json)" || return
  for id in $(jq -r --arg b "$branch" --arg base "$base" '.[]
      | select(.state == "OPEN" and .source.branch.name == $b and ($base == "" or .destination.branch.name == $base))
      | .id' <<<"$prs"); do
    bb_pr_is_open "$id" "$branch" "$base"; rc=$?
    case "$rc" in 0) open="$open $id" ;; 1) ;; *) return "$rc" ;; esac
  done
  set -- $open
  case "$#" in
    1) echo "$1" ;;
    0) echo "no open pull request from branch $branch${base:+ into $base}" >&2; return 1 ;;
    *) echo "$# open pull requests from branch $branch (ids: $*); say which base branch" >&2; return 1 ;;
  esac
}
```

The current branch is `git branch --show-current`.

### Read the CI checks of a pull request

```bash
# recipe: bb_checks_state
# Usage: bb_checks_state ID [--watch]  -> prints passing | failing | pending | none
bb_checks_state() {
  local id="${1:-}" out rc
  out="$(bb pr checks "$id" ${2:+--watch --fail-fast} --json 2>/dev/null)"; rc=$?
  if ! jq -e 'type == "array"' >/dev/null 2>&1 <<<"$out"; then
    [ "$rc" -ne 0 ] || rc=1
    echo "could not read checks for pull request $id (exit $rc)" >&2
    return "$rc"
  fi
  case "$rc" in
    0) if [ "$(jq length <<<"$out")" -eq 0 ]; then echo none; else echo passing; fi ;;
    1) echo failing; jq -r '.[] | select(.state == "FAILED" or .state == "STOPPED") | "  \(.state) \(.name) \(.url)"' <<<"$out" >&2 ;;
    8) echo pending ;;
    *) return "$rc" ;;
  esac
}
```

`--watch` blocks until no check is in progress (or one fails). Only use it when the user wants
you to wait.

### Summarize a pull request (read-only)

```bash
# recipe: bb_pr_summary
# Usage: bb_pr_summary ID
bb_pr_summary() {
  local id="${1:-}" pr checks
  pr="$(bb pr view "$id" --json)" || return
  jq -r '"#\(.id) \(.title)",
    "state: \(.state)\(if .draft then " (draft)" else "" end)",
    "branch: \(.source.branch.name) -> \(.destination.branch.name)",
    "approvals: \([.participants[]? | select(.approved)] | length)",
    "changes requested: \([.participants[]? | select(.state == "changes_requested")] | length)",
    "url: \(.links.html.href)"' <<<"$pr"
  checks="$(bb_checks_state "$id")" || return
  echo "checks: $checks"
}
```

For the changes themselves: `bb pr diff ID --name-only`, then `bb pr diff ID` (add
`-e '*.lock'` to skip noise). For the discussion: `bb pr comments ID --json`.

### Why did CI fail on a branch? (read-only)

Looks at the **latest** pipeline only, so an old failure that has since been fixed is not
reported as current.

```bash
# recipe: bb_failed_logs
# Usage: bb_failed_logs BRANCH  -> prints the failed steps' logs of the latest pipeline
bb_failed_logs() {
  local branch="${1:-}" list n result
  list="$(bb run list -b "$branch" -L 1 --json)" || return
  n="$(jq -r '.[0].build_number // empty' <<<"$list")"
  [ -n "$n" ] || { echo "no pipelines for branch $branch" >&2; return 1; }
  result="$(jq -r '.[0].state.result.name // .[0].state.name' <<<"$list")"
  case "$result" in
    SUCCESSFUL) echo "latest pipeline #$n on $branch passed" >&2 ;;
    FAILED|ERROR|STOPPED) echo "latest pipeline #$n on $branch: $result" >&2
                          bb run view "$n" --log-failed ;;
    *) echo "latest pipeline #$n on $branch is still running ($result)" >&2 ;;
  esac
}
```

### Open a pull request (write)

Push the branch first (`git push -u origin HEAD`). This reuses an existing open PR instead of
creating a duplicate (confirmed with `pr view`, rule 4), and shows a dry run before creating.

```bash
# recipe: bb_ensure_pr
# Usage: bb_ensure_pr HEAD BASE TITLE [BODY]  -> prints the PR's URL
bb_ensure_pr() {
  local head="${1:-}" base="${2:-}" title="${3:-}" body="${4:-}" prs id url rc
  prs="$(bb pr list -s open -H "$head" -B "$base" -L 0 --json)" || return
  for id in $(jq -r --arg h "$head" --arg b "$base" '.[]
      | select(.state == "OPEN" and .source.branch.name == $h and .destination.branch.name == $b) | .id' <<<"$prs"); do
    bb_pr_is_open "$id" "$head" "$base"; rc=$?
    case "$rc" in 1) continue ;; 0) ;; *) return "$rc" ;; esac
    url="$(jq -r --argjson id "$id" '.[] | select(.id == $id) | .links.html.href' <<<"$prs")"
    echo "pull request already open: $url" >&2
    echo "$url"
    return 0
  done
  bb pr create -H "$head" -B "$base" -t "$title" -b "$body" --dry-run >&2 || return
  bb pr create -H "$head" -B "$base" -t "$title" -b "$body"
}
```

### Post or update a status comment (write)

Agents act with the user's token, so "your last comment" may be one the user wrote by hand —
don't use `bb pr comment --edit-last` for agent updates. Tag agent comments with a marker and
update only a comment that carries it and was written by the same account.

```bash
# recipe: bb_upsert_comment
# Usage: bb_upsert_comment ID MARKER BODY   e.g. MARKER='[agent:ci-summary]'
bb_upsert_comment() {
  local id="${1:-}" marker="${2:-}" body="${2:-}"$'\n\n'"${3:-}" me comments url
  me="$(bb api /user)" || return
  me="$(jq -r '.account_id // empty' <<<"$me")"
  [ -n "$me" ] || { echo "could not read the current user" >&2; return 1; }
  comments="$(bb pr comments "$id" --json)" || return
  url="$(jq -r --arg m "$marker" --arg me "$me" '[.[]
    | select(.deleted != true and .user.account_id == $me and (.content.raw | startswith($m)))]
    | last | .links.self.href // empty' <<<"$comments")"
  if [ -n "$url" ]; then
    jq -n --arg b "$body" '{content: {raw: $b}}' | bb api -X PUT "$url" --input - >/dev/null || return
    echo "updated comment on pull request $id" >&2
  else
    bb pr comment "$id" -b "$body"
  fi
}
```

### Merge a pull request (write)

Only when the user asked to merge *this* PR. Refuses unless it is open, not a draft, has no
change requests, and its checks passed. `--allow-no-checks` accepts a PR with no checks at all.

```bash
# recipe: bb_safe_merge
# Usage: bb_safe_merge ID --squash|--merge|--rebase|--fast-forward [--allow-no-checks]
bb_safe_merge() {
  local id="${1:-}" strategy="${2:-}" allow_none="${3:-}" pr checks
  case "$strategy" in
    --merge|--squash|--rebase|--fast-forward|--rebase-merge|--squash-fast-forward) ;;
    *) echo "pass a merge strategy (--squash, --merge, --rebase, --fast-forward)" >&2; return 2 ;;
  esac
  pr="$(bb pr view "$id" --json)" || return
  if ! jq -e '.state == "OPEN"' >/dev/null <<<"$pr"; then
    echo "pull request $id is $(jq -r .state <<<"$pr"), not OPEN" >&2; return 1
  fi
  if jq -e '.draft == true' >/dev/null <<<"$pr"; then
    echo "pull request $id is a draft" >&2; return 1
  fi
  if jq -e '[.participants[]? | select(.state == "changes_requested")] | length > 0' >/dev/null <<<"$pr"; then
    echo "pull request $id has changes requested" >&2; return 1
  fi
  checks="$(bb_checks_state "$id")" || return
  case "$checks" in
    passing) ;;
    none) [ "$allow_none" = --allow-no-checks ] || { echo "pull request $id has no checks (pass --allow-no-checks)" >&2; return 1; } ;;
    pending) echo "checks on pull request $id are still running" >&2; return 8 ;;
    *) echo "checks on pull request $id are $checks" >&2; return 1 ;;
  esac
  bb pr merge "$id" "$strategy"
}
```

Add `-d` to the final `bb pr merge` only if the user asked to delete the branch.

### Other common tasks

- **Review:** `bb pr review ID -a` (approve), `-r -b TEXT` (request changes), `-c -b TEXT`
  (comment). Inline comment: `bb pr comment ID -b TEXT --file src/x.rs --line 10`.
- **Check out a PR locally:** `bb pr checkout ID` (it runs `git fetch` + `git checkout`; mind
  uncommitted work).
- **Re-run CI:** `bb workflow run -r BRANCH` (default pipeline) or `bb workflow run NAME -r BRANCH`
  (custom pipeline) — a write; it uses build minutes.
- **Projects:** project commands and `-p` take the project's **key** (e.g. `NGPG`), not its name
  ("NGA-Playground"). The workspace is `--owner WS`, else the current repo's. When the user names a
  project, look up the key and stop if there isn't exactly one match:
  `bb project list --owner WS -L 0 --json | jq -r --arg n "$NAME" '.[] | select(.name == $n) | .key'`.
  List its repositories with `bb repo list WS -p KEY -L 0 --json`; move a repository (a write) with
  `bb repo edit WS/REPO -p KEY`.
- **Anything else:** `bb api PATH` (relative to `https://api.bitbucket.org/2.0`), e.g.
  `bb api repositories/WS/REPO/pullrequests/42/activity`. `--paginate` returns one array of all
  pages' `values`.
