# bb auth

Manage credentials. See [Authentication](../auth.md) for how to create a token and where it is
stored.

## bb auth login

Verify an Atlassian email + API token and save them (token in the system keychain). Replaces any
previous login and reports where the token was stored.

| Flag | Meaning |
|---|---|
| `--email <EMAIL>` | Atlassian account email (prompted if absent) |
| `--with-token` | Read the API token from stdin |
| `--insecure-storage` | Store the token in plaintext in the config file instead of the system keychain |

## bb auth status

Show where credentials come from (env / keychain / plaintext file), the account if readable, and
whether the token works.

| Flag | Meaning |
|---|---|
| `-t, --show-token` | Also print the API token |

## bb auth logout

Delete the saved login (keychain entry and config file).

## bb auth token

Print the API token (for scripting).

## Differences from `gh`

- *Extra:* `auth login --email`. Bitbucket API tokens authenticate as `email:token`, so the email
  is required.
- *Gap:* no `auth login -w/--web` (no OAuth), `-p/--git-protocol` (git is always SSH, see
  `repo clone`) or `-s/--scopes` (scopes are chosen when the token is created).
- *Gap:* no `-u/--user` on `logout`/`token`, and no `auth status -a` (single account).
