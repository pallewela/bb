//! `bb pr`: pull requests.
use crate::client::{enc, limit_arg, Client};
use crate::context::{self, Repo};
use crate::exit::{self, Exit};
use crate::output::{self, s};
use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Args, Subcommand};
use serde_json::{json, Value};
use std::io::{IsTerminal, Read};
use std::time::Duration;

#[derive(Args)]
pub struct PrArg {
    /// PR number, URL or branch (default: the pull request for the current branch)
    pr: Option<String>,
}

/// `--state` values: `gh`'s (open, closed, merged, all) plus Bitbucket's declined and superseded.
const STATES: [&str; 6] = ["open", "closed", "merged", "all", "declined", "superseded"];

#[derive(Subcommand)]
pub enum Cmd {
    /// List pull requests
    #[command(visible_alias = "ls")]
    List {
        /// Filter by state: open, closed (merged, declined or superseded), merged, all, declined, superseded
        #[arg(short, long, default_value = "open", value_parser = STATES, hide_possible_values = true)]
        state: String,
        /// Filter by author username
        #[arg(short = 'A', long)]
        author: Option<String>,
        /// Filter by base (destination) branch
        #[arg(short = 'B', long)]
        base: Option<String>,
        /// Filter by head (source) branch
        #[arg(short = 'H', long)]
        head: Option<String>,
        /// Search pull request titles and descriptions
        #[arg(short = 'S', long, value_name = "QUERY")]
        search: Option<String>,
        /// Only draft pull requests
        #[arg(short, long)]
        draft: bool,
        /// Open the pull request list in the browser
        #[arg(short, long)]
        web: bool,
        /// Maximum number of pull requests to list [default: 30; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show a pull request
    View {
        #[command(flatten)]
        p: PrArg,
        /// Also show comments
        #[arg(short, long)]
        comments: bool,
        /// Open in the browser
        #[arg(short, long)]
        web: bool,
    },
    /// Create a pull request
    #[command(visible_alias = "new", group(ArgGroup::new("fill_mode")))]
    Create {
        #[arg(short, long)]
        title: Option<String>,
        #[arg(short, long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the body from FILE ("-" for stdin)
        #[arg(short = 'F', long, value_name = "FILE")]
        body_file: Option<String>,
        /// Base (destination) branch (default: repo main branch)
        #[arg(short = 'B', long)]
        base: Option<String>,
        /// Head (source) branch (default: current branch)
        #[arg(short = 'H', long)]
        head: Option<String>,
        /// Mark the pull request as a draft
        #[arg(short, long)]
        draft: bool,
        /// Reviewer uuid (`{...}`) or account_id; repeatable
        #[arg(short, long)]
        reviewer: Vec<String>,
        /// Close the source branch when the pull request is merged
        #[arg(long)]
        close_source_branch: bool,
        /// Use commit info for title and body
        #[arg(short, long, group = "fill_mode")]
        fill: bool,
        /// Use the first commit's info for title and body
        #[arg(long, group = "fill_mode")]
        fill_first: bool,
        /// Use commit messages and bodies for the description
        #[arg(long, group = "fill_mode")]
        fill_verbose: bool,
        /// Print what would be created instead of creating it
        #[arg(long)]
        dry_run: bool,
        /// Open the create page in the browser instead
        #[arg(short, long)]
        web: bool,
    },
    /// Check out a pull request's source branch
    #[command(visible_alias = "co")]
    Checkout {
        /// PR number, URL or branch
        pr: String,
        /// Local branch name to use (default: the source branch name)
        #[arg(short, long)]
        branch: Option<String>,
        /// Reset the existing local branch to the pull request's state
        #[arg(short, long)]
        force: bool,
        /// Check out with a detached HEAD
        #[arg(long, conflicts_with_all = ["branch", "force"])]
        detach: bool,
    },
    /// Show a pull request's diff
    Diff {
        #[command(flatten)]
        p: PrArg,
        /// Only list the names of changed files
        #[arg(long)]
        name_only: bool,
        /// Show the diff in patch format (one patch per commit)
        #[arg(long, conflicts_with = "name_only")]
        patch: bool,
        /// Use color: always, never or auto
        #[arg(long, default_value = "auto", value_parser = ["always", "never", "auto"])]
        color: String,
        /// Exclude files matching a glob pattern (repeatable)
        #[arg(short, long, value_name = "PATTERN")]
        exclude: Vec<String>,
        /// Open the diff in the browser
        #[arg(short, long)]
        web: bool,
    },
    /// Merge a pull request
    #[command(group(ArgGroup::new("strategy")))]
    Merge {
        #[command(flatten)]
        p: PrArg,
        /// Merge commit (Bitbucket `merge_commit`)
        #[arg(short, long, group = "strategy")]
        merge: bool,
        /// Squash the commits into one (Bitbucket `squash`)
        #[arg(short, long, group = "strategy")]
        squash: bool,
        /// Rebase the commits onto the base branch (Bitbucket `rebase_fast_forward`)
        #[arg(short, long, group = "strategy")]
        rebase: bool,
        /// Fast-forward only (Bitbucket `fast_forward`)
        #[arg(long, group = "strategy")]
        fast_forward: bool,
        /// Subject text for the merge commit
        #[arg(short = 't', long, alias = "message")]
        subject: Option<String>,
        /// Body text for the merge commit
        #[arg(short, long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the merge commit body from FILE ("-" for stdin)
        #[arg(short = 'F', long, value_name = "FILE")]
        body_file: Option<String>,
        /// Delete the source branch on Bitbucket and locally after merge
        #[arg(short, long)]
        delete_branch: bool,
        /// Close the source branch on Bitbucket after merge (keeps the local branch)
        #[arg(long)]
        close_source_branch: bool,
    },
    /// Close (decline) a pull request
    #[command(visible_alias = "decline")]
    Close {
        #[command(flatten)]
        p: PrArg,
        /// Leave a closing comment
        #[arg(short, long)]
        comment: Option<String>,
        /// Delete the source branch on Bitbucket and locally after closing
        #[arg(short, long)]
        delete_branch: bool,
    },
    /// Add a review: approve, request changes or comment
    #[command(group(ArgGroup::new("action").required(true)))]
    Review {
        #[command(flatten)]
        p: PrArg,
        /// Approve the pull request
        #[arg(short, long, group = "action")]
        approve: bool,
        /// Request changes on the pull request
        #[arg(short, long, group = "action")]
        request_changes: bool,
        /// Comment on the pull request
        #[arg(short, long, group = "action")]
        comment: bool,
        /// Review body
        #[arg(short, long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the review body from FILE ("-" for stdin)
        #[arg(short = 'F', long, value_name = "FILE")]
        body_file: Option<String>,
    },
    /// Approve a pull request
    Approve(PrArg),
    /// Remove your approval
    Unapprove(PrArg),
    /// Request changes on a pull request
    RequestChanges {
        #[command(flatten)]
        p: PrArg,
        /// Withdraw the change request
        #[arg(long)]
        undo: bool,
    },
    /// Comment on a pull request
    Comment {
        #[command(flatten)]
        p: PrArg,
        /// Comment text
        #[arg(short, long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the comment from FILE ("-" for stdin)
        #[arg(short = 'F', long, value_name = "FILE")]
        body_file: Option<String>,
        /// File path for an inline comment
        #[arg(long)]
        file: Option<String>,
        /// Line (new version) for an inline comment
        #[arg(long, requires = "file")]
        line: Option<u64>,
        /// Edit your last comment instead of adding one
        #[arg(long, conflicts_with = "delete_last")]
        edit_last: bool,
        /// With --edit-last: add a comment if you have none
        #[arg(long, requires = "edit_last")]
        create_if_none: bool,
        /// Delete your last comment (needs --yes)
        #[arg(long)]
        delete_last: bool,
        /// Confirm --delete-last
        #[arg(long)]
        yes: bool,
        /// Open the pull request in the browser to comment there
        #[arg(short, long)]
        web: bool,
    },
    /// List comments on a pull request
    Comments(PrArg),
    /// Show build statuses for a pull request (exit 1 if any failed, 8 if any are pending)
    Checks {
        #[command(flatten)]
        p: PrArg,
        /// Watch checks until they finish
        #[arg(long)]
        watch: bool,
        /// Refresh interval in seconds for --watch
        #[arg(short, long, default_value_t = 10, value_name = "SECONDS")]
        interval: u64,
        /// Stop watching at the first failed check
        #[arg(long, requires = "watch")]
        fail_fast: bool,
        /// Open the pull request in the browser
        #[arg(short, long)]
        web: bool,
    },
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { state, author, base, head, search, draft, web, limit } => {
            let repo = Repo::current()?;
            if web {
                return output::open_browser(&format!("{}/pull-requests/?state={}", repo.web(), web_state(&state)));
            }
            let c = Client::new()?;
            let (limit, chosen) = limit_arg(limit, 30);
            let f = Filter { state: &state, author, base, head, search, draft };
            let list = c.list(&format!("{}/pullrequests?{}", repo.api(), list_query(&f, limit)), limit)?;
            let prs = &list.items;
            if output::json_mode() {
                output::print_json(&Value::Array(list.items));
            } else if prs.is_empty() {
                eprintln!("No pull requests found");
            } else {
                let mut rows = vec![["ID", "TITLE", "BRANCH", "AUTHOR", "STATE"].map(String::from).to_vec()];
                rows.extend(prs.iter().map(|p| {
                    let draft = if p["draft"] == true { " (draft)" } else { "" };
                    vec![
                        format!("#{}", s(p, "id")),
                        s(p, "title"),
                        s(p, "source.branch.name"),
                        s(p, "author.display_name"),
                        format!("{}{draft}", s(p, "state")),
                    ]
                }));
                output::table(&rows);
                if !chosen {
                    output::more_note(&list, "pull requests", "");
                }
            }
        }
        Cmd::View { p, comments, web } => {
            let (c, path, _) = target(&p.pr)?;
            let pr = c.get(&path)?;
            if web {
                return output::open_browser(&s(&pr, "links.html.href"));
            }
            if output::json_mode() {
                output::print_json(&pr);
                return Ok(());
            }
            let draft = if pr["draft"] == true { " (draft)" } else { "" };
            println!("{} #{}", s(&pr, "title"), s(&pr, "id"));
            println!(
                "{}{draft} • {} • {} → {}",
                s(&pr, "state"),
                s(&pr, "author.display_name"),
                s(&pr, "source.branch.name"),
                s(&pr, "destination.branch.name")
            );
            for r in pr["participants"].as_array().into_iter().flatten() {
                let status = if r["approved"] == true {
                    "approved"
                } else if r["state"] == "changes_requested" {
                    "changes requested"
                } else if r["role"] == "REVIEWER" {
                    "pending"
                } else {
                    continue;
                };
                println!("Reviewer: {} ({status})", s(r, "user.display_name"));
            }
            let desc = s(&pr, "description");
            if !desc.is_empty() {
                println!("\n{desc}\n");
            }
            println!("{}", s(&pr, "links.html.href"));
            if comments {
                println!();
                print_comments(&c.paginate(&format!("{path}/comments?pagelen=100"), usize::MAX)?);
            }
        }
        Cmd::Create {
            title,
            body,
            body_file,
            base,
            head,
            draft,
            reviewer,
            close_source_branch,
            fill,
            fill_first,
            fill_verbose,
            dry_run,
            web,
        } => {
            let repo = Repo::current()?;
            let head = match head {
                Some(h) => h,
                None => context::current_branch()?,
            };
            if web {
                return output::open_browser(&format!("{}/pull-requests/new?source={}", repo.web(), enc(&head)));
            }
            let body = read_body(body, body_file)?;
            let mode = if fill_first {
                Some(Fill::First)
            } else if fill_verbose {
                Some(Fill::Verbose)
            } else if fill {
                Some(Fill::Default)
            } else {
                None
            };
            let (title, body) = match mode {
                Some(mode) if title.is_none() || body.is_none() => {
                    let base = match &base {
                        Some(b) => b.clone(),
                        None => s(&Client::new()?.get(&repo.api())?, "mainbranch.name"),
                    };
                    let (t, b) = fill_text(mode, &branch_commits(&base, &head), &head);
                    (title.unwrap_or(t), body.unwrap_or(b))
                }
                _ => match title {
                    Some(t) => (t, body.unwrap_or_default()),
                    None => bail!("--title is required (or use --fill, --fill-first or --fill-verbose)"),
                },
            };
            let req = create_body(&title, &body, &head, &base, close_source_branch, draft, &reviewer);
            if dry_run {
                if output::json_mode() {
                    output::print_json(&req);
                } else {
                    let base = base.as_deref().unwrap_or("(main branch)");
                    let draft = if draft { " (draft)" } else { "" };
                    println!("Would create pull request{draft} in {}/{}: {head} → {base}", repo.workspace, repo.slug);
                    println!("title: {title}");
                    if !reviewer.is_empty() {
                        println!("reviewers: {}", reviewer.join(", "));
                    }
                    if !body.is_empty() {
                        println!("\n{body}");
                    }
                }
                return Ok(());
            }
            let pr = Client::new()?.post(&format!("{}/pullrequests", repo.api()), Some(&req))?;
            if output::json_mode() {
                output::print_json(&pr);
            } else {
                println!("{}", s(&pr, "links.html.href"));
            }
        }
        Cmd::Checkout { pr, branch, force, detach } => {
            let (c, path, _) = target(&Some(pr))?;
            // ponytail: assumes the source branch lives in `origin`; fork PRs need the fork remote.
            let src = s(&c.get(&path)?, "source.branch.name");
            let remote_ref = format!("refs/remotes/origin/{src}");
            context::git_passthrough(&["fetch", "origin", &format!("+refs/heads/{src}:{remote_ref}")])?;
            let tip = format!("origin/{src}");
            if detach {
                return context::git_passthrough(&["checkout", "--detach", &tip]);
            }
            let local = branch.unwrap_or(src);
            if !local_branch_exists(&local) {
                context::git_passthrough(&["checkout", "-b", &local, "--track", &tip])?;
            } else if force {
                context::git_passthrough(&["checkout", "-B", &local, &tip])?;
            } else {
                context::git_passthrough(&["checkout", &local])?;
                context::git_passthrough(&["merge", "--ff-only", &tip])?;
            }
        }
        Cmd::Diff { p, name_only, patch, color, exclude, web } => {
            let (c, path, _) = target(&p.pr)?;
            if web {
                return output::open_browser(&format!("{}/diff", s(&c.get(&path)?, "links.html.href")));
            }
            if name_only {
                for f in c.paginate(&format!("{path}/diffstat?pagelen=500"), usize::MAX)? {
                    let name = Some(s(&f, "new.path")).filter(|n| !n.is_empty()).unwrap_or_else(|| s(&f, "old.path"));
                    if !excluded(&name, &exclude) {
                        println!("{name}");
                    }
                }
                return Ok(());
            }
            let text = c.get_text(&format!("{path}/{}", if patch { "patch" } else { "diff" }))?;
            let text = exclude_files(&text, &exclude);
            let colored = color == "always" || (color == "auto" && std::io::stdout().is_terminal());
            print!("{}", if colored { colorize(&text) } else { text });
        }
        Cmd::Merge {
            p,
            merge,
            squash,
            rebase,
            fast_forward,
            subject,
            body,
            body_file,
            delete_branch,
            close_source_branch,
        } => {
            let strategy = [(merge, "merge_commit"), (squash, "squash"), (rebase, "rebase_fast_forward"), (fast_forward, "fast_forward")]
                .into_iter()
                .find_map(|(on, name)| on.then_some(name));
            let message = commit_message(subject, read_body(body, body_file)?);
            let (c, path, id) = target(&p.pr)?;
            let close = close_source_branch || delete_branch;
            let v = c.post(&format!("{path}/merge"), Some(&merge_body(strategy, &message, close)))?;
            done(&v, format!("Merged pull request #{id}"));
            if delete_branch {
                delete_local_branch(&s(&v, "source.branch.name"), &s(&v, "destination.branch.name"));
            }
        }
        Cmd::Close { p, comment, delete_branch } => {
            let (c, path, id) = target(&p.pr)?;
            if let Some(text) = comment {
                c.post(&format!("{path}/comments"), Some(&comment_body(&text, &None, None)))?;
            }
            let v = c.post(&format!("{path}/decline"), None)?;
            done(&v, format!("Declined pull request #{id}"));
            if delete_branch {
                let (src, dest) = (s(&v, "source.branch.name"), s(&v, "destination.branch.name"));
                let repo_api = path.split("/pullrequests/").next().unwrap_or_default();
                c.delete(&format!("{repo_api}/refs/branches/{}", enc_path(&src)))?;
                if !output::json_mode() {
                    println!("Deleted branch {src} on Bitbucket");
                }
                delete_local_branch(&src, &dest);
            }
        }
        Cmd::Review { p, approve, request_changes, comment: _, body, body_file } => {
            let body = read_body(body, body_file)?.filter(|b| !b.trim().is_empty());
            if !approve && body.is_none() {
                bail!("--body or --body-file is required with --request-changes and --comment");
            }
            let (c, path, id) = target(&p.pr)?;
            if let Some(text) = &body {
                c.post(&format!("{path}/comments"), Some(&comment_body(text, &None, None)))?;
            }
            if approve {
                done(&c.post(&format!("{path}/approve"), None)?, format!("Approved pull request #{id}"));
            } else if request_changes {
                done(&c.post(&format!("{path}/request-changes"), None)?, format!("Requested changes on pull request #{id}"));
            } else {
                done(&Value::Null, format!("Reviewed pull request #{id}"));
            }
        }
        Cmd::Approve(p) => {
            let (c, path, id) = target(&p.pr)?;
            done(&c.post(&format!("{path}/approve"), None)?, format!("Approved pull request #{id}"));
        }
        Cmd::Unapprove(p) => {
            let (c, path, id) = target(&p.pr)?;
            done(&c.delete(&format!("{path}/approve"))?, format!("Removed approval from pull request #{id}"));
        }
        Cmd::RequestChanges { p, undo } => {
            let (c, path, id) = target(&p.pr)?;
            let url = format!("{path}/request-changes");
            if undo {
                done(&c.delete(&url)?, format!("Withdrew change request on pull request #{id}"));
            } else {
                done(&c.post(&url, None)?, format!("Requested changes on pull request #{id}"));
            }
        }
        Cmd::Comment { p, body, body_file, file, line, edit_last, create_if_none, delete_last, yes, web } => {
            let (c, path, id) = target(&p.pr)?;
            if web {
                return output::open_browser(&s(&c.get(&path)?, "links.html.href"));
            }
            if delete_last {
                if !yes {
                    bail!("refusing to delete your last comment without --yes");
                }
                let last = last_own_comment(&c, &path)?.context("you have no comments on this pull request")?;
                c.delete(&format!("{path}/comments/{}", s(&last, "id")))?;
                done(&Value::Null, format!("Deleted your last comment on pull request #{id}"));
                return Ok(());
            }
            let body = read_body(body, body_file)?.context("--body or --body-file is required")?;
            if edit_last {
                match last_own_comment(&c, &path)? {
                    Some(last) => {
                        let v = c.put(&format!("{path}/comments/{}", s(&last, "id")), &json!({"content": {"raw": body}}))?;
                        done(&v, format!("Edited your last comment on pull request #{id}"));
                        return Ok(());
                    }
                    None if !create_if_none => bail!("you have no comments on this pull request (use --create-if-none)"),
                    None => {}
                }
            }
            let v = c.post(&format!("{path}/comments"), Some(&comment_body(&body, &file, line)))?;
            done(&v, format!("Commented on pull request #{id}"));
        }
        Cmd::Comments(p) => {
            let (c, path, _) = target(&p.pr)?;
            let list = c.paginate(&format!("{path}/comments?pagelen=100"), usize::MAX)?;
            if output::json_mode() {
                output::print_json(&Value::Array(list));
            } else {
                print_comments(&list);
            }
        }
        Cmd::Checks { p, watch, interval, fail_fast, web } => {
            let (c, path, _) = target(&p.pr)?;
            if web {
                return output::open_browser(&s(&c.get(&path)?, "links.html.href"));
            }
            let statuses = format!("{path}/statuses?pagelen=100");
            let mut list = c.paginate(&statuses, usize::MAX)?;
            if watch {
                let mut shown = String::new();
                loop {
                    let (failed, pending) = tally(&list);
                    let rendered = checks_table(&list);
                    if rendered != shown && !output::json_mode() {
                        print!("{rendered}");
                        println!("{}\n", summary(list.len(), failed, pending));
                        shown = rendered;
                    }
                    if pending == 0 || (fail_fast && failed > 0) {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(interval.max(1)));
                    list = c.paginate(&statuses, usize::MAX)?;
                }
                if output::json_mode() {
                    output::print_json(&Value::Array(list.clone()));
                }
            } else if output::json_mode() {
                output::print_json(&Value::Array(list.clone()));
            } else if list.is_empty() {
                eprintln!("No checks reported");
            } else {
                print!("{}", checks_table(&list));
            }
            let (failed, pending) = tally(&list);
            if failed > 0 {
                bail!("{failed} check(s) failed");
            }
            if pending > 0 {
                return Err(Exit::new(exit::PENDING, format!("{pending} check(s) pending")));
            }
        }
    }
    Ok(())
}

/// Print `msg` for a mutation, or the raw response in --json mode.
fn done(v: &Value, msg: String) {
    if output::json_mode() {
        output::print_json(v);
    } else {
        println!("{msg}");
    }
}

fn print_comments(list: &[Value]) {
    for c in list.iter().filter(|c| c["deleted"] != true) {
        let mut head = format!("{} ({})", s(c, "user.display_name"), output::short_time(&s(c, "created_on")));
        if c["inline"].is_object() {
            let line = Some(s(c, "inline.to")).filter(|l| !l.is_empty()).unwrap_or_else(|| s(c, "inline.from"));
            head += &format!(" {}:{line}", s(c, "inline.path"));
        }
        println!("{head}\n{}\n", s(c, "content.raw"));
    }
}

/// `-b TEXT`, or `-F FILE` (`-` = stdin).
fn read_body(body: Option<String>, file: Option<String>) -> Result<Option<String>> {
    match file.as_deref() {
        None => Ok(body),
        Some("-") => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).context("could not read stdin")?;
            Ok(Some(text))
        }
        Some(f) => std::fs::read_to_string(f).map(Some).with_context(|| format!("could not read {f}")),
    }
}

/// How a `[PR]` argument was written.
#[derive(Debug, PartialEq)]
enum PrRef {
    /// Numeric id (optionally `#12`), or a PR URL whose repo overrides the context.
    Id(Option<Repo>, u64),
    /// Anything else is a branch name, as in `gh`.
    Branch(String),
}

fn parse_pr(arg: &str) -> Result<PrRef> {
    if let Ok(id) = arg.trim_start_matches('#').parse() {
        return Ok(PrRef::Id(None, id));
    }
    if arg.contains("://") {
        if let Some((_, rest)) = arg.split_once("/pull-requests/")
            && let (Ok(id), Ok(repo)) = (rest.split(['/', '?', '#']).next().unwrap_or("").parse(), Repo::parse(arg))
        {
            return Ok(PrRef::Id(Some(repo), id));
        }
        bail!("invalid pull request URL `{arg}`");
    }
    Ok(PrRef::Branch(arg.to_string()))
}

/// Resolve `[PR]` to (client, `/repositories/{ws}/{slug}/pullrequests/{id}`, id).
fn target(arg: &Option<String>) -> Result<(Client, String, u64)> {
    let c = Client::new()?;
    let pr = match arg {
        Some(a) => parse_pr(a)?,
        None => PrRef::Branch(context::current_branch()?),
    };
    let (repo, id) = match pr {
        PrRef::Id(Some(r), id) => (r, id),
        PrRef::Id(None, id) => (Repo::current()?, id),
        PrRef::Branch(branch) => {
            let repo = Repo::current()?;
            let page = c.get(&format!("{}/pullrequests?{}", repo.api(), branch_query(&branch)))?;
            match pick_pr(page["values"].as_array().map_or(&[][..], Vec::as_slice)) {
                Some(id) => (repo, id),
                None => bail!("no pull requests found for branch `{branch}`"),
            }
        }
    };
    let path = format!("{}/pullrequests/{id}", repo.api());
    Ok((c, path, id))
}

/// PRs whose source is `branch`, any state, most recently updated first.
fn branch_query(branch: &str) -> String {
    let q = enc(&format!("source.branch.name=\"{branch}\""));
    format!("q={q}&state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&sort=-updated_on&pagelen=10")
}

/// Like `gh`: the open PR for a branch if there is one, else the most recent one.
fn pick_pr(prs: &[Value]) -> Option<u64> {
    prs.iter().find(|p| p["state"] == "OPEN").or(prs.first()).and_then(|p| p["id"].as_u64())
}

/// Bitbucket states for a `--state` value.
fn states(state: &str) -> &'static [&'static str] {
    match state {
        "merged" => &["MERGED"],
        "closed" => &["MERGED", "DECLINED", "SUPERSEDED"],
        "declined" => &["DECLINED"],
        "superseded" => &["SUPERSEDED"],
        "all" => &["OPEN", "MERGED", "DECLINED", "SUPERSEDED"],
        _ => &["OPEN"],
    }
}

/// The web UI's `?state=` value for a `--state` value.
fn web_state(state: &str) -> &'static str {
    match states(state) {
        [one] => one,
        _ => "ALL",
    }
}

struct Filter<'a> {
    state: &'a str,
    author: Option<String>,
    base: Option<String>,
    head: Option<String>,
    search: Option<String>,
    draft: bool,
}

fn list_query(f: &Filter, limit: usize) -> String {
    let mut params: Vec<String> = states(f.state).iter().map(|st| format!("state={st}")).collect();
    let mut q: Vec<String> = [("author.username", &f.author), ("destination.branch.name", &f.base), ("source.branch.name", &f.head)]
        .into_iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}=\"{v}\"")))
        .collect();
    if let Some(text) = &f.search {
        let text = text.replace('"', "\\\"");
        q.push(format!("(title ~ \"{text}\" OR description ~ \"{text}\")"));
    }
    if f.draft {
        q.push("draft=true".into());
    }
    if !q.is_empty() {
        params.push(format!("q={}", enc(&q.join(" AND "))));
    }
    params.push(format!("pagelen={}", limit.clamp(1, 50)));
    params.join("&")
}

#[derive(Clone, Copy)]
enum Fill {
    /// `--fill`: one commit → its subject/body; several → branch name as title, commit list as body.
    Default,
    /// `--fill-first`: the first commit's subject/body.
    First,
    /// `--fill-verbose`: title as `--fill`; body = every commit's subject and body.
    Verbose,
}

/// (subject, body) of the commits on `head` that aren't on `base`, oldest first.
/// Falls back to the last commit when the range can't be computed.
fn branch_commits(base: &str, head: &str) -> Vec<(String, String)> {
    let range = format!("origin/{base}..{head}");
    let out = context::git(&["log", "--reverse", "--format=%s%x1f%b%x1e", &range])
        .ok()
        .filter(|o| !o.is_empty())
        .or_else(|| context::git(&["log", "-1", "--format=%s%x1f%b%x1e"]).ok())
        .unwrap_or_default();
    out.split('\x1e')
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            let (subj, body) = r.split_once('\x1f').unwrap_or((r, ""));
            (subj.trim().to_string(), body.trim().to_string())
        })
        .collect()
}

fn fill_text(mode: Fill, commits: &[(String, String)], head: &str) -> (String, String) {
    let first = commits.first().cloned().unwrap_or_default();
    let title = if commits.len() == 1 { first.0.clone() } else { humanize(head) };
    match mode {
        Fill::First => first,
        Fill::Default if commits.len() == 1 => first,
        Fill::Default => (title, commits.iter().map(|(subj, _)| format!("- {subj}")).collect::<Vec<_>>().join("\n")),
        Fill::Verbose => {
            let body = commits
                .iter()
                .map(|(subj, body)| if body.is_empty() { subj.clone() } else { format!("{subj}\n\n{body}") })
                .collect::<Vec<_>>()
                .join("\n\n");
            (title, body)
        }
    }
}

/// `my-feature_branch` → `My feature branch` (`gh`'s title for multi-commit `--fill`).
fn humanize(branch: &str) -> String {
    let words = branch.rsplit('/').next().unwrap_or(branch).replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn create_body(
    title: &str,
    body: &str,
    head: &str,
    base: &Option<String>,
    close: bool,
    draft: bool,
    reviewers: &[String],
) -> Value {
    let reviewers: Vec<Value> = reviewers
        .iter()
        .map(|r| if r.starts_with('{') { json!({"uuid": r}) } else { json!({"account_id": r}) })
        .collect();
    let mut v = json!({
        "title": title,
        "description": body,
        "source": {"branch": {"name": head}},
        "close_source_branch": close,
        "draft": draft,
        "reviewers": reviewers,
    });
    if let Some(b) = base {
        v["destination"] = json!({"branch": {"name": b}});
    }
    v
}

/// `gh`'s `--subject` + `--body` → one commit message.
fn commit_message(subject: Option<String>, body: Option<String>) -> Option<String> {
    match (subject, body) {
        (Some(s), Some(b)) => Some(format!("{s}\n\n{b}")),
        (s, b) => s.or(b),
    }
}

fn merge_body(strategy: Option<&str>, message: &Option<String>, close: bool) -> Value {
    let mut v = json!({});
    if let Some(st) = strategy {
        v["merge_strategy"] = json!(st);
    }
    if let Some(m) = message {
        v["message"] = json!(m);
    }
    if close {
        v["close_source_branch"] = json!(true);
    }
    v
}

fn comment_body(body: &str, file: &Option<String>, line: Option<u64>) -> Value {
    let mut v = json!({"content": {"raw": body}});
    if let Some(f) = file {
        v["inline"] = json!({"path": f});
        if let Some(l) = line {
            v["inline"]["to"] = json!(l);
        }
    }
    v
}

/// Percent-encode each `/`-separated segment of a branch name for a URL path.
fn enc_path(name: &str) -> String {
    name.split('/').map(enc).collect::<Vec<_>>().join("/")
}

fn local_branch_exists(name: &str) -> bool {
    context::git(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_ok()
}

/// After `-d/--delete-branch`: delete the local copy of `src`, switching to `dest` first if it's checked out.
/// Problems are warnings: the pull request itself has already been merged or declined.
fn delete_local_branch(src: &str, dest: &str) {
    if src.is_empty() || !local_branch_exists(src) {
        return;
    }
    if context::current_branch().ok().as_deref() == Some(src)
        && let Err(e) = context::git_passthrough(&["checkout", dest])
    {
        eprintln!("bb: warning: could not switch to {dest} ({e:#}); keeping local branch {src}");
        return;
    }
    match context::git_passthrough(&["branch", "-D", src]) {
        Ok(()) if !output::json_mode() => println!("Deleted local branch {src}"),
        Ok(()) => {}
        Err(e) => eprintln!("bb: warning: could not delete local branch {src} ({e:#})"),
    }
}

/// Your most recent non-deleted comment on the PR. Needs `GET /user` (the read:user scope).
fn last_own_comment(c: &Client, path: &str) -> Result<Option<Value>> {
    let me = c.get("/user").context("finding your account (the token needs the read:user scope)")?;
    let me = s(&me, "account_id");
    let list = c.paginate(&format!("{path}/comments?pagelen=100"), usize::MAX)?;
    Ok(list.into_iter().filter(|x| x["deleted"] != true && s(x, "user.account_id") == me).max_by_key(|x| x["id"].as_u64()))
}

/// (failed, pending) counts. Bitbucket states: SUCCESSFUL, FAILED, INPROGRESS, STOPPED.
fn tally(list: &[Value]) -> (usize, usize) {
    let count = |states: &[&str]| list.iter().filter(|x| states.iter().any(|st| x["state"] == *st)).count();
    (count(&["FAILED", "STOPPED"]), count(&["INPROGRESS"]))
}

fn summary(total: usize, failed: usize, pending: usize) -> String {
    let passed = total - failed - pending;
    format!("{failed} failed, {passed} passed, {pending} pending")
}

fn checks_table(list: &[Value]) -> String {
    let mut rows = vec![["STATE", "NAME", "URL"].map(String::from).to_vec()];
    rows.extend(list.iter().map(|x| {
        let name = Some(s(x, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| s(x, "key"));
        vec![s(x, "state"), name, s(x, "url")]
    }));
    output::table_string(&rows)
}

/// Glob match (`*`, `?`) against the whole path or its file name, as `gh pr diff --exclude` does.
fn excluded(path: &str, patterns: &[String]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    patterns.iter().any(|p| glob(p.as_bytes(), path.as_bytes()) || glob(p.as_bytes(), name.as_bytes()))
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    match (p.first(), s.first()) {
        (None, None) => true,
        (Some(b'*'), _) => glob(&p[1..], s) || (!s.is_empty() && s[0] != b'/' && glob(p, &s[1..])),
        (Some(b'?'), Some(&c)) if c != b'/' => glob(&p[1..], &s[1..]),
        (Some(a), Some(b)) if a == b => glob(&p[1..], &s[1..]),
        _ => false,
    }
}

/// Drop the per-file sections of a git diff whose path matches an exclude pattern.
fn exclude_files(diff: &str, patterns: &[String]) -> String {
    if patterns.is_empty() {
        return diff.to_string();
    }
    let mut out = String::new();
    let mut keep = true;
    for line in diff.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.trim_end().rsplit_once(" b/").map_or(rest.trim_end(), |(_, b)| b);
            keep = !excluded(path, patterns);
        }
        if keep {
            out.push_str(line);
        }
    }
    out
}

fn colorize(diff: &str) -> String {
    diff.split_inclusive('\n')
        .map(|line| {
            let code = if line.starts_with("diff --git") || line.starts_with("+++") || line.starts_with("---") || line.starts_with("index ") {
                "1"
            } else if line.starts_with("@@") {
                "36"
            } else if line.starts_with('+') {
                "32"
            } else if line.starts_with('-') {
                "31"
            } else {
                return line.to_string();
            };
            let (text, nl) = line.strip_suffix('\n').map_or((line, ""), |t| (t, "\n"));
            format!("\x1b[{code}m{text}\x1b[m{nl}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_arg() {
        assert_eq!(parse_pr("12").unwrap(), PrRef::Id(None, 12));
        assert_eq!(parse_pr("#7").unwrap(), PrRef::Id(None, 7));
        let repo = Repo { workspace: "ws".into(), slug: "r".into() };
        assert_eq!(parse_pr("https://bitbucket.org/ws/r/pull-requests/34").unwrap(), PrRef::Id(Some(repo.clone()), 34));
        assert_eq!(parse_pr("https://bitbucket.org/ws/r/pull-requests/34/diff?x=1").unwrap(), PrRef::Id(Some(repo), 34));
        assert_eq!(parse_pr("feature/abc").unwrap(), PrRef::Branch("feature/abc".into()));
        assert!(parse_pr("https://bitbucket.org/ws/r/pull-requests/x").is_err());
        assert!(parse_pr("https://bitbucket.org/ws/r").is_err());
    }

    #[test]
    fn branch_lookup() {
        assert_eq!(
            branch_query("feat/x"),
            "q=source.branch.name%3D%22feat%2Fx%22&state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&sort=-updated_on&pagelen=10"
        );
        let prs = [json!({"id": 3, "state": "MERGED"}), json!({"id": 2, "state": "OPEN"})];
        assert_eq!(pick_pr(&prs), Some(2));
        assert_eq!(pick_pr(&prs[..1]), Some(3));
        assert_eq!(pick_pr(&[]), None);
    }

    #[test]
    fn state_values() {
        assert_eq!(states("closed"), ["MERGED", "DECLINED", "SUPERSEDED"]);
        assert_eq!(states("declined"), ["DECLINED"]);
        assert_eq!(states("open"), ["OPEN"]);
        assert_eq!(web_state("merged"), "MERGED");
        assert_eq!(web_state("closed"), "ALL");
    }

    fn filter(state: &str) -> Filter<'_> {
        Filter { state, author: None, base: None, head: None, search: None, draft: false }
    }

    #[test]
    fn query() {
        assert_eq!(list_query(&filter("open"), 30), "state=OPEN&pagelen=30");
        assert_eq!(list_query(&filter("closed"), 30), "state=MERGED&state=DECLINED&state=SUPERSEDED&pagelen=30");
        let f = Filter { author: Some("bob".into()), base: Some("main".into()), ..filter("all") };
        assert_eq!(
            list_query(&f, 100),
            "state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&q=author.username%3D%22bob%22%20AND%20destination.branch.name%3D%22main%22&pagelen=50"
        );
        let f = Filter { search: Some("fix \"x\"".into()), draft: true, ..filter("open") };
        assert_eq!(
            list_query(&f, 10),
            format!("state=OPEN&q={}&pagelen=10", enc("(title ~ \"fix \\\"x\\\"\" OR description ~ \"fix \\\"x\\\"\") AND draft=true"))
        );
    }

    #[test]
    fn fill() {
        let one = [("Add x".to_string(), "Because y".to_string())];
        let two = [("Add x".to_string(), "Because y".to_string()), ("Fix z".to_string(), String::new())];
        assert_eq!(fill_text(Fill::Default, &one, "feat/add-x"), ("Add x".into(), "Because y".into()));
        assert_eq!(fill_text(Fill::Default, &two, "feat/add-x"), ("Add x".into(), "- Add x\n- Fix z".into()));
        assert_eq!(fill_text(Fill::Default, &two, "feat/new_thing"), ("New thing".into(), "- Add x\n- Fix z".into()));
        assert_eq!(fill_text(Fill::First, &two, "b"), ("Add x".into(), "Because y".into()));
        assert_eq!(fill_text(Fill::Verbose, &two, "my-branch"), ("My branch".into(), "Add x\n\nBecause y\n\nFix z".into()));
        assert_eq!(fill_text(Fill::Default, &[], "b"), ("B".into(), String::new()));
    }

    #[test]
    fn bodies() {
        let v = create_body("t", "d", "feat", &None, true, false, &["{u-1}".into(), "acc".into()]);
        assert_eq!(v["source"]["branch"]["name"], "feat");
        assert!(v.get("destination").is_none());
        assert_eq!(v["reviewers"], json!([{"uuid": "{u-1}"}, {"account_id": "acc"}]));
        let v = create_body("t", "d", "feat", &Some("dev".into()), false, true, &[]);
        assert_eq!(v["destination"]["branch"]["name"], "dev");
        assert_eq!(v["draft"], true);

        assert_eq!(merge_body(None, &None, false), json!({}));
        assert_eq!(
            merge_body(Some("squash"), &Some("m".into()), true),
            json!({"merge_strategy": "squash", "message": "m", "close_source_branch": true})
        );
        assert_eq!(commit_message(Some("s".into()), Some("b".into())), Some("s\n\nb".into()));
        assert_eq!(commit_message(None, Some("b".into())), Some("b".into()));
        assert_eq!(commit_message(None, None), None);

        assert_eq!(comment_body("hi", &None, None), json!({"content": {"raw": "hi"}}));
        assert_eq!(
            comment_body("hi", &Some("a.rs".into()), Some(3)),
            json!({"content": {"raw": "hi"}, "inline": {"path": "a.rs", "to": 3}})
        );
        assert_eq!(enc_path("feature/a b"), "feature/a%20b");
    }

    #[test]
    fn checks() {
        let list = [json!({"state": "SUCCESSFUL"}), json!({"state": "FAILED"}), json!({"state": "STOPPED"}), json!({"state": "INPROGRESS"})];
        assert_eq!(tally(&list), (2, 1));
        assert_eq!(summary(4, 2, 1), "2 failed, 1 passed, 1 pending");
    }

    #[test]
    fn diff_filters() {
        let pats = ["*.lock".to_string(), "docs/*".to_string()];
        assert!(excluded("Cargo.lock", &pats));
        assert!(excluded("sub/dir/yarn.lock", &pats));
        assert!(excluded("docs/a.md", &pats));
        assert!(!excluded("docs/deep/a.md", &pats));
        assert!(!excluded("src/main.rs", &pats));
        assert!(glob(b"a?c", b"abc") && !glob(b"a?c", b"a/c"));

        let diff = "diff --git a/Cargo.lock b/Cargo.lock\n-x\n+y\ndiff --git a/src/a.rs b/src/a.rs\n+z\n";
        assert_eq!(exclude_files(diff, &pats), "diff --git a/src/a.rs b/src/a.rs\n+z\n");
        assert_eq!(exclude_files(diff, &[]), diff);
        assert_eq!(colorize("+a\n-b\n c\n"), "\x1b[32m+a\x1b[m\n\x1b[31m-b\x1b[m\n c\n");
    }
}
