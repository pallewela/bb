# Contributing to bb

Thanks for helping. Bug reports and feature requests go in
[issues](https://github.com/pallewela/bb/issues) (see [SUPPORT.md](SUPPORT.md)); security problems
go through a private advisory instead (see [SECURITY.md](SECURITY.md)). For anything bigger than a
small fix, open an issue first so we can agree on the behavior. `bb` follows `gh`, and
[PRD.md](PRD.md) lists where and why it differs.

## Build

You need a stable Rust toolchain recent enough for edition 2024 and let-chains (Rust 1.88 or
newer). There are no other build dependencies.

    cargo build                  # debug build: target/debug/bb
    cargo build --release        # release build
    cargo install --path .       # install `bb` from your checkout

CI builds with `--locked`, so commit `Cargo.lock` when you change dependencies.

## Tests

    cargo test                   # unit tests + the skill's recipes against a mock bb
    tests/skill/run.sh           # just the skill recipe tests (cargo test runs this too)
    scripts/e2e.sh               # every command against real Bitbucket Cloud

**`cargo test`** needs no network or credentials. It includes `tests/skill_recipes.rs`, which runs
`tests/skill/run.sh` with `BB_REAL` set to the freshly built `bb`. That test needs `bash` and
`jq`: it is skipped (with a message) when `jq` is missing, and ignored on Windows. One keychain
test is `#[ignore]`d because it writes a dummy entry to your real OS keychain; run it with
`cargo test -- --ignored`.

**`tests/skill/run.sh`** needs `bash` and `jq`. It extracts the `# recipe:` blocks from
[skills/bitbucket-cli/SKILL.md](skills/bitbucket-cli/SKILL.md), runs them against
`tests/skill/mock-bb` with the data in `tests/skill/fixtures/repo.json`, and checks that each one
picks the right PR or pipeline and makes only the expected writes. With `BB_REAL=target/debug/bb`
it also replays every command the recipes ran against the real CLI to catch flags that no longer
exist. `VERBOSE=1` prints each call.

**`scripts/e2e.sh`** is optional and talks to real Bitbucket Cloud. It creates a throwaway private
repository, exercises every command on it, and deletes it on exit (`KEEP=1` keeps it). You need:

- `bb` logged in (`bb auth login`) or `BITBUCKET_EMAIL`/`BITBUCKET_TOKEN` set, with a token that has
  the repository (read, write, admin, delete), pull request (read, write), pipeline (read, write),
  `read:workspace:bitbucket` and `read:user` scopes;
- `git` with SSH access to bitbucket.org (`ssh -T git@ssh.bitbucket.org` works), `jq` and `timeout`;
- a workspace you can create repositories in. The script uses `WORKSPACE="dupelk"`; change that line
  at the top of the script to your own workspace;
- Pipelines enabled in that workspace. The Pipelines section uses about 10 build minutes;
  `SKIP_PIPELINES=1` skips it.

Run it against your build with `BB=target/debug/bb scripts/e2e.sh`. Your saved login is not
touched: the login/logout tests use a temporary config directory.

Two more scripts exist for skill work: `scripts/skill-e2e.sh` runs the skill's recipes against
real Bitbucket (same requirements as `e2e.sh`, honors a `WORKSPACE` env var, no Pipelines
minutes), and `scripts/skill-eval.sh` checks that Claude Code uses the skill correctly against the
mock (needs the `claude` CLI and costs API usage).

Before opening a PR, run `cargo test` and `cargo clippy --all-targets -- -D warnings` (CI fails on
any clippy warning). If you changed how a command talks to Bitbucket, also run
`scripts/e2e.sh` if you can, and say in the PR whether you did.

## The agent skill tracks the CLI

[skills/bitbucket-cli/SKILL.md](skills/bitbucket-cli/SKILL.md) teaches coding agents to use `bb`.
**A PR that changes the CLI must update the skill in the same PR**: new, renamed or removed
commands and flags, `--json` fields, output that recipes parse, exit codes, and safety behavior
(confirmations, which commands write). If a recipe changes, update its asserts in
`tests/skill/run.sh` and, if needed, the fixture. `cargo test` fails when a recipe uses a flag the
real `bb` no longer accepts, but it can't notice that the skill is missing a new feature or
describes old behavior. Reviewers will ask about that.

## Commit messages and releases

PRs are usually squash-merged, so the PR title becomes the commit subject on `main`. Every push
to `main` that passes CI is released by [.github/workflows/release.yml](.github/workflows/release.yml).
Write the title as a [conventional commit](https://www.conventionalcommits.org/):

    feat(pr): add --rebase-merge and --squash-fast-forward to pr merge
    fix(windows): store hosts.json in %APPDATA%
    docs: ...    ci(release): ...    test(skill): ...    chore(deps): ...

Use short bullets in the body for what changed and why. Markers in the squash commit's **subject**
pick the next version from the latest `vX.Y.Z` tag; the same text in the body is ignored, so
the body can mention a marker without triggering it. Only `BREAKING CHANGE` counts anywhere in
the message, as conventional commits define it.

| Marker                                          | Release                              |
| ----------------------------------------------- | ------------------------------------ |
| `[bump:major]` in the subject, `BREAKING CHANGE` | major: `1.4.2` -> `2.0.0`            |
| `[bump:minor]` in the subject                   | minor: `1.4.2` -> `1.5.0`            |
| none                                            | patch: `1.4.2` -> `1.4.3` (default)  |
| `[skip release]` in the subject                 | no release                           |

So put markers in the PR title. With GitHub's squash default, a PR with a single commit uses that
commit's subject instead of the PR title, so put the marker in both.

Use `[skip release]` for changes that don't affect the binary (docs, CI, tests, scripts), so they
don't produce an identical release. Commits with `[skip release]` in the subject are also left out
of release notes. Don't edit `version` in `Cargo.toml`: releases stamp it from the tag.
