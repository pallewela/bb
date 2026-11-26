# bb repo

Work with repositories.

## bb repo list (alias `ls`)

```text
bb repo list [OPTIONS] [WORKSPACE]
```

List repositories in a workspace (default: the current repo's workspace). Columns: full name,
visibility, language, updated.

| Flag | Meaning |
|---|---|
| `-L, --limit <N>` | Maximum number of repositories to list [default: 30; 0 = all] |
| `--visibility <VISIBILITY>` | Filter by visibility: `public`, `private` |
| `-l, --language <LANGUAGE>` | Filter by primary language |
| `--fork` | Show only forks |
| `--source` | Show only non-forks |
| `-p, --project <KEY>` | Only repositories in this project |

## bb repo view

```text
bb repo view [OPTIONS] [REPO]
```

Show a repository: name, description, main branch, visibility, clone URLs, web URL. `REPO` is
`WORKSPACE/REPO` or a URL (default: the current repo).

| Flag | Meaning |
|---|---|
| `-w, --web` | Open in the browser |
| `-b, --branch <BRANCH>` | Show this branch's latest commit (with `--web`: open the branch). `--json` still prints the repository. |

## bb repo clone

```text
bb repo clone [OPTIONS] <REPO> [DIR] [-- <GIT_ARGS>...]
```

Clone a repository over SSH (`git@bitbucket.org:WORKSPACE/REPO.git`). Arguments after `--` are
passed to `git clone`.

## bb repo create (alias `new`)

```text
bb repo create [OPTIONS] <REPO>
```

Create a repository `WORKSPACE/REPO` (private by default).

| Flag | Meaning |
|---|---|
| `--private` | Make the repository private (default) |
| `--public` | Make the repository public |
| `-d, --description <DESCRIPTION>` | Repository description |
| `-p, --project <PROJECT>` | Project key to create the repository in |
| `-c, --clone` | Clone the new repository afterwards |

## bb repo edit

```text
bb repo edit [OPTIONS] <--description <DESCRIPTION>|--visibility <VISIBILITY>|--project <KEY>> [REPO]
```

Edit a repository (default: the current repo). Only the given fields change. `bb` checks that the
repository exists first, because Bitbucket's update call would otherwise create it.

| Flag | Meaning |
|---|---|
| `-d, --description <DESCRIPTION>` | New description |
| `--visibility <VISIBILITY>` | New visibility: `public`, `private` (needs `--accept-visibility-change-consequences`) |
| `--accept-visibility-change-consequences` | Confirm a visibility change |
| `-p, --project <KEY>` | Move the repository to this project |

```sh
bb repo edit ws/api --project PLAT
```

## bb repo delete

```text
bb repo delete [OPTIONS] [REPO]
```

Delete a repository (default: the current repo). Refuses without `--yes`.

| Flag | Meaning |
|---|---|
| `--yes` | Confirm deletion |

## Differences from `gh`

- `repo list` with no argument uses the current repo's workspace rather than "your" repos, because
  Bitbucket removed the cross-workspace listing.
- *Extra:* `repo create -p/--project KEY` (Bitbucket projects).
- *Gap:* `repo create` is private by default and needs `WORKSPACE/REPO`; `bb` doesn't prompt.
- *Gap:* `repo delete` refuses without `--yes` instead of prompting.
- *Gap:* `repo clone` is SSH only (API tokens don't work for git over HTTPS); no fork/upstream flags.
- *Gap:* no `repo list --archived`/`--no-archived`, `--visibility internal` or `--topic`
  (Bitbucket has no archived flag, internal visibility or topics).
- *Gap:* `repo view -b` shows the branch's head commit rather than its README.
