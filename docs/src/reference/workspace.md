# bb workspace (aliases `org`, `ws`)

Work with workspaces. `bb org list` is the `gh org list` analog.

## bb workspace list (alias `ls`)

List workspaces you can access. Columns: slug, role (admin/member). Needs the
`read:workspace:bitbucket` token scope; a 403 explains that.

| Flag | Meaning |
|---|---|
| `--admin` | Only workspaces you administer |
| `-L, --limit <N>` | Maximum number of workspaces to list [default: 30; 0 = all] |

## Differences from `gh`

- *Extra:* `--admin` (Bitbucket's per-workspace admin flag); `workspace` and `ws` as Bitbucket
  names for `org`.
