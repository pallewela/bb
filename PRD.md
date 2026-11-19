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
- **Same behaviour.** Same defaults (e.g. `-L 30`), same streams (data on stdout;
  hints, prompts and errors on stderr), same exit codes, same inference from the
  current git checkout.
- **Map, don't invent.** A Bitbucket concept that has a `gh` analog takes the `gh` name
  (Pipelines → `bb run` as well as `bb pipeline`; the PR checks list → `bb pr checks`).
  A Bitbucket-only concept (e.g. workspaces) gets the shape `gh` uses for similar
  things: `list` / `view` / `create` / `delete`, `-L`, `--json`, `--web`.
- **`gh` words for commands, Bitbucket words for data.** Commands and flags use `gh`'s
  vocabulary; output shows what Bitbucket calls things (workspace, build number).
- **Deviations are deliberate and recorded.** If Bitbucket's API forces a difference,
  or a `gh` behaviour cannot be matched, note it and the reason in the command's
  table in §5. An unrecorded difference from `gh` is a bug.
- **Matching `gh` beats extra features.** A feature that would need a command shape
  `gh` doesn't have needs a strong reason; prefer the `gh` form even if it's less
  convenient.

## 3. Non-goals (v1)

- Bitbucket Server / Data Center (different API).
- Issues (Bitbucket's issue tracker is disabled on most repos; Jira is used instead).
- Multiple accounts / `auth switch`. OAuth browser login flow, shell completions, aliases, extensions.
- `gh`-style `--json field1,field2` + `--jq`. v1 has a plain `--json` (raw API JSON); pipe to `jq`.
- Interactive prompts/editors beyond `auth login`.

## 4. Conventions (apply to every command)

| Convention | Behaviour |
|---|---|
| Repo resolution | `-R/--repo WORKSPACE/REPO` (global flag; also accepts a bitbucket.org URL). Otherwise parsed from the `origin` git remote of the cwd. |
| Output | Human-readable aligned tables/text by default. Global `--json` prints the raw API JSON instead (lists print a JSON array). |
| Auth | HTTP Basic `email:api_token` (Atlassian API token; Bearer does **not** work). Sources, in order: `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN` env vars, then the saved login. Storage follows `gh`: the token lives in the OS keychain (macOS Keychain / Windows Credential Manager / Secret Service; service `bb:bitbucket.org`, account = email) and `~/.config/bb/hosts.json` (mode 0600) holds only the email. With `--insecure-storage`, or when no keychain is available, the token is written to `hosts.json` in plaintext instead. Keychain calls time out (3s on Linux, where a stuck Secret Service daemon can hang; 60s elsewhere to allow for an OS access prompt) and then fail or fall back rather than hang. |
| Errors | Message on stderr, exit code 1. API errors show `HTTP <code>: <message>`. |
| Lists | `-L/--limit N` (default 30; `workspace list` 100; `0` = all); pagination followed automatically up to the limit. When `-L` is not given and items are left out, a note on stderr says how many were shown of the total (API `size`) and how to see more; nothing is added to stdout or `--json`. |
| `--web` | Where offered, opens the resource in the browser instead of printing. |

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

### 5.2 `bb repo`
| Command | API |
|---|---|
| `repo list [WORKSPACE]` | `GET /repositories/{ws}`. No workspace: the current repo's workspace (Bitbucket removed cross-workspace listing). Columns: full name, visibility, language, updated. |
| `repo view [REPO] [--web]` | `GET /repositories/{ws}/{slug}` — name, description, main branch, visibility, clone URLs, web URL. |
| `repo clone WORKSPACE/REPO [DIR] [-- git args]` | `git clone git@bitbucket.org:ws/repo.git` (SSH: API tokens don't work for git-over-HTTPS). |
| `repo create WORKSPACE/REPO [--private/--public] [--description] [--project KEY] [--clone]` | `POST /repositories/{ws}/{slug}` with `scm: git`. Private by default. |
| `repo delete [REPO] --yes` | `DELETE /repositories/{ws}/{slug}`. Refuses without `--yes`. |

### 5.2a `bb workspace` (alias `bb ws`)
| Command | API |
|---|---|
| `workspace list [--admin] [-L N]` | `GET /user/workspaces?sort=slug` (`administrator=true` with `--admin`). Columns: slug, role (admin/member). Needs the `read:workspace:bitbucket` scope; a 403 explains that. |

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

### 5.4 `bb pipeline` (the `gh run` analog; alias `bb run`)
| Command | API (under `/repositories/{ws}/{slug}/pipelines`) |
|---|---|
| `pipeline list [--branch B] [-L N]` | `GET /?sort=-created_on` — build number, state/result, ref, trigger, creator, created. |
| `pipeline view BUILD_NUMBER\|UUID [--web]` | `GET /{id}` + `GET /{id}/steps/` — summary + per-step status/duration. |
| `pipeline run [--branch B] [--pipeline CUSTOM_NAME] [--var K=V…]` | `POST /` with `pipeline_ref_target` (branch defaults to current). Custom pipelines use a `selector: {type: custom, pattern}`. |
| `pipeline stop BUILD_NUMBER\|UUID` | `POST /{uuid}/stopPipeline`. |
| `pipeline logs BUILD_NUMBER\|UUID [--step NAME\|UUID] [--failed]` | `GET /{uuid}/steps/{step}/log` — default: all steps in order; `--failed`: only failed steps. |

### 5.5 `bb api`
`bb api PATH [-X METHOD] [-f key=value…] [--input FILE|-] [--paginate]`

Authenticated raw request. `PATH` is relative to `/2.0` or a full URL. `-f` builds a
JSON object body (and implies POST if `-X` is absent). `--paginate` follows `next`
links and prints the concatenated `values` array. Output is the response body, pretty-printed if JSON.

### 5.6 `bb browse`
`bb browse [PATH] [--branch B] [--no-browser]` — open the repo (or a file/dir in it)
on bitbucket.org; `--no-browser` prints the URL.

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
