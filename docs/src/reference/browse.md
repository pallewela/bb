# bb browse

```text
bb browse [OPTIONS] [NUMBER|PATH|SHA]
```

Open the repository on bitbucket.org. As in `gh browse`, the argument is a PR if it is a number, a
commit if it is 7 to 40 lowercase hex characters, and otherwise a file or directory path relative
to the repository root (`PATH:LINE` or `PATH:START-END` selects lines).

| Flag | Meaning |
|---|---|
| `-b, --branch <BRANCH>` | Branch to browse (default with PATH: current branch, else the main branch) |
| `-c, --commit[=<SHA>]` | Browse at a commit: `--commit=SHA`, or the last commit when no SHA is given |
| `--blame` | Open the blame view for a file |
| `-s, --settings` | Open the repository settings |
| `-a, --actions` | Open the repository's Pipelines (the `gh` Actions analog) |
| `-w, --wiki` | Open the repository wiki |
| `-n, --no-browser` | Print the URL instead of opening a browser |

```sh
bb browse src/main.rs:10-20 -n
bb browse 42
```

## Differences from `gh`

- *Gap:* `-p/--projects` and `-r/--releases` have no Bitbucket equivalent.
- *Gap:* `--commit` and `--branch` can't be combined (in `gh`, `--commit` wins).
