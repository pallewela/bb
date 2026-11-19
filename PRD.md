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
| Auth | HTTP Basic `email:api_token` (Atlassian API token; Bearer does **not** work). Sources, in order: `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN` env vars, then the saved login. Storage follows `gh`: the token lives in the OS keychain (macOS Keychain / Windows Credential Manager / Secret Service; service `bb:bitbucket.org`, account = email) and `~/.config/bb/hosts.json` (mode 0600) holds only the email. With `--insecure-storage`, or when no keychain is available, the token is written to `hosts.json` in plaintext instead. Keychain calls time out (3s on Linux, where a stuck Secret Service daemon can hang; 60s elsewhere to allow for an OS access prompt) and then fail or fall back rather than hang. |
| Errors | Message on stderr, exit code 1. API errors show `HTTP <code>: <message>`. |
| Lists | `-L/--limit N` (default 30; `workspace list` 100; `0` = all); pagination followed automatically up to the limit. When `-L` is not given and items are left out, a note on stderr says how many were shown of the total (API `size`) and how to see more; nothing is added to stdout or `--json`. |
| `--web` | Where offered, opens the resource in the browser instead of printing. |

**Differences from `gh` (all commands)**
- *Gap:* `--json` prints raw API JSON; `gh`'s `--json FIELDS`, `--jq` and `--template` are a
  v1 non-goal (§3).
- *Gap:* `-R` takes `WORKSPACE/REPO` or a bitbucket.org URL; there is no `HOST/` prefix (Cloud only).
- *Gap:* every failure exits 1. `gh` uses 4 when auth is required, 2 when cancelled and 8 for
  pending checks; match these (#6).

## 5. Commands

### 5.1 `bb auth`
| Command | Behaviour |
|---|---|
| `auth login [--insecure-storage]` | Prompt for email + API token (or `--email`, `--with-token` reading token from stdin). Verify against the API, then save (keychain by default; replaces any previous login). Reports where the token was stored. |
| `auth status` | Show where creds come from (env / keychain / plaintext file), the account if readable, and whether the token works. |
| `auth logout` | Delete the saved login: keychain entry and `hosts.json`. |
| `auth token` | Print the token (for scripting). |

Token verification: `GET /user`; if that 403s (scoped token lacking `read:user`),
treat the token as valid — a bad token gets 401, so 403 proves it authenticated.

**Differences from `gh`**
- *Extra:* `auth login --email`. Bitbucket API tokens authenticate as `email:token`, so the email is required.
- *Gap:* no `auth login -w/--web` (OAuth is a v1 non-goal), `-p/--git-protocol` (git is always SSH,
  see `repo clone`) or `-s/--scopes` (scopes are chosen when the token is created).
- *Gap:* no `-u/--user` on `logout`/`token`, and no `auth status -a` (single account, §3).
- *Gap:* `auth status -t/--show-token` is missing (#6).

### 5.2 `bb repo`
| Command | API |
|---|---|
| `repo list [WORKSPACE]` | `GET /repositories/{ws}`. No workspace: the current repo's workspace (Bitbucket removed cross-workspace listing). Columns: full name, visibility, language, updated. |
| `repo view [REPO] [--web]` | `GET /repositories/{ws}/{slug}` — name, description, main branch, visibility, clone URLs, web URL. |
| `repo clone WORKSPACE/REPO [DIR] [-- git args]` | `git clone git@bitbucket.org:ws/repo.git` (SSH: API tokens don't work for git-over-HTTPS). |
| `repo create WORKSPACE/REPO [--private/--public] [--description] [--project KEY] [--clone]` | `POST /repositories/{ws}/{slug}` with `scm: git`. Private by default. |
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
- *Gap:* missing `gh` flags: `repo list --visibility/-l/--fork/--source/--archived`, `repo view -w/-b`,
  `repo create -c` (#6).

### 5.2a `bb workspace` (alias `bb ws`)
| Command | API |
|---|---|
| `workspace list [--admin] [-L N]` | `GET /user/workspaces?sort=slug` (`administrator=true` with `--admin`). Columns: slug, role (admin/member). Needs the `read:workspace:bitbucket` scope; a 403 explains that. |

**Differences from `gh`**
- The `gh` analog is `gh org list`. *Gap:* `bb org list` is not an alias yet, and the default `-L` is
  100 instead of `gh`'s 30 (#6).
- *Extra:* `--admin` (Bitbucket's per-workspace admin flag); `workspace`/`ws` as Bitbucket names.

### 5.3 `bb pr`
`[PR]` argument = numeric id, a PR URL, or omitted → the open PR whose source is the
current git branch.

| Command | API (under `/repositories/{ws}/{slug}/pullrequests`) |
|---|---|
| `pr list [--state open\|merged\|declined\|all] [--author NICK] [--base BRANCH] [--head BRANCH] [-L N]` | `GET ?state=…&q=…` — columns: id, title, source branch, author, state. |
| `pr view [PR] [--comments] [--web]` | `GET /{id}` — title, state, author, branches, reviewers + approval status, description, URL. |
| `pr create [-t TITLE] [-b BODY] [-B BASE] [-H HEAD] [--draft] [--reviewer UUID\|ACCOUNT_ID…] [--close-source-branch] [--fill] [--web]` | `POST` — head defaults to current branch, base to repo main branch; `--fill` takes title/body from the last commit. Prints the PR URL. |
| `pr checkout PR` | `git fetch origin <source-branch>` + `git checkout <source-branch>`. |
| `pr diff [PR]` | `GET /{id}/diff` (text). |
| `pr merge [PR] [--merge\|--squash\|--fast-forward] [-m MESSAGE] [--close-source-branch]` | `POST /{id}/merge` with `merge_strategy`. |
| `pr decline [PR]` (alias `close`) | `POST /{id}/decline`. |
| `pr approve [PR]` / `pr unapprove [PR]` | `POST` / `DELETE /{id}/approve`. |
| `pr request-changes [PR] [--undo]` | `POST` / `DELETE /{id}/request-changes`. |
| `pr comment [PR] -b BODY [--file PATH --line N]` | `POST /{id}/comments` with `content.raw`, optional `inline: {path, to}`. |
| `pr comments [PR]` | `GET /{id}/comments` — author, location (if inline), body. |
| `pr checks [PR]` | `GET /{id}/statuses` — state, name, URL. Exit code 1 if any check FAILED. |

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

- *Extra:* `pr decline` is the Bitbucket name for `gh pr close`; both names must work.
- *Extra:* `pr approve`, `pr unapprove` and `pr request-changes [--undo]` are separate Bitbucket
  actions. `gh` uses `pr review -a/-r/-c`, which must also work; `unapprove` and `--undo` have no `gh` equivalent.
- *Extra:* `pr comments` (`gh` only has `pr view --comments`, which `bb` also supports).
- *Extra:* `pr comment --file/--line` for inline comments.
- *Extra:* `pr create --close-source-branch`, `pr merge --fast-forward` and `--close-source-branch`
  (Bitbucket options). `gh pr merge -d/--delete-branch` should map to `close_source_branch`.
- *Gap:* `pr create --reviewer` needs a UUID or account_id; `gh` takes a handle (#6: resolve nicknames).
- *Gap:* no labels, assignees or milestones (Bitbucket has none), so `-l`, `-a` and `-m` on
  `pr create`/`pr list` don't apply.
- *Gap (bug):* `pr merge -m` means `--message`, but in `gh` `-m` is `--merge`. Fix by switching to `gh`'s
  `-m/--merge`, `-s/--squash`, `-t/--subject` and `-b/--body` (#6, breaking).
- *Gap:* `[PR]` doesn't accept a branch name yet, `--state closed` isn't accepted yet, and the
  remaining missing flags are listed in #6.

### 5.4 `bb pipeline` (the `gh run` analog; alias `bb run`)
| Command | API (under `/repositories/{ws}/{slug}/pipelines`) |
|---|---|
| `pipeline list [--branch B] [-L N]` | `GET /?sort=-created_on` — build number, state/result, ref, trigger, creator, created. |
| `pipeline view BUILD_NUMBER\|UUID [--web]` | `GET /{id}` + `GET /{id}/steps/` — summary + per-step status/duration. |
| `pipeline run [--branch B] [--pipeline CUSTOM_NAME] [--var K=V…]` | `POST /` with `pipeline_ref_target` (branch defaults to current). Custom pipelines use a `selector: {type: custom, pattern}`. |
| `pipeline stop BUILD_NUMBER\|UUID` | `POST /{uuid}/stopPipeline`. |
| `pipeline logs BUILD_NUMBER\|UUID [--step NAME\|UUID] [--failed]` | `GET /{uuid}/steps/{step}/log` — default: all steps in order; `--failed`: only failed steps. |

**Differences from `gh`**
- *Extra:* `pipeline` is the Bitbucket name; `run` is the `gh` name, and both work.
- *Extra:* `pipeline logs`. `gh`'s form, `run view --log / --log-failed / -j`, must also work (#6).
- *Extra:* `pipeline stop` is the Bitbucket name for `gh run cancel`; both must work (#6).
- *Extra:* `pipeline run --branch/--pipeline/--var`. Triggering a run is `gh workflow run NAME --ref B -f k=v`
  in `gh`, and that form (custom pipeline = workflow) must also work (#6).
- *Gap:* `run watch`, `run rerun` (if the API supports re-runs), `run list` filters (`-s`, `-u`,
  `-c`, `-w`, `-e`), `-b` for `--branch`, the default `-L 20`, and `run view` without an ID (#6).

### 5.5 `bb api`
`bb api PATH [-X METHOD] [-f key=value…] [--input FILE|-] [--paginate]`

Authenticated raw request. `PATH` is relative to `/2.0` or a full URL. `-f` builds a
JSON object body (and implies POST if `-X` is absent). `--paginate` follows `next`
links and prints the concatenated `values` array. Output is the response body, pretty-printed if JSON.

**Differences from `gh`**
- `-f key=value` matches `gh api -f/--raw-field` (string values).
- *Gap:* `--paginate` prints one combined `values` array, which is closer to `gh api --paginate --slurp`,
  because Bitbucket pages are objects that wrap `values`. Keep and document this, or add `--slurp` (#6).
- *Gap:* missing `-F/--field` (typed values, `@file`), `-H/--header`, `-i/--include`, `--silent` and
  `--verbose` (#6). `--jq` and `--cache` are out of scope (§3).

### 5.6 `bb browse`
`bb browse [PATH] [--branch B] [--no-browser]` — open the repo (or a file/dir in it)
on bitbucket.org; `--no-browser` prints the URL.

**Differences from `gh`**
- *Gap:* missing `-n` for `--no-browser`, a PR number as the argument, `-c/--commit`, `--blame`,
  `-s/--settings`, `-a/--actions` (→ Pipelines) and `-w/--wiki` (#6). `-p/--projects` and
  `-r/--releases` have no Bitbucket equivalent.

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
src/workspace.rs bb workspace
```

Dependencies: `clap` (derive), `ureq` (blocking HTTP, no async runtime), `serde_json`,
`anyhow`, `base64`, `keyring` (OS credential store). API responses are handled as `serde_json::Value` — no typed model
layer; Bitbucket payloads are large and we read a handful of fields.

## 7. Quality bar

- `cargo build` warning-free; `cargo test` green.
- Unit tests for pure logic: remote-URL parsing, PR-argument parsing, query building, `-f` body building.
- No network in tests. Live verification is a manual read-only smoke test.

## 8. Milestones

1. Core (client, config, context, output, CLI skeleton) — the contract the rest builds on.
2. In parallel: `auth` + `api` · `repo` + `browse` · `pr` · `pipeline`.
3. Integrate, build, test, smoke-test, README.
