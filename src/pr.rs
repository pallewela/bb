//! `bb pr`: pull requests.
use crate::client::{enc, Client};
use crate::context::{self, Repo};
use crate::output::{self, s};
use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};

#[derive(Args)]
pub struct PrArg {
    /// PR number or URL (default: the open PR for the current branch)
    pr: Option<String>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// List pull requests
    List {
        #[arg(long, default_value = "open", value_parser = ["open", "merged", "declined", "all"])]
        state: String,
        /// Filter by author nickname
        #[arg(long)]
        author: Option<String>,
        /// Filter by destination branch
        #[arg(long)]
        base: Option<String>,
        /// Filter by source branch
        #[arg(long)]
        head: Option<String>,
        #[arg(short = 'L', long, default_value_t = 30)]
        limit: usize,
    },
    /// Show a pull request
    View {
        #[command(flatten)]
        p: PrArg,
        /// Also show comments
        #[arg(long)]
        comments: bool,
        /// Open in the browser
        #[arg(long)]
        web: bool,
    },
    /// Create a pull request
    Create {
        #[arg(short, long)]
        title: Option<String>,
        #[arg(short, long)]
        body: Option<String>,
        /// Destination branch (default: repo main branch)
        #[arg(short = 'B', long)]
        base: Option<String>,
        /// Source branch (default: current branch)
        #[arg(short = 'H', long)]
        head: Option<String>,
        #[arg(long)]
        draft: bool,
        /// Reviewer uuid (`{...}`) or account_id; repeatable
        #[arg(short, long)]
        reviewer: Vec<String>,
        #[arg(long)]
        close_source_branch: bool,
        /// Take title/body from the last commit
        #[arg(long)]
        fill: bool,
        /// Open the create page in the browser instead
        #[arg(long)]
        web: bool,
    },
    /// Check out a pull request's source branch
    Checkout { pr: String },
    /// Show a pull request's diff
    Diff(PrArg),
    /// Merge a pull request
    Merge {
        #[command(flatten)]
        p: PrArg,
        #[arg(long, group = "strategy")]
        merge: bool,
        #[arg(long, group = "strategy")]
        squash: bool,
        #[arg(long, group = "strategy")]
        fast_forward: bool,
        /// Merge commit message
        #[arg(short, long)]
        message: Option<String>,
        #[arg(long)]
        close_source_branch: bool,
    },
    /// Decline a pull request
    #[command(alias = "close")]
    Decline(PrArg),
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
        #[arg(short, long)]
        body: String,
        /// File path for an inline comment
        #[arg(long)]
        file: Option<String>,
        /// Line (new version) for an inline comment
        #[arg(long, requires = "file")]
        line: Option<u64>,
    },
    /// List comments on a pull request
    Comments(PrArg),
    /// Show build statuses for a pull request
    Checks(PrArg),
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { state, author, base, head, limit } => {
            let c = Client::new()?;
            let path = format!("{}/pullrequests?{}", Repo::current()?.api(), list_query(&state, &author, &base, &head, limit));
            let prs = c.paginate(&path, limit)?;
            if output::json_mode() {
                output::print_json(&Value::Array(prs));
            } else if prs.is_empty() {
                eprintln!("No pull requests found");
            } else {
                let mut rows = vec![["ID", "TITLE", "BRANCH", "AUTHOR", "STATE"].map(String::from).to_vec()];
                rows.extend(prs.iter().map(|p| {
                    vec![
                        format!("#{}", s(p, "id")),
                        s(p, "title"),
                        s(p, "source.branch.name"),
                        s(p, "author.display_name"),
                        s(p, "state"),
                    ]
                }));
                output::table(&rows);
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
        Cmd::Create { title, body, base, head, draft, reviewer, close_source_branch, fill, web } => {
            let repo = Repo::current()?;
            let head = match head {
                Some(h) => h,
                None => context::current_branch()?,
            };
            if web {
                return output::open_browser(&format!("{}/pull-requests/new?source={}", repo.web(), enc(&head)));
            }
            let title = match title {
                Some(t) => t,
                None if fill => context::git(&["log", "-1", "--pretty=%s"])?,
                None => bail!("--title is required (or use --fill)"),
            };
            let body = match body {
                Some(b) => b,
                None if fill => context::git(&["log", "-1", "--pretty=%b"])?,
                None => String::new(),
            };
            let req = create_body(&title, &body, &head, &base, close_source_branch, draft, &reviewer);
            let pr = Client::new()?.post(&format!("{}/pullrequests", repo.api()), Some(&req))?;
            if output::json_mode() {
                output::print_json(&pr);
            } else {
                println!("{}", s(&pr, "links.html.href"));
            }
        }
        Cmd::Checkout { pr } => {
            let (c, path, _) = target(&Some(pr))?;
            // ponytail: assumes the source branch lives in `origin`; fork PRs need the fork remote.
            let branch = s(&c.get(&path)?, "source.branch.name");
            context::git_passthrough(&["fetch", "origin", &branch])?;
            context::git_passthrough(&["checkout", &branch])?;
        }
        Cmd::Diff(p) => {
            let (c, path, _) = target(&p.pr)?;
            print!("{}", c.get_text(&format!("{path}/diff"))?);
        }
        Cmd::Merge { p, merge, squash, fast_forward, message, close_source_branch } => {
            let strategy = [(merge, "merge_commit"), (squash, "squash"), (fast_forward, "fast_forward")]
                .into_iter()
                .find_map(|(on, name)| on.then_some(name));
            let (c, path, id) = target(&p.pr)?;
            let v = c.post(&format!("{path}/merge"), Some(&merge_body(strategy, &message, close_source_branch)))?;
            done(&v, format!("Merged pull request #{id}"));
        }
        Cmd::Decline(p) => {
            let (c, path, id) = target(&p.pr)?;
            done(&c.post(&format!("{path}/decline"), None)?, format!("Declined pull request #{id}"));
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
        Cmd::Comment { p, body, file, line } => {
            let (c, path, id) = target(&p.pr)?;
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
        Cmd::Checks(p) => {
            let (c, path, _) = target(&p.pr)?;
            let list = c.paginate(&format!("{path}/statuses?pagelen=100"), usize::MAX)?;
            if output::json_mode() {
                output::print_json(&Value::Array(list.clone()));
            } else if list.is_empty() {
                eprintln!("No checks reported");
            } else {
                let mut rows = vec![["STATE", "NAME", "URL"].map(String::from).to_vec()];
                rows.extend(list.iter().map(|x| {
                    let name = Some(s(x, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| s(x, "key"));
                    vec![s(x, "state"), name, s(x, "url")]
                }));
                output::table(&rows);
            }
            let failed = list.iter().filter(|x| x["state"] == "FAILED").count();
            if failed > 0 {
                bail!("{failed} check(s) failed");
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

/// Numeric id (optionally `#12`), or a PR URL whose repo overrides the context.
fn parse_pr(arg: &str) -> Result<(Option<Repo>, u64)> {
    if let Ok(id) = arg.trim_start_matches('#').parse() {
        return Ok((None, id));
    }
    if let Some((_, rest)) = arg.split_once("/pull-requests/") {
        if let (Ok(id), Ok(repo)) = (rest.split(['/', '?', '#']).next().unwrap_or("").parse(), Repo::parse(arg)) {
            return Ok((Some(repo), id));
        }
    }
    bail!("invalid pull request `{arg}`: expected a number or a pull request URL")
}

/// Resolve `[PR]` to (client, `/repositories/{ws}/{slug}/pullrequests/{id}`, id).
fn target(arg: &Option<String>) -> Result<(Client, String, u64)> {
    let c = Client::new()?;
    let (repo, id) = match arg {
        Some(a) => match parse_pr(a)? {
            (Some(r), id) => (r, id),
            (None, id) => (Repo::current()?, id),
        },
        None => {
            let repo = Repo::current()?;
            let branch = context::current_branch()?;
            let q = enc(&format!("source.branch.name=\"{branch}\" AND state=\"OPEN\""));
            let page = c.get(&format!("{}/pullrequests?q={q}", repo.api()))?;
            match page["values"][0]["id"].as_u64() {
                Some(id) => (repo, id),
                None => bail!("no open pull request found for branch `{branch}`"),
            }
        }
    };
    let path = format!("{}/pullrequests/{id}", repo.api());
    Ok((c, path, id))
}

fn list_query(state: &str, author: &Option<String>, base: &Option<String>, head: &Option<String>, limit: usize) -> String {
    let states: &[&str] = match state {
        "merged" => &["MERGED"],
        "declined" => &["DECLINED"],
        "all" => &["OPEN", "MERGED", "DECLINED", "SUPERSEDED"],
        _ => &["OPEN"],
    };
    let mut params: Vec<String> = states.iter().map(|st| format!("state={st}")).collect();
    let q: Vec<String> = [("author.nickname", author), ("destination.branch.name", base), ("source.branch.name", head)]
        .into_iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}=\"{v}\"")))
        .collect();
    if !q.is_empty() {
        params.push(format!("q={}", enc(&q.join(" AND "))));
    }
    params.push(format!("pagelen={}", limit.clamp(1, 50)));
    params.join("&")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_arg() {
        assert_eq!(parse_pr("12").unwrap(), (None, 12));
        assert_eq!(parse_pr("#7").unwrap(), (None, 7));
        let repo = Repo { workspace: "ws".into(), slug: "r".into() };
        assert_eq!(parse_pr("https://bitbucket.org/ws/r/pull-requests/34").unwrap(), (Some(repo.clone()), 34));
        assert_eq!(parse_pr("https://bitbucket.org/ws/r/pull-requests/34/diff?x=1").unwrap(), (Some(repo), 34));
        assert!(parse_pr("abc").is_err());
        assert!(parse_pr("https://bitbucket.org/ws/r/pull-requests/x").is_err());
    }

    #[test]
    fn query() {
        assert_eq!(list_query("open", &None, &None, &None, 30), "state=OPEN&pagelen=30");
        assert_eq!(
            list_query("all", &Some("bob".into()), &Some("main".into()), &None, 100),
            "state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&q=author.nickname%3D%22bob%22%20AND%20destination.branch.name%3D%22main%22&pagelen=50"
        );
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

        assert_eq!(comment_body("hi", &None, None), json!({"content": {"raw": "hi"}}));
        assert_eq!(
            comment_body("hi", &Some("a.rs".into()), Some(3)),
            json!({"content": {"raw": "hi"}, "inline": {"path": "a.rs", "to": 3}})
        );
    }
}
