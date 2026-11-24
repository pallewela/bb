# JSON output and exit codes

## `--json`

The global `--json` flag prints the **raw Bitbucket API JSON** instead of a table. List commands
print a single JSON array of the items from every page they fetched. There is no field selection:
pipe to `jq`.

```sh
bb pr list --json | jq -r '.[] | "\(.id)\t\(.title)"'
bb pr view 42 --json | jq '{state, draft, branch: .source.branch.name}'
bb run list -b main -L 1 --json | jq '.[0].state'
```

Notes such as "Showing 30 of 120..." go to stderr, never into the `--json` output, so stdout is
always one valid JSON document.

`gh`'s `--json FIELDS`, `--jq` and `--template` are not supported (they are a v1 non-goal).

### Fields you will commonly need

Pull request (`pr view --json`, items of `pr list --json`):
`.id`, `.title`, `.state` (`OPEN`, `MERGED`, `DECLINED`, `SUPERSEDED`), `.draft`,
`.source.branch.name`, `.destination.branch.name`, `.author.display_name`,
`.participants[] | {approved, state, role}` (`state` is `"changes_requested"` when a reviewer
requested changes), `.links.html.href`, `.updated_on`.

Check (`pr checks --json` items): `.state` (`SUCCESSFUL`, `FAILED`, `INPROGRESS`, `STOPPED`),
`.name`, `.key`, `.url`.

Pipeline (`run list --json` items, newest first): `.build_number`, `.state.name`
(`PENDING`, `IN_PROGRESS`, `COMPLETED`), `.state.result.name` (`SUCCESSFUL`, `FAILED`, `ERROR`,
`STOPPED`), `.target.ref_name`. `run view N --json` prints `{pipeline, steps}`.

Comment (`pr comments --json` items): `.id`, `.content.raw`, `.user.account_id`, `.deleted`,
`.inline.path`, `.links.self.href`.

`repo view -b BRANCH --json` still prints the repository, not the branch.

## Exit codes

Exit codes follow `gh`. Check the exit code rather than parsing error text.

| Exit | Meaning |
|---|---|
| 0 | Success. For `pr checks`: all checks passed, or none were reported. |
| 1 | Error (including an API error, shown as `HTTP <code>: <message>` on stderr). Also: `pr checks` when a check failed or was stopped; `run view --exit-status` / `run watch --exit-status` when the pipeline failed; `bb api` on a non-2xx response. |
| 2 | Usage error: an unknown flag or a bad argument (reported by the argument parser). Fix the command; don't retry it as-is. |
| 4 | Authentication required: no usable credentials. Run `bb auth login` or set `BITBUCKET_EMAIL` + `BITBUCKET_TOKEN`. |
| 8 | `pr checks`: some checks are still in progress. Wait (`bb pr checks ID --watch`) or report "pending". |

`bb` has no interactive prompts to cancel, so unlike `gh` it never uses exit code 2 to mean
"cancelled".

`pr checks --json` still prints the JSON array of checks when it exits 1 or 8, so a script can tell
"a check failed" (exit 1 and an array on stdout) from a real error (exit 1 and no JSON):

```sh
out="$(bb pr checks 42 --json 2>/dev/null)"; rc=$?
if jq -e 'type == "array"' >/dev/null 2>&1 <<<"$out"; then
  case $rc in 0) echo passing ;; 1) echo failing ;; 8) echo pending ;; esac
else
  echo "could not read checks (exit $rc)" >&2
fi
```
