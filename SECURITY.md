# Security policy

## Supported versions

Only the latest release of `bb` gets security fixes. Every push to `main` that passes CI is
released, so a fix ships as a new release rather than a backport. Upgrade to the
[latest release](https://github.com/pallewela/bb/releases/latest) (`brew upgrade pallewela/tap/bb`
or `cargo install bitbucket-bb-cli`) before reporting.

| Version        | Supported |
| -------------- | --------- |
| latest release | yes       |
| anything older | no        |

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it privately through a GitHub
security advisory:

**<https://github.com/pallewela/bb/security/advisories/new>**

Include the `bb --version` output, your OS, what you did, and what happened (a proof of concept
helps). Leave real tokens out of the report: revoke any token you have exposed and use a dummy one
in examples. The maintainer aims to reply within 7 days. Once a fix is released, the advisory is
published with credit to you unless you ask otherwise.

## How `bb` handles your token

`bb` authenticates to Bitbucket Cloud with your Atlassian account email and an
[API token](https://id.atlassian.com/manage-profile/security/api-tokens), sent as HTTP Basic auth
over HTTPS. Credentials are looked up in this order (see `src/config.rs`):

1. **Environment variables.** If both `BITBUCKET_EMAIL` and `BITBUCKET_TOKEN` are set and non-empty,
   they are used and nothing on disk is read.
2. **Saved login** from `bb auth login`:
   - The email is stored in `hosts.json`, at `$XDG_CONFIG_HOME/bb/hosts.json` if `XDG_CONFIG_HOME`
     is set, else `%APPDATA%\bb\hosts.json` on Windows, else `~/.config/bb/hosts.json`.
   - The token is stored in the OS keychain (macOS Keychain, Windows Credential Manager, or the
     Secret Service on Linux) under the service name `bb:bitbucket.org`, with the email as the
     account.
   - **Plaintext fallback:** with `bb auth login --insecure-storage`, or when the keychain is not
     available (no Secret Service, a locked or unresponsive keychain; `bb` gives up after 3 s on
     Linux and 60 s elsewhere and prints a warning), the token is written to `hosts.json` itself
     in plaintext. On Unix the file is always created or reset to mode `0600`. On Windows it relies
     on the default permissions of your `%APPDATA%` directory.

`bb auth status` shows which of these the current credentials come from. `bb auth logout`
deletes the keychain entry and `hosts.json`; it does not unset the environment variables.

Things to keep in mind:

- `bb auth token` and `bb auth status --show-token` print the token to stdout. Don't run them where
  output is logged or shared.
- `bb auth login` without `--with-token` reads the token from the terminal with echo on. In shared
  sessions or screen recordings, pipe it in instead: `bb auth login --email you@example.com
  --with-token < token.txt`.
- `bb api` sends your credentials to any full `https://` URL you give it, not only
  `api.bitbucket.org`. Only pass it Bitbucket URLs. Redirects keep the `Authorization` header only
  when they stay on the same host, so presigned log URLs (for example S3) never receive it.
- Use a scoped API token with only the scopes you need, and revoke it when you stop using `bb`.

Reports about any of the above (a token reaching an unexpected place, file permissions,
keychain handling) are in scope for this policy.
