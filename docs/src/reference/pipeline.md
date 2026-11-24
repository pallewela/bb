# bb pipeline (alias `run`)

Work with Bitbucket Pipelines. `bb run` is the `gh run` name; `bb pipeline` is the Bitbucket name.
Both work everywhere on this page.

`[ID]` is a build number or a pipeline UUID. Omitted, it is the latest pipeline for the current git
branch.

## bb pipeline list (alias `ls`)

List recent pipelines, newest first: build number, state/result, ref, trigger, creator, created.

| Flag | Meaning |
|---|---|
| `-b, --branch <BRANCH>` | Only pipelines for this branch (including pull request pipelines from it) |
| `-s, --status <STATUS>` | Only pipelines with this status (see below) |
| `-u, --user <USER>` | Only pipelines started by this user (nickname, display name, account ID, UUID or `@me`) |
| `-c, --commit <SHA>` | Only pipelines for this commit (full or abbreviated SHA) |
| `-w, --workflow <NAME>` | Only runs of this pipeline (custom pipeline name or branch pattern) |
| `-e, --event <EVENT>` | Only pipelines with this trigger: `push`, `pull_request`, `workflow_dispatch` (= `manual`), `schedule`, `parent_step` |
| `-L, --limit <N>` | Maximum number of pipelines to list [default: 20; 0 = all] |

`--status` accepts `gh`'s values and Bitbucket's:

| Value | Bitbucket states |
|---|---|
| `queued`, `pending`, `requested` | PENDING, PARSING |
| `in_progress` | BUILDING, PAUSED, HALTED |
| `waiting`, `action_required` | PAUSED, HALTED |
| `completed` | PASSED, FAILED, ERROR, STOPPED |
| `success` | PASSED |
| `failure` | FAILED |
| `cancelled` | STOPPED |
| `startup_failure` | ERROR |
| Bitbucket extras | `successful`, `failed`, `error`, `stopped`, `running`, `paused`, `halted`, `parsing` |

All filters run server-side. A nickname, display name or account ID for `-u` is looked up among the
creators of the 500 most recent pipelines (the API can't filter by name); a UUID or `@me` is exact.

## bb pipeline view `[ID]`

Show a pipeline and its steps, or its logs.

| Flag | Meaning |
|---|---|
| `-j, --job <STEP>` | Only this step (name, case-insensitive, or UUID) |
| `--log` | Print the full log (of all steps, or of `--job`) |
| `--log-failed` | Print the log of failed steps only |
| `--exit-status` | Exit with non-zero status if the pipeline failed |
| `-v, --verbose` | Steps are always shown; accepted for `gh` compatibility |
| `-w, --web` | Open the pipeline in the browser |

## bb pipeline watch `[ID]`

Watch a pipeline until it finishes, printing each pipeline and step status change. It also stops
(exit 0) when the pipeline is paused or halted waiting for a manual step.

| Flag | Meaning |
|---|---|
| `--exit-status` | Exit with non-zero status if the pipeline fails |
| `-i, --interval <N>` | Refresh interval in seconds [default: 3] |

## bb pipeline run

Trigger a pipeline on a branch. See also [`bb workflow run`](workflow.md), the `gh` form.

| Flag | Meaning |
|---|---|
| `-r, --ref <BRANCH>` (alias `--branch`) | Branch to run on (default: the current branch) |
| `-f, --raw-field <KEY=VALUE>` (alias `--var`) | Pipeline variable (repeatable) |
| `-F, --field <KEY=VALUE>` | Pipeline variable; VALUE `@file` reads a file, `@-` stdin (repeatable) |
| `--pipeline <NAME>` | Custom pipeline name (from `bitbucket-pipelines.yml` `custom:`) |

## bb pipeline cancel `ID` (alias `stop`)

Cancel (stop) a running pipeline.

## bb pipeline logs `[ID]`

Print step logs: all steps in order by default. The same as `run view --log` / `--log-failed` /
`-j`.

| Flag | Meaning |
|---|---|
| `--step <NAME\|UUID>` | Only this step (name, case-insensitive, or UUID) |
| `--failed` | Only failed steps |

## Differences from `gh`

- *Extra:* `pipeline`, `pipeline stop` and `pipeline logs` as Bitbucket names; `run list -s`
  accepts Bitbucket's status names; `-u` accepts a UUID or display name.
- *Extra:* `pipeline run --branch/--pipeline/--var` (and `-r/--ref`, `-f`, `-F`).
- `run view` / `run watch` without an ID use the latest pipeline for the current branch instead of
  prompting. `run view -j` needs the pipeline ID too.
- `run watch` prints status-change lines rather than redrawing the screen.
- *Gap:* no `run rerun`: the Bitbucket Cloud REST API has no endpoint to re-run a pipeline or its
  failed steps. Start a new run with `workflow run` / `pipeline run`.
- *Gap:* no `run list --created`, `-a/--all`; no `run view -a/--attempt`; no
  `run cancel --force`; no `run watch --compact`; no `run delete`, `run download`.
