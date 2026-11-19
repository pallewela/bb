# bb

A `gh`-style CLI for Bitbucket Cloud. Scope and design: [PRD.md](PRD.md).

## Install

    brew install pallewela/tap/bb      # macOS arm64
    cargo install --path .             # from source

Prebuilt binaries are on the [releases page](https://github.com/pallewela/bb/releases).

## Auth

Create a scoped Atlassian API token (Bitbucket product; repository + pull request + pipeline scopes)
at https://id.atlassian.com/manage-profile/security/api-tokens, then either:

    bb auth login                      # prompts, verifies, saves the token to the system keychain
    bb auth login --insecure-storage   # ...or in plaintext in ~/.config/bb/hosts.json (0600)
    export BITBUCKET_EMAIL=... BITBUCKET_TOKEN=...   # or use env vars (take precedence)

## Usage

Run inside a clone of a Bitbucket repo, or pass `-R WORKSPACE/REPO`. Add `--json` for raw API JSON.

    bb repo list [WORKSPACE]         bb repo view / clone / create / delete
    bb pr list                       bb pr view [PR] [--comments] [--web]
    bb pr create --fill              bb pr checkout 12
    bb pr diff                       bb pr merge --squash
    bb pr approve / unapprove / request-changes / decline
    bb pr comment -b "text" [--file src/x.rs --line 10]
    bb pr comments                   bb pr checks
    bb pipeline list                 bb pipeline view 42
    bb pipeline run [--pipeline custom-name --var K=V]
    bb pipeline logs 42 --failed     bb pipeline stop 42
    bb api /user                     bb api PATH -X POST -f key=value [--paginate]
    bb browse [PATH] [--no-browser]

`[PR]` is an id, a PR URL, or omitted for the current branch's open PR. `bb run` is an alias for `bb pipeline`.

## Releases

Every push to `main` that passes CI is released automatically (`.github/workflows/release.yml`):
the patch version is bumped by default; put `[bump:minor]` or `[bump:major]` in the head commit
message to bump more, or `[skip release]` to skip. The Homebrew formula in
[pallewela/homebrew-tap](https://github.com/pallewela/homebrew-tap) is updated afterwards
(needs the `TAP_REPO_TOKEN` secret).
