# bb skill

```text
bb skill install [OPTIONS]
```

Install or update the [agent skill](../agent-skill.md) for Claude Code. The skill is built into
`bb`, so the installed copy matches the `bb` you have; run it again after upgrading. It writes
`bitbucket-cli/SKILL.md` in the skills directory and says whether it installed, updated or found
it already up to date. It refuses to write through a `bitbucket-cli` symlink (as made by linking a
clone), since that would overwrite the clone's copy; remove the link first.

| Flag | Meaning |
|---|---|
| `--dir <DIR>` | Skills directory to install into (default: `$CLAUDE_CONFIG_DIR/skills`, else `~/.claude/skills`) |

```sh
bb skill install
bb skill install --dir .claude/skills   # for one project only
```

## Differences from `gh`

- *Extra:* `gh` has no equivalent.
