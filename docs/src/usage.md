# Usage and conventions

These apply to every command.

## Choosing the repository

Run `bb` inside a clone of a Bitbucket repository: the repository is read from the `origin` git
remote. Anywhere else, pass the global flag `-R, --repo WORKSPACE/REPO` (a bitbucket.org URL also
works). There is no `HOST/` prefix: `bb` is Cloud only.

```sh
bb pr list -R myteam/api
bb pr view https://bitbucket.org/myteam/api/pull-requests/12
```

## Output

Output is human-readable tables and text by default. The global `--json` flag prints the raw
Bitbucket API JSON instead; lists print one JSON array. See
[JSON output and exit codes](json-and-exit-codes.md).

Data goes to stdout. Hints, prompts, notes and errors go to stderr.

## Lists

- Lists show 30 items by default (`run list`: 20). Use `-L, --limit N` to choose how many, or
  `-L 0` for all. Pagination is followed automatically up to the limit.
- When `-L` is not given and items were left out, a note on stderr says how many were shown of the
  total and how to see more. Nothing is added to stdout or to `--json` output.
- `ls` is an alias for every `list`.

## `--web`

Where offered, `-w, --web` opens the resource in the browser instead of printing it (`open` on
macOS, `xdg-open` on Linux, `rundll32 url.dll,FileProtocolHandler` on Windows).

## Command aliases

| `gh` name | Bitbucket name |
|---|---|
| `bb run` | `bb pipeline` |
| `bb run cancel` | `bb pipeline stop` |
| `bb org` | `bb workspace`, `bb ws` |
| `bb pr close` | `bb pr decline` |
| `bb pr checkout` | `bb pr co` |
| `bb pr create`, `bb repo create` | `new` |

## Errors

Error messages go to stderr; API errors show as `HTTP <code>: <message>`. Exit codes follow `gh`:
see [exit codes](json-and-exit-codes.md#exit-codes).

## No prompts

Apart from `bb auth login`, `bb` never prompts and has no editor mode. Where `gh` would prompt for
a missing argument, `bb` uses a default (for example the current branch's pull request) or fails.
Destructive commands need an explicit `--yes` (`repo delete`, `pr comment --delete-last`). Pass
bodies with `-b TEXT` or `-F FILE` (`-F -` reads stdin).
