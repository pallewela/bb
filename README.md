# bb

A `gh`-style CLI for Bitbucket Cloud. Scope and design: [PRD.md](PRD.md).

## Install

    brew install pallewela/tap/bb      # macOS arm64
    brew update && brew upgrade pallewela/tap/bb   # upgrade later
    cargo install --path .             # from source

Always use the full name `pallewela/tap/bb` with brew: plain `bb` also matches an unrelated cask.
Prebuilt binaries are on the [releases page](https://github.com/pallewela/bb/releases).

## Auth

Create a scoped Atlassian API token (Bitbucket product; repository + pull request + pipeline scopes,
plus `read:workspace:bitbucket` for `bb org list` and `read:user` for `bb pr comment --edit-last/--delete-last`)
at https://id.atlassian.com/manage-profile/security/api-tokens, then either:

    bb auth login                      # prompts, verifies, saves the token to the system keychain
    bb auth login --insecure-storage   # ...or in plaintext in ~/.config/bb/hosts.json (0600)
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

`[PR]` is an id, a PR URL, or a branch name; omitted, it's the current branch's PR.
Exit codes follow `gh`: 1 = error, 4 = not logged in, 8 = checks still pending (`pr checks`).
Differences from `gh` (Bitbucket extras, and what can't be matched) are listed per command in the [PRD](PRD.md).

## Releases

Every push to `main` that passes CI is released automatically (`.github/workflows/release.yml`):
the patch version is bumped by default; put `[bump:minor]` or `[bump:major]` in the head commit
message to bump more, or `[skip release]` to skip. The Homebrew formula in
[pallewela/homebrew-tap](https://github.com/pallewela/homebrew-tap) is updated afterwards
(needs the `TAP_REPO_TOKEN` secret).
