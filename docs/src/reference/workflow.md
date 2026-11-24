# bb workflow

Run custom pipelines (the `gh workflow` analog).

## bb workflow run `[NAME]`

Run a custom pipeline `NAME` (from `bitbucket-pipelines.yml` `custom:`), or, with no `NAME`, the
default pipeline for the ref. The ref defaults to the repository's main branch, as in `gh`.

| Flag | Meaning |
|---|---|
| `-r, --ref <BRANCH>` (alias `--branch`) | Branch to run on (default: the main branch) |
| `-f, --raw-field <KEY=VALUE>` (alias `--var`) | Pipeline variable (repeatable) |
| `-F, --field <KEY=VALUE>` | Pipeline variable; VALUE `@file` reads a file, `@-` stdin (repeatable) |

```sh
bb workflow run deploy -r main -f ENV=staging
```

## Differences from `gh`

- *Gap:* `workflow run --json` (inputs as JSON on stdin) clashes with the global `--json` flag; use
  `-f`/`-F`. `--ref` takes a branch only.
- *Gap:* no `workflow list/view/enable/disable`: custom pipelines live in
  `bitbucket-pipelines.yml`, and there is no API to list or toggle them.
