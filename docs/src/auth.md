# Authentication

`bb` authenticates with an Atlassian **API token** and your Atlassian account email (HTTP Basic
`email:token`; Bitbucket API tokens do not work as Bearer tokens).

## 1. Create a token

Create a scoped API token for the Bitbucket product at
<https://id.atlassian.com/manage-profile/security/api-tokens>. Give it:

- repository, pull request and pipeline scopes,
- `read:workspace:bitbucket`, for `bb org list`,
- `read:project:bitbucket` and `admin:project:bitbucket`, for `bb project`,
- `read:user`, for `bb pr comment --edit-last` / `--delete-last`.

Scopes are chosen when the token is created; `bb` has no `--scopes` flag.

## 2. Log in, or use environment variables

```sh
bb auth login                      # prompts, verifies, saves the token to the system keychain
bb auth login --insecure-storage   # ...or in plaintext in hosts.json (mode 0600)
export BITBUCKET_EMAIL=... BITBUCKET_TOKEN=...   # or use env vars (take precedence)
```

`bb auth login` prompts for the email (or takes `--email EMAIL`) and the token (or reads it from
stdin with `--with-token`), verifies them against the API, then saves them, replacing any previous
login. It reports where the token was stored. Non-interactive example:

```sh
printf '%s' "$TOKEN" | bb auth login --email me@example.com --with-token
```

## Where credentials come from

In order:

1. `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN` environment variables.
2. The saved login.

As in `gh`, the saved token lives in the OS keychain (macOS Keychain, Windows Credential Manager,
or Secret Service on Linux; service `bb:bitbucket.org`, account = your email), and `hosts.json`
(mode 0600) holds only the email. `hosts.json` is in:

- `$XDG_CONFIG_HOME/bb/` if `XDG_CONFIG_HOME` is set,
- else `%APPDATA%\bb\` on Windows,
- else `~/.config/bb/`.

With `--insecure-storage`, or when no keychain is available, the token is written to `hosts.json`
in plaintext instead. Keychain calls time out (3 s on Linux, 60 s elsewhere to allow for an OS
access prompt) and then fail or fall back rather than hang.

## Checking and removing the login

```sh
bb auth status        # where creds come from, the account, and whether the token works
bb auth status -t     # ...and print the token
bb auth token         # print the token only (for scripting)
bb auth logout        # delete the keychain entry and hosts.json
```

A command that needs credentials and finds none exits with code **4**
(see [exit codes](json-and-exit-codes.md#exit-codes)).

Token verification uses `GET /user`. A scoped token without `read:user` gets 403 there; `bb` treats
that as valid, because a bad token gets 401.

Only one account is supported: there is no `auth switch` and no `-u/--user` flag.
