# Command reference

```text
bb [OPTIONS] <COMMAND>

Commands:
  auth       Manage credentials
  repo       Work with repositories
  workspace  Work with workspaces [alias: org]
  pr         Work with pull requests
  project    Work with projects (groups of repositories in a workspace)
  pipeline   Work with Bitbucket Pipelines
  workflow   Run custom pipelines (the `gh workflow` analog)
  api        Make an authenticated API request
  browse     Open the repository in the browser
  skill      Install the agent skill that teaches AI coding agents to use bb
  help       Print this message or the help of the given subcommand(s)
```

Global options, accepted by every command:

| Flag | Meaning |
|---|---|
| `-R, --repo <WORKSPACE/REPO>` | Select another repository: `WORKSPACE/REPO` or a bitbucket.org URL |
| `--json` | Print raw API JSON instead of human-readable output |
| `-h, --help` | Print help |
| `-V, --version` | Print version (top level only) |

The pages in this section list each subcommand's own flags, taken from `bb <command> --help`.
`bb <command> <subcommand> --help` is always the authoritative reference for the version you have
installed.

Each page ends with the differences from `gh` for that command group: *extras* are Bitbucket
additions `gh` lacks, *gaps* are `gh` forms `bb` does not support (with the reason).
