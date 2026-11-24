# PRD: `bb` — a `gh`-style CLI for Bitbucket Cloud

## 1. Problem

GitHub users have `gh`: one binary for PRs, repos, CI and raw API calls, usable by
humans and scripts/agents alike. Bitbucket Cloud has no equivalent, so everyday work
means the web UI or hand-rolled `curl` against the REST API.

## 2. Goal

A single Rust binary, `bb`, that mirrors `gh`'s command shape and conventions for
Bitbucket Cloud (`api.bitbucket.org/2.0`). Someone who knows `gh` should be able to
guess the `bb` command.

### 2.1 Principle: stay as close to `gh` as possible

`gh` parity is `bb`'s differentiator. Other Bitbucket CLIs (e.g. `bkt`, Atlassian's `twg`)
cover more ground (Data Center, OAuth, more platforms), but none of them lets `gh`
muscle memory carry over unchanged. Every command and flag decision follows these rules:

- **Same names.** Where `gh` has an equivalent, use its command, subcommand and flag
  names, short flags included (`-R`, `-L`, `-B/--base`, `-H/--head`, `-b/--body`,
  `-t/--title`, `-w/--web`, ...), and the same positional arguments. Check
  `gh <command> --help` before adding or renaming anything.
- **Same behaviour.** Same defaults (e.g. `-L 30` for most lists, `-L 20` for
  `run list`), same streams (data on stdout; hints, prompts and errors on stderr),
  same exit codes, same inference from the current git checkout.
- **Accept what a `gh` user types; keep Bitbucket's words as extras.** Parity is about
  input, not about renaming Bitbucket. Every command, flag and flag value a `gh` user
  would type must work, with `gh`'s meaning. Where Bitbucket is more precise, its
  terms stay as additional names or values; they are never removed to make room for
  `gh`'s. Example: `pr list --state closed` works as in `gh` (merged *and* declined
  PRs), while `--state declined` stays as a Bitbucket extra and output still shows
  `DECLINED`. Likewise `pr close` and `pr decline` are the same command, and
  `run cancel` and `pipeline stop` are the same command.
- **Map, don't invent.** A Bitbucket concept that has a `gh` analog takes the `gh` name
  (Pipelines → `bb run` as well as `bb pipeline`; workspaces → `bb org` as well as
  `bb workspace`; the PR checks list → `bb pr checks`). A Bitbucket-only concept gets
  the shape `gh` uses for similar things: `list` / `view` / `create` / `delete`, `-L`,
  `--json`, `--web`.
- **Bitbucket words for data.** Output shows what Bitbucket calls things (DECLINED,
  workspace, build number, step), because that is what users see in the web UI.
- **Deviations are deliberate and recorded.** Every difference from `gh` is listed under
  "Differences from `gh`" in the command's §5 section, as either an **extra** (a
  Bitbucket addition `gh` lacks) or a **gap** (a `gh` form `bb` can't or doesn't
  support yet, with the reason). An unrecorded difference is a bug. Gaps still to
  close are tracked in [#6](https://github.com/pallewela/bb/issues/6).
- **Matching `gh` beats extra features.** A feature that would need a command shape
  `gh` doesn't have needs a strong reason; prefer the `gh` form even if it's less
  convenient.

## 3. Non-goals (v1)

- Bitbucket Server / Data Center (different API).
- Issues (Bitbucket's issue tracker is disabled on most repos; Jira is used instead).
- Multiple accounts / `auth switch` (so no `-u/--user` flags). OAuth browser login flow,
  shell completions, user-defined aliases (`gh alias`), extensions. Built-in command
  aliases that `gh` has (`ls`, `co`, `new`) are in scope.
- `gh`-style `--json field1,field2` + `--jq`. v1 has a plain `--json` (raw API JSON); pipe to `jq`.
- Interactive prompts/editors beyond `auth login`. Where `gh` prompts for a missing
  argument, `bb` uses a sensible default (e.g. the current branch's PR) or errors.

## 4. Conventions (apply to every command)

| Convention | Behaviour |
|---|---|
| Repo resolution | `-R/--repo WORKSPACE/REPO` (global flag; also accepts a bitbucket.org URL). Otherwise parsed from the `origin` git remote of the cwd. |
| Output | Human-readable aligned tables/text by default. Global `--json` prints the raw API JSON instead (lists print a JSON array). |
| Auth | HTTP Basic `email:api_token` (Atlassian API token; Bearer does **not** work). Sources, in order: `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN` env vars, then the saved login. Storage follows `gh`: the token lives in the OS keychain (macOS Keychain / Windows Credential Manager / Secret Service; service `bb:bitbucket.org`, account = email) and `hosts.json` (mode 0600) holds only the email. As in `gh`, it lives in `$XDG_CONFIG_HOME/bb/` if that is set, else `%APPDATA%\bb\` on Windows, else `~/.config/bb/`. With `--insecure-storage`, or when no keychain is available, the token is written to `hosts.json` in plaintext instead. Keychain calls time out (3s on Linux, where a stuck Secret Service daemon can hang; 60s elsewhere to allow for an OS access prompt) and then fail or fall back rather than hang. |
| Errors | Message on stderr. API errors show `HTTP <code>: <message>`. Exit codes follow `gh`: 1 = error, 4 = authentication required (no usable credentials), 8 = checks pending (`pr checks`). |
| Lists | `-L/--limit N` (default 30 as in `gh`; `run list` 20; `0` = all; `ls` alias on every `list`); pagination followed automatically up to the limit. When `-L` is not given and items are left out, a note on stderr says how many were shown of the total (API `size`) and how to see more; nothing is added to stdout or `--json`. |
| `--web` | Where offered, opens the resource in the browser instead of printing (`open` on macOS, `xdg-open` on Linux, `rundll32 url.dll,FileProtocolHandler` on Windows, which unlike `cmd /c start` keeps `&` in URLs). |

**Differences from `gh` (all commands)**
- *Gap:* `--json` prints raw API JSON; `gh`'s `--json FIELDS`, `--jq` and `--template` are a
  v1 non-goal (§3).
- *Gap:* `-R` takes `WORKSPACE/REPO` or a bitbucket.org URL; there is no `HOST/` prefix (Cloud only).
- *Gap:* no exit code 2 ("cancelled"): `bb` has no interactive prompts to cancel (§3).

## 5. Commands

### 5.1 `bb auth`
| Command | Behaviour |
|---|---|
| `auth login [--insecure-storage]` | Prompt for email + API token (or `--email`, `--with-token` reading token from stdin). Verify against the API, then save (keychain by default; replaces any previous login). Reports where the token was stored. |
| `auth status [-t/--show-token]` | Show where creds come from (env / keychain / plaintext file), the account if readable, and whether the token works. `-t` also prints the token. |
| `auth logout` | Delete the saved login: keychain entry and `hosts.json`. |
| `auth token` | Print the token (for scripting). |

Token verification: `GET /user`; if that 403s (scoped token lacking `read:user`),
treat the token as valid — a bad token gets 401, so 403 proves it authenticated.

**Differences from `gh`**
- *Extra:* `auth login --email`. Bitbucket API tokens authenticate as `email:token`, so the email is required.
- *Gap:* no `auth login -w/--web` (OAuth is a v1 non-goal), `-p/--git-protocol` (git is always SSH,
  see `repo clone`) or `-s/--scopes` (scopes are chosen when the token is created).
- *Gap:* no `-u/--user` on `logout`/`token`, and no `auth status -a` (single account, §3).

### 5.2 `bb repo`
| Command | API |
|---|---|
| `repo list [WORKSPACE] [--visibility public\|private] [-l LANG] [--fork\|--source] [-L N]` (alias `ls`) | `GET /repositories/{ws}?q=…`. No workspace: the current repo's workspace (Bitbucket removed cross-workspace listing). Filters run server-side: `is_private`, `language` (lowercased; Bitbucket stores and compares languages in lowercase), `parent != null` / `parent = null`. Columns: full name, visibility, language, updated. |
| `repo view [REPO] [-w/--web] [-b/--branch B]` | `GET /repositories/{ws}/{slug}` — name, description, main branch, visibility, clone URLs, web URL. `-b` adds `GET …/refs/branches/{B}`: the branch, its head commit, and its URL (`--json` still prints the repository). `-w -b B` opens `/src/B/`. |
| `repo clone WORKSPACE/REPO [DIR] [-- git args]` | `git clone git@bitbucket.org:ws/repo.git` (SSH: API tokens don't work for git-over-HTTPS). |
| `repo create WORKSPACE/REPO [--private/--public] [-d/--description] [-p/--project KEY] [-c/--clone]` (alias `new`) | `POST /repositories/{ws}/{slug}` with `scm: git`. Private by default. |
| `repo delete [REPO] --yes` | `DELETE /repositories/{ws}/{slug}`. Refuses without `--yes`. |

**Differences from `gh`**
- `repo list [WORKSPACE]` matches `gh repo list [OWNER]`, but with no argument it uses the current
  repo's workspace rather than "your" repos, because Bitbucket removed the cross-workspace listing.
- *Extra:* `repo create -p/--project KEY` (Bitbucket projects). `gh repo create -p` doesn't exist, so there's no clash.
- *Gap:* `repo create` is private by default. `gh` makes you choose `--public`/`--private` or prompts;
  `bb` doesn't prompt (§3), and private is the safe default.
- *Gap:* `repo create` needs `WORKSPACE/REPO`; there is no default owner.
- *Gap:* `repo delete` refuses without `--yes` instead of prompting (§3).
- *Gap:* `repo clone` is SSH only (API tokens don't work for git over HTTPS); no fork/upstream flags.
- *Gap:* `repo list --archived`/`--no-archived`: Bitbucket repositories have no archived flag (the API
  has no such field, and `q` filtering on `archived`/`is_archived` is rejected).
- *Gap:* `repo list --visibility internal` (no internal visibility on Bitbucket) and `--topic` (no topics).
- *Gap:* `repo view -b` shows the branch's head commit rather than its README (`bb repo view` shows no README).

### 5.2a `bb workspace` (aliases `bb org`, `bb ws`)
| Command | API |
|---|---|
| `workspace list [--admin] [-L N]` (alias `ls`) | `GET /user/workspaces?sort=slug` (`administrator=true` with `--admin`). Default `-L 30`. Columns: slug, role (admin/member). Needs the `read:workspace:bitbucket` scope; a 403 explains that. |

**Differences from `gh`**
- The `gh` analog is `gh org list`; `bb org list` / `bb org ls` work the same way.
- *Extra:* `--admin` (Bitbucket's per-workspace admin flag); `workspace`/`ws` as Bitbucket names.

### 5.3 `bb pr`
`[PR]` argument = numeric id (optionally `#12`), a PR URL, or a branch name, as in `gh`.
A branch (or an omitted `[PR]`, meaning the current git branch) resolves to the open PR
whose source is that branch, else the most recently updated PR from it in any state.

| Command | API (under `/repositories/{ws}/{slug}/pullrequests`) |
|---|---|
| `pr list` (alias `ls`) `[-s STATE] [-A NICK] [-B BASE] [-H HEAD] [-S QUERY] [-d] [-w] [-L N]` | `GET ?state=…&q=…`. `-S` searches title and description (`title ~ … OR description ~ …`); `-d` adds `draft=true`; `-w` opens the PR list page. Columns: id, title, source branch, author, state (+ draft). |
| `pr view [PR] [-c/--comments] [-w/--web]` | `GET /{id}` — title, state, author, branches, reviewers + approval status, description, URL. |
| `pr create` (alias `new`) `[-t TITLE] [-b BODY \| -F FILE] [-B BASE] [-H HEAD] [-d] [-r UUID\|ACCOUNT_ID…] [--close-source-branch] [-f \| --fill-first \| --fill-verbose] [--dry-run] [-w]` | `POST` — head defaults to current branch, base to repo main branch. Fill modes read the commits in `origin/BASE..HEAD` (falling back to the last commit): `--fill` uses one commit's subject/body, or for several commits the humanized branch name as title and a commit list as body; `--fill-first` uses the first commit; `--fill-verbose` puts every commit's subject and body in the description. Explicit `-t`/`-b` win. `--dry-run` prints what would be created. Prints the PR URL. |
| `pr checkout PR` (alias `co`) `[-b LOCAL] [-f] [--detach]` | `git fetch origin +refs/heads/SRC:refs/remotes/origin/SRC`, then: new local branch → `checkout -b LOCAL --track origin/SRC`; existing → `checkout` + `merge --ff-only` (or `checkout -B` with `-f`); `--detach` → detached at `origin/SRC`. |
| `pr diff [PR] [--name-only \| --patch] [--color always\|never\|auto] [-e GLOB…] [-w]` | `GET /{id}/diff`, `/{id}/patch`, or `/{id}/diffstat` for `--name-only`. `-e` drops files whose path or file name matches the glob. |
| `pr merge [PR] [-m \| -s \| -r \| --fast-forward \| --rebase-merge \| --squash-fast-forward] [-t SUBJECT] [-b BODY \| -F FILE] [-d] [--close-source-branch]` | `POST /{id}/merge` with `merge_strategy` (`merge_commit`, `squash`, `rebase_fast_forward`, `fast_forward`, `rebase_merge`, `squash_fast_forward`) and `message` = subject + body. `-d` sets `close_source_branch` and deletes the local branch (switching to the base branch first if it's checked out). |
| `pr close [PR] [-c COMMENT] [-d]` (alias `decline`) | Optional comment, then `POST /{id}/decline`. `-d` deletes the source branch (`DELETE /repositories/{ws}/{slug}/refs/branches/{name}`) and the local branch. |
| `pr review [PR] -a \| -r \| -c [-b BODY \| -F FILE]` | Body (required for `-r`/`-c`) is posted as a comment; then `POST /{id}/approve` or `/{id}/request-changes`. |
| `pr approve [PR]` / `pr unapprove [PR]` | `POST` / `DELETE /{id}/approve`. |
| `pr request-changes [PR] [--undo]` | `POST` / `DELETE /{id}/request-changes`. |
| `pr comment [PR] (-b BODY \| -F FILE) [--file PATH --line N] [--edit-last [--create-if-none]] [--delete-last --yes] [-w]` | `POST /{id}/comments` with `content.raw`, optional `inline: {path, to}`. `--edit-last` / `--delete-last` find your newest comment via `GET /user` (needs `read:user`) and `PUT` / `DELETE /{id}/comments/{cid}`. |
| `pr comments [PR]` | `GET /{id}/comments` — author, location (if inline), body. |
| `pr checks [PR] [--watch [-i SECS] [--fail-fast]] [-w]` | `GET /{id}/statuses` — state, name, URL. `--watch` re-polls (default every 10s) until nothing is INPROGRESS, printing the table when it changes. Exit 1 if any check FAILED or STOPPED, 8 if any is still INPROGRESS. |

**Differences from `gh`**

PR states map as follows. `gh`'s values must be accepted with `gh`'s meaning; Bitbucket's stay as extras.

| `--state` value | Bitbucket states | |
|---|---|---|
| `open` | OPEN | `gh` |
| `merged` | MERGED | `gh` |
| `closed` | MERGED, DECLINED, SUPERSEDED | `gh` (`gh`'s `closed` includes merged) |
| `all` | all four | `gh` |
| `declined` | DECLINED | *extra* |
| `superseded` | SUPERSEDED | *extra* |

- *Extra:* `pr decline` is the Bitbucket name for `gh pr close`; both names work.
- *Extra:* `pr approve`, `pr unapprove` and `pr request-changes [--undo]` are separate Bitbucket
  actions alongside `gh`'s `pr review -a/-r/-c`; `unapprove` and `--undo` have no `gh` equivalent.
- *Extra:* `pr comments` (`gh` only has `pr view --comments`, which `bb` also supports).
- *Extra:* `pr comment --file/--line` for inline comments.
- *Extra:* `pr create --close-source-branch`, `pr merge --fast-forward` and `--close-source-branch`
  (Bitbucket options; `--close-source-branch` keeps the local branch, `-d` deletes it too). `pr merge --message`
  is kept as a hidden alias of `--subject`.
- *Extra:* `pr merge --rebase-merge` and `--squash-fast-forward` (Bitbucket's `rebase_merge` and
  `squash_fast_forward`). `-r/--rebase` uses `rebase_fast_forward` (rebase, no merge commit), the closest
  to `gh`'s rebase.
- *Gap:* `pr create -r/--reviewer` needs a UUID or account_id; `gh` takes a handle. Bitbucket can't look
  users up by nickname: `/workspaces/{ws}/members` only filters by email (and only for admins), so
  resolving a nickname means scanning every member of the workspace (thousands in large ones).
- *Gap:* no labels, assignees or milestones (Bitbucket has none), so `-l`, `-a` and `-m` on
  `pr create`/`pr list` don't apply.
- *Gap:* `pr checks --required`: Bitbucket's statuses don't say which builds a merge check requires.
- *Gap:* `pr merge --auto/--admin/--match-head-commit`, `pr create --no-maintainer-edit/--recover/--attach`,
  `pr comment --attach` and `-e/--editor` everywhere (no interactive editors, §3).
- *Gap:* `pr list -S` is a substring match on title and description, not `gh`'s GitHub search syntax.
- *Gap:* `pr checks` with nothing reported prints "No checks reported" and exits 0.

### 5.4 `bb pipeline` (the `gh run` analog; alias `bb run`)
| Command | API (under `/repositories/{ws}/{slug}/pipelines`) |
|---|---|
`[ID]` = build number (`GET /{build_number}` works in place of the UUID) or UUID; omitted → the
latest pipeline whose branch is the current git branch (`GET /?target.branch=B&sort=-created_on&pagelen=1`).

| Command | API (under `/repositories/{ws}/{slug}/pipelines`) |
|---|---|
| `pipeline list` (alias `ls`) `[-b B] [-s STATUS] [-u USER] [-c SHA] [-w NAME] [-e EVENT] [-L N]` | `GET /?sort=-created_on` — build number, state/result, ref, trigger, creator, created. Default `-L 20`. All filters are server-side query parameters (see below). |
| `pipeline view [ID] [-j STEP] [--log\|--log-failed] [--exit-status] [-w/--web]` | `GET /{id}` + `GET /{id}/steps/` — summary + per-step status/duration (`-j`: one step). `--log`/`--log-failed` print step logs as `pipeline logs` does. |
| `pipeline watch [ID] [--exit-status] [-i N]` | Polls `GET /{uuid}` + `/steps/` every N seconds (default 3), printing each pipeline/step status change, until the pipeline completes. |
| `pipeline run [-r/--ref/--branch B] [--pipeline CUSTOM_NAME] [-f/--raw-field/--var K=V…] [-F/--field K=V…]` | `POST /` with `pipeline_ref_target` (branch defaults to current). Custom pipelines use a `selector: {type: custom, pattern}`; variables go in `variables: [{key, value}]`. |
| `workflow run [NAME] [-r/--ref B] [-f K=V…] [-F K=V…]` | Same `POST /`: NAME is a custom pipeline; no NAME runs the ref's default pipeline. The ref defaults to the repo's main branch, as in `gh`. `-F K=@file` / `@-` reads the value from a file / stdin. |
| `pipeline cancel ID` (alias `stop`) | `POST /{uuid}/stopPipeline`. |
| `pipeline logs [ID] [--step NAME\|UUID] [--failed]` | `GET /{uuid}/steps/{step}/log` — default: all steps in order; `--failed`: only failed steps. |

`run list` filter mapping (all server-side; checked live, repeated values are OR-ed):

| Flag | Query parameter | Values |
|---|---|---|
| `-b/--branch` | `target.branch` | Matches branch, custom-on-branch and pull request pipelines (by source branch), like `gh`'s head branch. |
| `-s/--status` | `status` | `gh`: `queued`/`pending`/`requested` → PENDING+PARSING, `in_progress` → BUILDING+PAUSED+HALTED, `waiting`/`action_required` → PAUSED+HALTED, `completed` → PASSED+FAILED+ERROR+STOPPED, `success` → PASSED, `failure` → FAILED, `cancelled` → STOPPED, `startup_failure` → ERROR. Bitbucket extras: `successful`, `failed`, `error`, `stopped`, `running`, `paused`, `halted`, `parsing`. |
| `-u/--user` | `creator.uuid` | UUID or `@me` (`GET /user`). A nickname, display name or account ID is looked up among the 500 most recent pipelines' creators (the API can't filter by name). |
| `-c/--commit` | `target.commit.hash` | The API needs the full hash; an abbreviated one is expanded with `GET /commit/{sha}`. |
| `-w/--workflow` | `target.selector.pattern` | Custom pipeline name (or branch pattern). |
| `-e/--event` | `trigger_type` / `target.selector.type` | `push` → PUSH, `workflow_dispatch`/`manual` → MANUAL, `schedule` → SCHEDULED, `parent_step` → PARENT_STEP, `pull_request` → selector type PULLREQUESTS (PR pipelines are PUSH-triggered). |

**Differences from `gh`**
- *Extra:* `pipeline` is the Bitbucket name; `run` is the `gh` name, and both work.
- *Extra:* `pipeline logs [--step] [--failed]`, the same as `run view --log / --log-failed / -j`.
- *Extra:* `pipeline stop` is the Bitbucket name for `run cancel`; both work.
- *Extra:* `pipeline run --branch/--pipeline/--var` (and `-r/--ref`, `-f`, `-F`). `gh`'s form is
  `workflow run NAME --ref B -f k=v`, which also works.
- *Extra:* `run list -s` accepts Bitbucket's status names; `-u` accepts a UUID or display name.
- `run view`/`run watch` without an ID use the latest pipeline for the current branch instead of
  prompting (§3). `run view -j` needs the pipeline ID too (steps aren't addressable on their own).
- `run watch` prints status-change lines rather than redrawing the screen, and also stops (exit 0)
  when the pipeline is PAUSED or HALTED waiting for a manual step, since watching can't make it continue.
- `run view -v` is accepted and does nothing (steps are always shown).
- *Gap:* no `run rerun [--failed] [-j] [-d]`. The Bitbucket Cloud REST API has no endpoint to re-run a
  pipeline or its failed steps (the OpenAPI spec only offers list/get/create/stopPipeline under
  `/pipelines`; the `write:pipeline` scope text mentions re-runs, but that is a web-UI-only action).
  Use `workflow run` / `pipeline run` to start a new run.
- *Gap:* no `run list --created`, `-a/--all`; no `run view -a/--attempt`; no `run cancel --force`;
  no `run watch --compact`; no `run delete`, `run download`.
- *Gap:* `workflow run --json` (inputs as JSON on stdin) clashes with the global `--json` flag; use `-f`/`-F`.
  `--ref` takes a branch only (Bitbucket's `ref_type` would need to know it's a tag).
- *Gap:* no `workflow list/view/enable/disable` (custom pipelines live in `bitbucket-pipelines.yml`;
  there is no API to list or toggle them).

### 5.5 `bb api`
`bb api PATH [-X METHOD] [-f/--raw-field key=value…] [-F/--field key=value…] [-H/--header key:value…] [--input FILE|-] [-i/--include] [--silent] [--verbose] [--paginate]`

Authenticated raw request. `PATH` is relative to `/2.0` or a full URL. As in `gh api`:
`-f` adds a string field; `-F` adds a typed field (`true`, `false`, `null` and integers become
JSON values, `@FILE` reads the value from a file and `@-` from stdin). Fields build a JSON object
body and imply POST if `-X` is absent; with `-X GET`, or when `--input` supplies the body, they go
in the query string instead. `-H` adds request headers. `-i` prints the status line and response
headers before the body; `--silent` drops the body; `--verbose` prints the request and response
headers (and the request body) to stderr, never the Authorization header. Output is the response
body, pretty-printed if JSON; on a non-2xx status the body is still printed, then
`HTTP <code>: <message>` goes to stderr and the exit code is 1. `--paginate` follows `next` links
and prints the concatenated `values` array.

**Differences from `gh`**
- *Deliberate:* `--paginate` prints one combined array of every page's `values`, not each page in turn as
  `gh` does. Bitbucket pages are objects that wrap `values`, so printing them as-is would need
  `jq '.values[]'` where `gh` users write `jq '.[]'`; the combined array keeps `jq '.[]'` working and is
  one valid JSON document. `--slurp` is not accepted: `gh`'s gives an array of pages, which `bb` doesn't
  produce, so a copied `gh … --slurp` command fails loudly rather than returning a different shape.
- *Gap:* `--paginate` can't be combined with `-H`, `-i` or `--verbose`.
- *Gap:* no nested field syntax (`key[sub]=v`, `key[]=v`) and no `{owner}`/`{repo}`/`{branch}`
  placeholders; use `--input` for nested bodies. `--input` must be JSON.
- *Gap:* `-i`/`--verbose` show HTTP/1.1 and lowercased header names (as received).
- `--jq`, `--template`, `--cache`, `-p/--preview` and `--hostname` are out of scope (§3; Cloud only).

### 5.6 `bb browse`
`bb browse [NUMBER|PATH|SHA] [-b/--branch B] [-c/--commit[=SHA]] [--blame] [-s/--settings] [-a/--actions] [-w/--wiki] [-n/--no-browser]`
— open the repo on bitbucket.org; `-n` prints the URL instead. As in `gh browse`, the argument is
a PR if it's a number (`/pull-requests/N`), a commit if it's 7 to 40 lowercase hex characters
(`/commits/SHA`), and otherwise a file/dir path (`/src/REV/PATH`, with `PATH:LINE` or
`PATH:START-END` → `#lines-N` / `#lines-START:END`). REV is `--commit` (no SHA: the last commit —
local HEAD in the checkout, else the main branch's head), else `--branch`, else the current
branch in the checkout, else the main branch. `--blame` opens `/annotate/REV/PATH`. `-s` opens
`/admin`, `-a` the Pipelines page (`/pipelines`), `-w` `/wiki`.

**Differences from `gh`**
- *Gap:* `-p/--projects` and `-r/--releases` have no Bitbucket equivalent.
- *Gap:* `--commit` and `--branch` can't be combined (in `gh`, `--commit` wins).

### 5.7 Agent skill (`skills/bitbucket-cli/SKILL.md`)

An agent skill that teaches AI coding agents to use `bb` safely. It adds no new commands; it
documents how agents should use the ones above:
- **Rules:** read with `--json` + `jq`; write only when the user asked for that action; pass the PR
  id explicitly for every write (branch resolution, §5.3, can pick a merged PR); match branches
  exactly; confirm list results with `pr view` before acting (Bitbucket's PR list can lag after a
  write: a PR merged seconds ago may still be listed as open); no prompts, `--web` or `bb browse`;
  `-L 0` when completeness matters; never print the token.
- **Exit codes:** what 1, 2, 4 and 8 mean to an agent, including that `pr checks --json` still
  prints a JSON array when it exits 1 for a failed check.
- **Reference:** read-only vs write commands, and the JSON fields agents need.
- **Workflows** as shell functions in ```` ```bash ```` blocks that start with `# recipe: NAME`:
  `bb_pr_is_open`, `bb_pr_for_branch`, `bb_checks_state`, `bb_pr_summary`, `bb_failed_logs`, `bb_ensure_pr`,
  `bb_upsert_comment`, `bb_safe_merge`. The tests (§7) run these blocks exactly as written.

**The skill is part of `bb`'s interface.** Any change to a command, flag, flag value, default,
`--json` output shape, exit code or PR/pipeline resolution rule that the skill mentions must update
SKILL.md in the same change, along with `tests/skill/mock-bb` and its fixture when the mock
imitates the changed behavior. New commands or flags that agents would use (especially writes)
should get a line in the skill's command lists, and a recipe and tests if they need a safety check.

## 6. Architecture

Single crate, one module per command group, thin shared core.

```
src/main.rs      clap CLI root, global flags, dispatch
src/client.rs    HTTP client: auth header, get/post/put/delete, paginate, error mapping
src/config.rs    credentials: env → keychain/hosts.json; save/delete
src/context.rs   repo resolution (-R or git remote), git helpers
src/output.rs    json mode flag, table printer, JSON field helpers, open-in-browser
src/auth.rs      bb auth          src/pr.rs        bb pr
src/repo.rs      bb repo          src/pipeline.rs  bb pipeline
src/api.rs       bb api           src/browse.rs    bb browse
src/workspace.rs bb workspace    src/exit.rs      exit codes (gh's 1/4/8)

skills/bitbucket-cli/SKILL.md   agent skill (§5.7)
tests/skill/run.sh              skill recipe tests; mock-bb = fake bb, fixtures/ = its data
tests/skill_recipes.rs          runs tests/skill/run.sh under `cargo test`
scripts/e2e.sh                  live test of every command; scripts/skill-e2e.sh: the skill's recipes
scripts/skill-eval.sh           agent eval: Claude Code + the skill against the mock (opt-in, paid)
```

Dependencies: `clap` (derive), `ureq` (blocking HTTP, no async runtime), `serde_json`,
`anyhow`, `base64`, `keyring` (OS credential store). API responses are handled as `serde_json::Value` — no typed model
layer; Bitbucket payloads are large and we read a handful of fields.

## 7. Quality bar

- `cargo build` warning-free; `cargo test` green on macOS, Linux and Windows (CI runs all three;
  the bash-based skill tests are skipped on Windows).
- Unit tests for pure logic: remote-URL parsing, PR-argument parsing, query building, `-f` body building.
- No network in tests. Live verification is a manual read-only smoke test.
- The agent skill's recipes pass against the mock (`tests/skill/run.sh`, part of `cargo test`),
  and the real `bb` accepts every command line they run. That replay catches renamed or removed
  flags, but not changed semantics or output. So a PR that changes `bb`'s behavior also updates
  SKILL.md and the mock (§5.7), and reviewers check for it.
- Changes to SKILL.md's rules or workflows are checked with `scripts/skill-eval.sh`: Claude Code
  given realistic requests must pick the right PR and make only the expected writes. It is not
  deterministic and costs API usage, so it is run by hand (`RUNS=3` or more), not in CI.

## 8. Releases and supported platforms

Every push to `main` that passes CI is released (`.github/workflows/release.yml`): the patch
version is bumped unless the head commit says `[bump:minor]`, `[bump:major]` or `[skip release]`.
Each release publishes one archive per target, built and checked on a native GitHub runner:

| Target | Platform | Runner | Archive | Check before publishing |
|---|---|---|---|---|
| `aarch64-apple-darwin` | macOS, Apple silicon | `macos-latest` | `.tar.gz` | runs `bb --version` |
| `x86_64-apple-darwin` | macOS, Intel | `macos-latest` (cross-built) | `.tar.gz` | `lipo -archs` is `x86_64` (arm64 runner can't run it) |
| `x86_64-unknown-linux-musl` | Linux x86_64, any distro | `ubuntu-latest` | `.tar.gz` | runs `bb --version`; no shared libraries |
| `aarch64-unknown-linux-musl` | Linux arm64, any distro | `ubuntu-24.04-arm` | `.tar.gz` | runs `bb --version`; no shared libraries |
| `x86_64-pc-windows-msvc` | Windows x86_64 | `windows-latest` | `.zip` (`bb.exe`) | runs `bb --version` |

Archives are named `bb-vX.Y.Z-TARGET.{tar.gz,zip}`, contain the binary, `LICENSE` and `README.md`,
and come with a `.sha256` file. The release also carries a signed SLSA build provenance
attestation for all archives (`bb-vX.Y.Z.intoto.jsonl`, made with `actions/attest-build-provenance`
and also stored by GitHub, so `gh attestation verify` works). All of them are published in one
release call, so a release is never partial.

- **Linux binaries are static (musl).** One file works on every distro, old or new, and in Alpine
  or scratch containers; there is no glibc version to match. This is possible because no
  dependency needs a system library: TLS is `rustls`, and the keychain (Secret Service) is
  reached over D-Bus with pure-Rust `zbus`. `ring` compiles some C, hence `musl-tools` on the runner.
- **Native runners, not cross-compilers** (`cross`, `zig`): each binary is run on its own OS
  before it is published. Only Intel macOS is cross-built, because GitHub's macOS runners are arm64.
- **Pull requests build every target too** (without publishing), so a change that breaks one
  platform's build is caught before it reaches `main`.
- **Homebrew** (`pallewela/tap/bb`) covers macOS and Linux, arm64 and x86_64: the release sends
  each archive's SHA256 to the tap, whose workflow rewrites `Formula/bb.rb` with one URL per
  platform.
- **crates.io:** the crate is `bitbucket-bb-cli` (`bb`, `bb-cli` and `bitbucket-cli` belong to
  other projects); `[[bin]] name = "bb"` keeps the command `bb`. It ships only `src/`,
  `Cargo.toml`, `Cargo.lock`, `README.md` and `LICENSE`. After the GitHub release, a separate
  publish run of `release.yml` (`workflow_dispatch` with `publish_tag`) runs `cargo publish` from
  the tag with the same version stamp as the binaries, using crates.io trusted publishing (GitHub
  OIDC, no stored token), when the repository variable `CRATES_IO_PUBLISH` is `true`; a version
  that is already on crates.io is skipped, so re-runs are safe. It is a separate run because
  crates.io refuses trusted-publishing tokens for `workflow_run` runs, which is how releases start.
  The same run bumps the Homebrew tap, in its own job. The first version is published by hand,
  since trusted publishing can only be set up for a crate that exists. Published versions can be
  yanked but never deleted.
- **Windows** installs from the `.zip` by hand for now. *Gap:* no Scoop or winget package yet, and
  the binary is unsigned, so SmartScreen may warn the first time it runs.
- Not built: Windows arm64, 32-bit targets, FreeBSD. They can be added as matrix rows when asked for.

## 9. Milestones

1. Core (client, config, context, output, CLI skeleton) — the contract the rest builds on.
2. In parallel: `auth` + `api` · `repo` + `browse` · `pr` · `pipeline`.
3. Integrate, build, test, smoke-test, README.
