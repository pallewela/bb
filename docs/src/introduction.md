# bb

`bb` is a `gh`-style CLI for Bitbucket Cloud: one binary for pull requests, repositories,
Pipelines and raw API calls, usable by people and by scripts and AI agents alike.

If you know `gh`, you know `bb`. Commands, flags and flag values are `gh`'s, with `gh`'s meaning.
Bitbucket's own names work too, as extras: `pr decline` = `pr close`, `pipeline` = `run`,
`workspace` = `org`, `pipeline stop` = `run cancel`, `--state declined`.

```sh
bb pr list                      # open PRs in the current repository
bb pr view 42 --json | jq .state
bb pr checks --watch            # CI status of the current branch's PR
bb run view --log-failed        # logs of the failed steps of the latest pipeline
bb api /user
```

Where to go next:

- [Install](install.md) `bb` with Homebrew, Cargo or a prebuilt binary.
- [Authenticate](auth.md) with an Atlassian API token.
- Read the [conventions](usage.md) every command follows, then the
  [command reference](reference/index.md).
- Scripting? See [JSON output and exit codes](json-and-exit-codes.md).
- Using an AI coding agent? See the [agent skill](agent-skill.md).

## Get it, give feedback, contribute

- **Get `bb`:** [install](install.md) a release, or get the source from the
  [GitHub repository](https://github.com/pallewela/bb).
- **Report a bug or request an enhancement:** open an
  [issue](https://github.com/pallewela/bb/issues/new). Security problems go through a
  [private advisory](https://github.com/pallewela/bb/security/advisories/new) instead.
- **Contribute:** pull requests are welcome. See
  [Feedback and contributing](contributing.md) for how to build, test and submit a change.

## Scope

`bb` targets Bitbucket **Cloud** (`api.bitbucket.org/2.0`). Not in scope: Bitbucket Server /
Data Center, Bitbucket issues, multiple accounts, OAuth browser login, shell completions,
user-defined aliases and extensions, and `gh`'s `--json FIELDS` / `--jq` / `--template`
(`bb --json` prints the raw API JSON; pipe it to `jq`).

The full design, including every deliberate difference from `gh`, is in
[PRD.md](https://github.com/pallewela/bb/blob/main/PRD.md).
