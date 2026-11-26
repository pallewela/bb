# bb project

Work with projects: the groups a workspace's repositories are organized in. The verbs and flags
are `gh project`'s; `PROJECT` is the project's key (e.g. `PROJ`), not its name. The workspace is
`--owner WORKSPACE`, else the workspace of `-R` or of the current repo.

Needs the `read:project:bitbucket` scope, and `admin:project:bitbucket` to create, edit or delete.

## bb project list (alias `ls`)

```text
bb project list [OPTIONS]
```

List the workspace's projects, sorted by key. Columns: key, name, visibility, updated.

| Flag | Meaning |
|---|---|
| `--owner <WORKSPACE>` | Workspace to list (default: the current repo's workspace) |
| `-L, --limit <N>` | Maximum number of projects to list [default: 30; 0 = all] |
| `-w, --web` | Open the workspace's projects page in the browser |

## bb project view

```text
bb project view [OPTIONS] <KEY>
```

Show a project: key, name, description, visibility, last update and URL.

| Flag | Meaning |
|---|---|
| `--owner <WORKSPACE>` | Workspace |
| `-w, --web` | Open in the browser |

## bb project create

```text
bb project create [OPTIONS] --title <NAME> <KEY>
```

Create a project (private by default) and print its URL. The key is letters, numbers and
underscores.

| Flag | Meaning |
|---|---|
| `--title <NAME>` | Project name |
| `--owner <WORKSPACE>` | Workspace |
| `-d, --description <DESCRIPTION>` | Description |
| `--visibility <VISIBILITY>` | `public` or `private` (any case) [default: private] |

## bb project edit

```text
bb project edit [OPTIONS] <--title <NAME>|--description <DESCRIPTION>|--visibility <VISIBILITY>> <KEY>
```

Change a project's name, description or visibility; other fields stay as they are. A key that
doesn't exist is an error (Bitbucket's update call would otherwise create the project).

| Flag | Meaning |
|---|---|
| `--owner <WORKSPACE>` | Workspace |
| `--title <NAME>` | New project name |
| `-d, --description <DESCRIPTION>` | New description |
| `--visibility <VISIBILITY>` | New visibility: `public` or `private` |

## bb project delete

```text
bb project delete [OPTIONS] <KEY>
```

Delete a project. Refuses without `--yes`. Bitbucket only deletes empty projects: move or delete
its repositories first (`bb repo list -p KEY`, `bb repo edit REPO -p OTHER`).

| Flag | Meaning |
|---|---|
| `--owner <WORKSPACE>` | Workspace |
| `--yes` | Confirm deletion |

```sh
bb project list --owner acme
bb project create PLAT --title Platform --owner acme
bb repo edit acme/api --project PLAT
bb repo list acme -p PLAT
```

## Differences from `gh`

- `gh project` manages GitHub Projects (boards); `bb project` manages Bitbucket projects, which
  group repositories. The shared verbs and flags work the same way.
- A project is named by its KEY where `gh` uses a number, and the KEY is required (`gh` prompts).
- With no `--owner`, `bb` uses the current repo's workspace (`gh` uses your own account).
- *Extra:* `project create` takes the KEY, `-d` and `--visibility`.
- *Gap:* `project delete` needs `--yes` (`gh` doesn't ask).
- *Gap:* `list --closed`, `close`, `copy`, `mark-template`, `link`/`unlink`, `field-*`, `item-*` and
  `edit --readme` have no Bitbucket counterpart. `--format`, `--jq` and `--template`: use `--json`.
