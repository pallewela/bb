//! `bb repo`: list, view, clone, create, edit, delete.
use crate::client::{Client, enc, limit_arg};
use crate::context::{Repo, git_passthrough};
use crate::output::{json_mode, more_note, open_browser, print_json, s, short_time, table};
use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub enum Cmd {
    /// List repositories in a workspace (default: the current repo's workspace)
    #[command(visible_alias = "ls")]
    List {
        workspace: Option<String>,
        /// Maximum number of repositories to list [default: 30; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
        /// Filter by visibility
        #[arg(long, value_parser = ["public", "private"])]
        visibility: Option<String>,
        /// Filter by primary language
        #[arg(short, long)]
        language: Option<String>,
        /// Show only forks
        #[arg(long, conflicts_with = "source")]
        fork: bool,
        /// Show only non-forks
        #[arg(long)]
        source: bool,
        /// Only repositories in this project (key)
        #[arg(short, long, value_name = "KEY")]
        project: Option<String>,
    },
    /// Show a repository
    View {
        /// WORKSPACE/REPO or URL (default: current repo)
        repo: Option<String>,
        /// Open in the browser
        #[arg(short, long)]
        web: bool,
        /// Show this branch's latest commit (with --web: open the branch)
        #[arg(short, long)]
        branch: Option<String>,
    },
    /// Clone a repository over SSH
    Clone {
        /// WORKSPACE/REPO or URL
        repo: String,
        dir: Option<String>,
        /// Extra arguments passed to `git clone` (after `--`)
        #[arg(last = true)]
        git_args: Vec<String>,
    },
    /// Create a repository (private by default)
    #[command(visible_alias = "new")]
    Create {
        /// WORKSPACE/REPO
        repo: String,
        /// Make the repository private (default)
        #[arg(long, conflicts_with = "public")]
        private: bool,
        /// Make the repository public
        #[arg(long)]
        public: bool,
        #[arg(short, long)]
        description: Option<String>,
        /// Project key to create the repository in
        #[arg(short, long)]
        project: Option<String>,
        /// Clone the new repository afterwards
        #[arg(short, long)]
        clone: bool,
    },
    /// Edit a repository's description, visibility or project
    #[command(group(clap::ArgGroup::new("change").args(["description", "visibility", "project"]).required(true).multiple(true)))]
    Edit {
        /// WORKSPACE/REPO or URL (default: current repo)
        repo: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        /// New visibility (needs --accept-visibility-change-consequences)
        #[arg(long, value_parser = crate::project::visibility, requires = "accept_visibility_change_consequences")]
        visibility: Option<bool>,
        /// Confirm a visibility change
        #[arg(long)]
        accept_visibility_change_consequences: bool,
        /// Move the repository to this project (key)
        #[arg(short, long, value_name = "KEY")]
        project: Option<String>,
    },
    /// Delete a repository
    Delete {
        /// WORKSPACE/REPO or URL (default: current repo)
        repo: Option<String>,
        /// Confirm deletion
        #[arg(long)]
        yes: bool,
    },
}

fn resolve(repo: Option<String>) -> Result<Repo> {
    repo.map_or_else(Repo::current, |r| Repo::parse(&r))
}

fn list_path(workspace: &str, limit: usize, q: Option<&str>) -> String {
    let q = q.map(|q| format!("&q={}", enc(q))).unwrap_or_default();
    format!("/repositories/{}?sort=-updated_on&pagelen={}{q}", enc(workspace), limit.clamp(1, 100))
}

/// Bitbucket `q` filter for the `repo list` flags, or None when none is given.
/// Bitbucket stores languages in lowercase and compares them case-sensitively.
fn list_query(
    visibility: Option<&str>,
    language: Option<&str>,
    fork: bool,
    source: bool,
    project: Option<&str>,
) -> Option<String> {
    let quote = |v: &str| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""));
    let mut q = Vec::new();
    if let Some(v) = visibility {
        q.push(format!("is_private = {}", v == "private"));
    }
    if let Some(l) = language {
        q.push(format!("language = {}", quote(&l.to_lowercase())));
    }
    if fork {
        q.push("parent != null".into());
    }
    if source {
        q.push("parent = null".into());
    }
    if let Some(k) = project {
        q.push(format!("project.key = {}", quote(k)));
    }
    (!q.is_empty()).then(|| q.join(" AND "))
}

fn clone(repo: &Repo, dir: Option<&str>, extra: &[String]) -> Result<()> {
    let url = repo.ssh_url();
    let mut args = vec!["clone"];
    args.extend(extra.iter().map(String::as_str));
    args.push(&url);
    args.extend(dir);
    git_passthrough(&args)
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { workspace, limit, visibility, language, fork, source, project } => {
            // Bitbucket removed the cross-workspace listing (GET /repositories?role=member now 404s).
            let ws = match workspace {
                Some(ws) => ws,
                None => Repo::current().map(|r| r.workspace).map_err(|_| {
                    anyhow::anyhow!("specify a workspace: `bb repo list WORKSPACE` (or run inside a Bitbucket repo)")
                })?,
            };
            let (limit, chosen) = limit_arg(limit, 30);
            let q = list_query(visibility.as_deref(), language.as_deref(), fork, source, project.as_deref());
            let list = Client::new()?.list(&list_path(&ws, limit, q.as_deref()), limit)?;
            if json_mode() {
                print_json(&Value::Array(list.items));
                return Ok(());
            }
            let mut rows = vec![vec!["NAME".into(), "VISIBILITY".into(), "LANGUAGE".into(), "UPDATED".into()]];
            for r in &list.items {
                let vis = if r["is_private"].as_bool() == Some(false) { "public" } else { "private" };
                rows.push(vec![s(r, "full_name"), vis.into(), s(r, "language"), short_time(&s(r, "updated_on"))]);
            }
            table(&rows);
            if !chosen {
                more_note(&list, "repositories", "most recently updated first");
            }
        }
        Cmd::View { repo, web, branch } => {
            let repo = resolve(repo)?;
            if web {
                return open_browser(&branch.map_or_else(|| repo.web(), |b| format!("{}/src/{b}/", repo.web())));
            }
            let client = Client::new()?;
            let v = client.get(&repo.api())?;
            if json_mode() {
                print_json(&v);
                return Ok(());
            }
            let vis = if v["is_private"].as_bool() == Some(false) { "public" } else { "private" };
            let mut rows = vec![
                vec!["name:".into(), s(&v, "full_name")],
                vec!["description:".into(), s(&v, "description")],
                vec!["main branch:".into(), s(&v, "mainbranch.name")],
                vec!["visibility:".into(), vis.into()],
            ];
            for c in v["links"]["clone"].as_array().into_iter().flatten() {
                rows.push(vec![format!("clone ({}):", s(c, "name")), s(c, "href")]);
            }
            match branch {
                Some(b) => {
                    let b = client.get(&format!("{}/refs/branches/{}", repo.api(), enc(&b)))?;
                    let hash: String = s(&b, "target.hash").chars().take(12).collect();
                    let msg = s(&b, "target.message");
                    let msg = msg.lines().next().unwrap_or("");
                    rows.push(vec!["branch:".into(), s(&b, "name")]);
                    rows.push(vec!["commit:".into(), format!("{hash} {} {msg}", short_time(&s(&b, "target.date")))]);
                    rows.push(vec!["url:".into(), s(&b, "links.html.href")]);
                }
                None => rows.push(vec!["url:".into(), s(&v, "links.html.href")]),
            }
            table(&rows);
        }
        Cmd::Clone { repo, dir, git_args } => clone(&Repo::parse(&repo)?, dir.as_deref(), &git_args)?,
        Cmd::Create { repo, private: _, public, description, project, clone: and_clone } => {
            let repo = Repo::parse(&repo)?;
            let mut body = json!({"scm": "git", "is_private": !public});
            if let Some(d) = description {
                body["description"] = json!(d);
            }
            if let Some(k) = project {
                body["project"] = json!({ "key": k });
            }
            let v = Client::new()?.post(&repo.api(), Some(&body))?;
            if json_mode() {
                print_json(&v);
            } else {
                println!("{}", s(&v, "links.html.href"));
            }
            if and_clone {
                clone(&repo, None, &[])?;
            }
        }
        Cmd::Edit { repo, description, visibility, accept_visibility_change_consequences: _, project } => {
            let repo = resolve(repo)?;
            let client = Client::new()?;
            // PUT creates the repository when it doesn't exist, so check first: a mistyped name
            // must fail, not make a new repository. Only the given fields are sent.
            client.get(&format!("{}?fields=full_name", repo.api()))?;
            let mut body = json!({});
            if let Some(d) = description {
                body["description"] = json!(d);
            }
            if let Some(p) = visibility {
                body["is_private"] = json!(p);
            }
            if let Some(k) = project {
                body["project"] = json!({ "key": k });
            }
            let v = client.put(&repo.api(), &body)?;
            if json_mode() {
                print_json(&v);
            } else {
                println!("Edited repository {}", s(&v, "full_name"));
            }
        }
        Cmd::Delete { repo, yes } => {
            let repo = resolve(repo)?;
            if !yes {
                bail!("refusing to delete {}/{} without --yes", repo.workspace, repo.slug);
            }
            Client::new()?.delete(&repo.api())?;
            println!("Deleted {}/{}", repo.workspace, repo.slug);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_paths() {
        assert_eq!(list_path("ws", 5, None), "/repositories/ws?sort=-updated_on&pagelen=5");
        assert_eq!(list_path("ws", 500, None), "/repositories/ws?sort=-updated_on&pagelen=100");
        assert_eq!(
            list_path("ws", 5, Some("parent = null")),
            "/repositories/ws?sort=-updated_on&pagelen=5&q=parent%20%3D%20null"
        );
    }

    #[test]
    fn list_queries() {
        assert_eq!(list_query(None, None, false, false, None), None);
        assert_eq!(list_query(Some("public"), None, false, false, None).unwrap(), "is_private = false");
        assert_eq!(
            list_query(Some("private"), Some("Go"), true, false, None).unwrap(),
            "is_private = true AND language = \"go\" AND parent != null"
        );
        assert_eq!(list_query(None, Some("C#"), false, true, None).unwrap(), "language = \"c#\" AND parent = null");
        assert_eq!(list_query(None, Some("a\"b"), false, false, None).unwrap(), "language = \"a\\\"b\"");
        assert_eq!(list_query(None, None, false, false, Some("NGPG")).unwrap(), "project.key = \"NGPG\"");
    }
}
