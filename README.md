# bb

[![CI](https://github.com/pallewela/bb/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/pallewela/bb/actions/workflows/ci.yml)
[![CodeQL](https://github.com/pallewela/bb/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/pallewela/bb/actions/workflows/codeql.yml)
[![codecov](https://codecov.io/gh/pallewela/bb/branch/main/graph/badge.svg)](https://codecov.io/gh/pallewela/bb)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/pallewela/bb/badge)](https://scorecard.dev/viewer/?uri=github.com/pallewela/bb)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/17063/badge)](https://www.bestpractices.dev/projects/17063)
[![Release](https://img.shields.io/github/v/release/pallewela/bb)](https://github.com/pallewela/bb/releases/latest)
[![Last release](https://img.shields.io/github/release-date/pallewela/bb)](https://github.com/pallewela/bb/releases)
[![License](https://img.shields.io/github/license/pallewela/bb)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey)](#requirements)
[![Homebrew tap](https://img.shields.io/badge/brew-pallewela%2Ftap%2Fbb-orange?logo=homebrew)](https://github.com/pallewela/homebrew-tap)
[![crates.io](https://img.shields.io/crates/v/bitbucket-bb-cli.svg?logo=rust)](https://crates.io/crates/bitbucket-bb-cli)

A `gh`-style CLI for Bitbucket Cloud. Scope and design: [PRD.md](PRD.md).

Documentation: https://pallewela.github.io/bb/

## Requirements

- **OS / architecture** (prebuilt binaries): macOS arm64 and x86_64; Linux x86_64 and arm64
  (static musl, any distro); Windows x86_64.
- **git** on your `PATH`: `bb` uses it to find the repo, branch and commits of the clone you run it
  in, and for `pr checkout` (not needed with `-R WORKSPACE/REPO` for most commands).
- **Network access** to `api.bitbucket.org` (HTTPS).

## Install

    brew install pallewela/tap/bb      # macOS and Linux (arm64 and x86_64)
    brew update && brew upgrade pallewela/tap/bb   # upgrade later
    sudo apt install ./bitbucket-bb-cli_X.Y.Z-1_amd64.deb   # Debian/Ubuntu (.deb from a release)
    sudo dnf install ./bitbucket-bb-cli-X.Y.Z-1.x86_64.rpm  # Fedora/RHEL (.rpm from a release)
    yay -S bitbucket-bb-cli-bin        # Arch Linux, from the AUR
    cargo install bitbucket-bb-cli     # from crates.io (builds from source; installs `bb`)
    cargo install --path .             # from a clone

Always use the full name `pallewela/tap/bb` with brew: plain `bb` also matches an unrelated cask.
On crates.io the crate is `bitbucket-bb-cli` (`bb` and `bb-cli` are other projects); building it
needs Rust and a C compiler.

Prebuilt binaries for macOS (Apple silicon, Intel), Linux (x86_64, arm64; static, any distro) and
Windows (x86_64, `.zip`), and `.deb`/`.rpm` packages for x86_64 and arm64, are on the
[releases page](https://github.com/pallewela/bb/releases). The Linux package is named
`bitbucket-bb-cli`; the command is `bb`. See [Install](https://pallewela.github.io/bb/install.html)
for all options.
On Windows, unzip `bb.exe` into a folder on your `PATH`.

## Auth

Create a scoped Atlassian API token (Bitbucket product; repository + pull request + pipeline scopes,
plus `read:workspace:bitbucket` for `bb org list` and `read:user` for `bb pr comment --edit-last/--delete-last`)
at https://id.atlassian.com/manage-profile/security/api-tokens, then either:

    bb auth login                      # prompts, verifies, saves the token to the system keychain
    bb auth login --insecure-storage   # ...or in plaintext in ~/.config/bb/hosts.json (0600;
                                       #    %APPDATA%\bb\hosts.json on Windows)
    export BITBUCKET_EMAIL=... BITBUCKET_TOKEN=...   # or use env vars (take precedence)

## Usage

If you know `gh`, you know `bb`: commands, flags and flag values are `gh`'s, with `gh`'s meaning
(see [PRD §2.1](PRD.md)). Bitbucket's own names work too, as extras: `pr decline` = `pr close`,
`pipeline` = `run`, `workspace` = `org`, `pipeline stop` = `run cancel`, `--state declined`.

Run inside a clone of a Bitbucket repo, or pass `-R WORKSPACE/REPO`. Add `--json` for raw API JSON.
List commands show 30 items by default (`run list`: 20) and say so when there are more; use `-L N`
to choose how many, or `-L 0` for all. `ls` works for every `list`.

    bb org list [--admin]                      bb repo list [WORKSPACE] [--visibility private] [-l go]
    bb repo view [-w] [-b BRANCH]              bb repo clone / create [-c] / delete --yes

    bb pr list [-s open|closed|merged|all] [-A NICK] [-B BASE] [-H HEAD] [-S TEXT] [-d] [-w]
    bb pr view [PR] [-c] [-w]                  bb pr checkout PR [-b LOCAL] [-f]   (alias: co)
    bb pr create -f | -t TITLE [-b BODY] [-d] [-r REVIEWER] [--dry-run]       (alias: new)
    bb pr diff [PR] [--name-only] [-e '*.lock']
    bb pr merge [PR] -m|-s|-r [-t SUBJECT] [-b BODY] [-d]
    bb pr review [PR] -a | -r -b TEXT | -c -b TEXT
    bb pr close [PR] [-c COMMENT] [-d]         bb pr checks [PR] [--watch] [--fail-fast]
    bb pr comment [PR] -b TEXT [--edit-last]   (inline: --file src/x.rs --line 10)
    bb pr approve / unapprove / request-changes [--undo] / comments      (Bitbucket extras)

    bb run list [-b BRANCH] [-s failure] [-u @me] [-e pull_request]
    bb run view [ID] [--log | --log-failed | -j STEP] [--exit-status]
    bb run watch [ID] [--exit-status]          bb run cancel ID
    bb workflow run [CUSTOM_PIPELINE] [-r BRANCH] [-f KEY=VALUE]

    bb api /user [-i]                          bb api PATH -X POST -F count=3 -f name=x [-H 'K: V']
    bb browse [PATH[:LINE] | PR | SHA] [-n] [-c] [-a] [-s] [--blame]
    bb skill install [--dir DIR]

`[PR]` is an id, a PR URL, or a branch name; omitted, it's the current branch's PR.
Exit codes follow `gh`: 1 = error, 4 = not logged in, 8 = checks still pending (`pr checks`).
Differences from `gh` (Bitbucket extras, and what can't be matched) are listed per command in the [PRD](PRD.md).

## AI coding agents

[skills/bitbucket-cli/SKILL.md](skills/bitbucket-cli/SKILL.md) is an agent skill that teaches
coding agents to use `bb` safely: `--json` + `jq` for reads, exit codes 1/4/8, explicit PR ids for
every write, and tested shell recipes for finding a branch's PR, reading checks, triaging failed
Pipelines, commenting, and creating or merging PRs. It is built into `bb`; to install or update it
for Claude Code:

    bb skill install

## Testing

    cargo test                          # unit tests, plus the skill's recipes against a mock bb
    scripts/e2e.sh                      # end-to-end test of every command against Bitbucket
    scripts/skill-e2e.sh                # the skill's recipes against Bitbucket (no Pipelines minutes)
    scripts/skill-eval.sh               # Claude Code + the skill against a mock bb (costs API usage)

`scripts/e2e.sh` creates a throwaway private repository in the `dupelk` workspace (set
`WORKSPACE` at the top of the script), exercises every command on it with asserts, and deletes it
afterwards (`KEEP=1` keeps it). It needs SSH access to bitbucket.org, `jq`, and about 10
Pipelines build minutes (`SKIP_PIPELINES=1` skips those tests). `BB=target/debug/bb` tests a
development build.

`tests/skill/run.sh` (run by `cargo test` through `tests/skill_recipes.rs`; needs `jq`) extracts the
`# recipe:` blocks from SKILL.md, runs them against `tests/skill/mock-bb` with the fixture in
`tests/skill/fixtures/`, and asserts that they pick the right PR and make only the expected writes.
It then replays each command they ran against the real `bb` (no credentials) to catch flags that
no longer exist. `scripts/skill-e2e.sh` runs the same recipes on a throwaway repository, as
`scripts/e2e.sh` does.

`scripts/skill-eval.sh` tests the agent rather than the recipes. It gives `claude -p` requests
like "merge the PR for my branch" in a scratch checkout where `bb` is the mock, then checks the
mock's log of `bb` calls: the right PR was used, the expected writes (often none) were made, and
the answer says what it should. Agent runs vary, so each case runs `RUNS` times (default 3) and
the score is a pass rate. `NO_SKILL=1` runs the same cases without the skill, as a baseline;
`--list` lists the cases. A run costs roughly $0.15–0.40.

## Releases

Every push to `main` that passes CI is released automatically (`.github/workflows/release.yml`),
with an archive for each target in [PRD §8](PRD.md):
the patch version is bumped by default; put `[bump:minor]` or `[bump:major]` in the head commit's
subject (the PR title, for a squash merge) to bump more, or `[skip release]` to skip. The
Homebrew formula in [pallewela/homebrew-tap](https://github.com/pallewela/homebrew-tap) is updated afterwards
(needs the `TAP_REPO_TOKEN` secret), the crate is published to crates.io when the repository
variable `CRATES_IO_PUBLISH` is `true`, and the AUR package `bitbucket-bb-cli-bin` is pushed when
the `AUR_SSH_PRIVATE_KEY` secret is set. Each release also carries `.deb` and `.rpm` packages
(`packaging/`).
