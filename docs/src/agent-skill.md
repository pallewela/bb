# Agent skill

[`skills/bitbucket-cli/SKILL.md`](https://github.com/pallewela/bb/blob/main/skills/bitbucket-cli/SKILL.md)
is an agent skill that teaches AI coding agents to use `bb` safely. It adds no commands; it
documents how an agent should use the ones in this book.

## Install for Claude Code

The skill is built into `bb`. [`bb skill install`](reference/skill.md) writes it to
`~/.claude/skills/bitbucket-cli/SKILL.md` (`$CLAUDE_CONFIG_DIR/skills` if set); run it again after
upgrading `bb` to update the skill. `--dir` installs into another skills directory.

When working on the skill itself, link it from a clone instead, so edits take effect at once:

```sh
ln -s "$PWD/skills/bitbucket-cli" ~/.claude/skills/bitbucket-cli
```

The skill triggers when the git remote is bitbucket.org, or the user mentions Bitbucket, a
Bitbucket PR, or Pipelines. It is not for GitHub (`gh`) or Bitbucket Server/Data Center.

## What it teaches

- **Read with `--json`, parse with `jq`.** Human tables can change; the JSON is the API's.
- **Reads are free; writes need the user's go-ahead.** Create, merge, close, approve, review,
  comment, pipeline run/cancel, repo create/edit/delete, project create/edit/delete and non-GET
  `bb api` calls only when the user asked for that action on that object.
- **Name the PR by id for every write.** A branch (or no argument) resolves to the open PR for
  that branch, else the most recently updated PR of any state, possibly an old merged one.
- **Match branches exactly**, and confirm a list result with `bb pr view ID --json` before acting
  on it: `pr list` can lag for a while after a write.
- **Never prompt or open a browser:** no `bb auth login`, `-w/--web` or `bb browse` from an agent;
  pass bodies with `-b` or `-F`.
- **Use `-L 0`** when completeness matters.
- **Projects by key:** look up a project's key from its name (one exact match, else stop) before
  `repo list -p` or `repo edit -p`.
- **Keep secrets out of output:** never `bb auth token` or `bb auth status -t`.
- **Check exit codes** 1, 2, 4 and 8 (see [exit codes](json-and-exit-codes.md#exit-codes)).

## Recipes

The skill's workflows are shell functions, each in a `bash` block starting with `# recipe: NAME`:

| Recipe | What it does |
|---|---|
| `bb_pr_is_open ID BRANCH [BASE]` | Exit 0 if `pr view` says the PR is open, from BRANCH (into BASE) |
| `bb_pr_for_branch BRANCH [BASE]` | Print the id of the one open PR from BRANCH; fail rather than guess |
| `bb_checks_state ID [--watch]` | Print `passing`, `failing`, `pending` or `none` |
| `bb_pr_summary ID` | Read-only summary: state, branches, approvals, change requests, checks |
| `bb_failed_logs BRANCH` | Failed steps' logs of the branch's latest pipeline |
| `bb_ensure_pr HEAD BASE TITLE [BODY]` | Reuse an open PR or create one (after a dry run); print its URL |
| `bb_upsert_comment ID MARKER BODY` | Post or update the agent's own marked status comment |
| `bb_safe_merge ID STRATEGY [--allow-no-checks]` | Merge only if open, not a draft, no change requests, checks passed |

## How it is tested

- `tests/skill/run.sh` (run by `cargo test`; needs `jq`) extracts the recipe blocks from SKILL.md,
  runs them against a mock `bb` (`tests/skill/mock-bb`) with a fixture, asserts they pick the
  right PR and make only the expected writes, then replays each command line against the real `bb`
  to catch flags that no longer exist.
- `scripts/skill-e2e.sh` runs the same recipes against a throwaway repository on Bitbucket.
- `scripts/skill-eval.sh` tests the agent: it gives `claude -p` realistic requests in a scratch
  checkout where `bb` is the mock, and scores whether the right PR was used and only the expected
  writes were made. It costs API usage and is run by hand.

The skill is part of `bb`'s interface: a change to a command, flag, default, `--json` shape, exit
code or PR resolution rule that the skill mentions updates SKILL.md (and the mock) in the same
change.
