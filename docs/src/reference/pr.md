# bb pr

Work with pull requests.

`[PR]` is a numeric id (optionally `#12`), a PR URL, or a branch name, as in `gh`. A branch, or an
omitted `[PR]` (meaning the current git branch), resolves to the open PR whose source is that
branch, else the most recently updated PR from it in any state. Scripts and agents should pass an
explicit id for every write.

## bb pr list (alias `ls`)

List pull requests. Columns: id, title, source branch, author, state (+ draft).

| Flag | Meaning |
|---|---|
| `-s, --state <STATE>` | `open`, `closed` (merged, declined or superseded), `merged`, `all`, `declined`, `superseded` [default: open] |
| `-A, --author <AUTHOR>` | Filter by author username |
| `-B, --base <BASE>` | Filter by base (destination) branch |
| `-H, --head <HEAD>` | Filter by head (source) branch |
| `-S, --search <QUERY>` | Search pull request titles and descriptions (substring match) |
| `-d, --draft` | Only draft pull requests |
| `-w, --web` | Open the pull request list in the browser |
| `-L, --limit <N>` | Maximum number of pull requests to list [default: 30; 0 = all] |

How `--state` maps to Bitbucket states:

| `--state` | Bitbucket states | |
|---|---|---|
| `open` | OPEN | `gh` |
| `merged` | MERGED | `gh` |
| `closed` | MERGED, DECLINED, SUPERSEDED | `gh` (`gh`'s `closed` includes merged) |
| `all` | all four | `gh` |
| `declined` | DECLINED | extra |
| `superseded` | SUPERSEDED | extra |

## bb pr view `[PR]`

Show a pull request: title, state, author, branches, reviewers and approval status, description,
URL.

| Flag | Meaning |
|---|---|
| `-c, --comments` | Also show comments |
| `-w, --web` | Open in the browser |

## bb pr create (alias `new`)

Create a pull request and print its URL. The head defaults to the current branch, the base to the
repository's main branch.

| Flag | Meaning |
|---|---|
| `-t, --title <TITLE>` | Title |
| `-b, --body <BODY>` | Description |
| `-F, --body-file <FILE>` | Read the body from FILE (`-` for stdin) |
| `-B, --base <BASE>` | Base (destination) branch (default: repo main branch) |
| `-H, --head <HEAD>` | Head (source) branch (default: current branch) |
| `-d, --draft` | Mark the pull request as a draft |
| `-r, --reviewer <REVIEWER>` | Reviewer UUID (`{...}`) or account_id; repeatable |
| `--close-source-branch` | Close the source branch when the pull request is merged |
| `-f, --fill` | Use commit info for title and body |
| `--fill-first` | Use the first commit's info for title and body |
| `--fill-verbose` | Use commit messages and bodies for the description |
| `--dry-run` | Print what would be created instead of creating it |
| `-w, --web` | Open the create page in the browser instead |

Fill modes read the commits in `origin/BASE..HEAD` (falling back to the last commit). `--fill` uses
one commit's subject and body, or for several commits the humanized branch name as title and a
commit list as body. Explicit `-t`/`-b` win.

## bb pr checkout `PR` (alias `co`)

Check out a pull request's source branch.

| Flag | Meaning |
|---|---|
| `-b, --branch <BRANCH>` | Local branch name to use (default: the source branch name) |
| `-f, --force` | Reset the existing local branch to the pull request's state |
| `--detach` | Check out with a detached HEAD |

## bb pr diff `[PR]`

Show a pull request's diff.

| Flag | Meaning |
|---|---|
| `--name-only` | Only list the names of changed files |
| `--patch` | Show the diff in patch format (one patch per commit) |
| `--color <COLOR>` | `always`, `never` or `auto` [default: auto] |
| `-e, --exclude <PATTERN>` | Exclude files whose path or file name matches a glob (repeatable) |
| `-w, --web` | Open the diff in the browser |

## bb pr merge `[PR]`

Merge a pull request.

| Flag | Meaning |
|---|---|
| `-m, --merge` | Merge commit (Bitbucket `merge_commit`) |
| `-s, --squash` | Squash the commits into one (Bitbucket `squash`) |
| `-r, --rebase` | Rebase the commits onto the base branch (Bitbucket `rebase_fast_forward`) |
| `--fast-forward` | Fast-forward only (Bitbucket `fast_forward`) |
| `--rebase-merge` | Rebase, then create a merge commit (Bitbucket `rebase_merge`) |
| `--squash-fast-forward` | Squash, then fast-forward (Bitbucket `squash_fast_forward`) |
| `-t, --subject <SUBJECT>` | Subject text for the merge commit |
| `-b, --body <BODY>` | Body text for the merge commit |
| `-F, --body-file <FILE>` | Read the merge commit body from FILE (`-` for stdin) |
| `-d, --delete-branch` | Delete the source branch on Bitbucket and locally after merge |
| `--close-source-branch` | Close the source branch on Bitbucket after merge (keeps the local branch) |

## bb pr close `[PR]` (alias `decline`)

Close (decline) a pull request.

| Flag | Meaning |
|---|---|
| `-c, --comment <COMMENT>` | Leave a closing comment |
| `-d, --delete-branch` | Delete the source branch on Bitbucket and locally after closing |

## bb pr review `[PR]`

Add a review: approve, request changes or comment. One of `-a`, `-r`, `-c` is required; a body is
required for `-r` and `-c` and is posted as a comment.

| Flag | Meaning |
|---|---|
| `-a, --approve` | Approve the pull request |
| `-r, --request-changes` | Request changes on the pull request |
| `-c, --comment` | Comment on the pull request |
| `-b, --body <BODY>` | Review body |
| `-F, --body-file <FILE>` | Read the review body from FILE (`-` for stdin) |

## bb pr approve / unapprove `[PR]`

Approve a pull request, or remove your approval.

## bb pr request-changes `[PR]`

Request changes on a pull request.

| Flag | Meaning |
|---|---|
| `--undo` | Withdraw the change request |

## bb pr comment `[PR]`

Comment on a pull request.

| Flag | Meaning |
|---|---|
| `-b, --body <BODY>` | Comment text |
| `-F, --body-file <FILE>` | Read the comment from FILE (`-` for stdin) |
| `--file <FILE>` | File path for an inline comment |
| `--line <LINE>` | Line (new version) for an inline comment |
| `--edit-last` | Edit your last comment instead of adding one |
| `--create-if-none` | With `--edit-last`: add a comment if you have none |
| `--delete-last` | Delete your last comment (needs `--yes`) |
| `--yes` | Confirm `--delete-last` |
| `-w, --web` | Open the pull request in the browser to comment there |

`--edit-last` and `--delete-last` find your newest comment via `GET /user`, so the token needs the
`read:user` scope.

## bb pr comments `[PR]`

List comments on a pull request: author, location (if inline), body.

## bb pr checks `[PR]`

Show build statuses for a pull request: state, name, URL. Exits 1 if any check failed or was
stopped, 8 if any is still in progress, 0 otherwise (including when no checks are reported).

| Flag | Meaning |
|---|---|
| `--watch` | Watch checks until they finish |
| `-i, --interval <SECONDS>` | Refresh interval in seconds for `--watch` [default: 10] |
| `--fail-fast` | Stop watching at the first failed check |
| `-w, --web` | Open the pull request in the browser |

## Differences from `gh`

- *Extra:* `pr decline` is the Bitbucket name for `pr close`; both work.
- *Extra:* `pr approve`, `pr unapprove` and `pr request-changes [--undo]` alongside `gh`'s
  `pr review -a/-r/-c`.
- *Extra:* `pr comments` (`gh` only has `pr view --comments`, which `bb` also supports).
- *Extra:* `pr comment --file/--line` for inline comments.
- *Extra:* `pr create --close-source-branch`; `pr merge --fast-forward`, `--rebase-merge`,
  `--squash-fast-forward` and `--close-source-branch`. `-r/--rebase` uses `rebase_fast_forward`,
  the closest to `gh`'s rebase.
- *Gap:* `pr create -r/--reviewer` needs a UUID or account_id, not a handle: Bitbucket can't look
  users up by nickname.
- *Gap:* no labels, assignees or milestones (Bitbucket has none).
- *Gap:* no `pr checks --required`, `pr merge --auto/--admin/--match-head-commit`,
  `pr create --no-maintainer-edit/--recover/--attach`, `pr comment --attach`, or `-e/--editor`.
- *Gap:* `pr list -S` is a substring match on title and description, not GitHub search syntax.
- *Gap:* `pr checks` with nothing reported prints "No checks reported" and exits 0.
