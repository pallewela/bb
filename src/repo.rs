//! `bb repo`: list, view, clone, create, delete.
use crate::client::{enc, limit_or_all, Client};
use crate::context::{git_passthrough, Repo};
use crate::output::{json_mode, more_note, open_browser, print_json, s, short_time, table};
use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum Cmd {
    /// List repositories in a workspace (default: the current repo's workspace)
    List {
        workspace: Option<String>,
        /// Maximum number of repositories to list (0 = all)
        #[arg(short = 'L', long, default_value_t = 30)]
        limit: usize,
    },
    /// Show a repository
    View {
        /// WORKSPACE/REPO or URL (default: current repo)
        repo: Option<String>,
        /// Open in the browser
        #[arg(long)]
        web: bool,
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
        #[arg(long)]
        clone: bool,
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

fn list_path(workspace: &str, limit: usize) -> String {
    format!("/repositories/{}?sort=-updated_on&pagelen={}", enc(workspace), limit.clamp(1, 100))
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
        Cmd::List { workspace, limit } => {
            // Bitbucket removed the cross-workspace listing (GET /repositories?role=member now 404s).
            let ws = match workspace {
                Some(ws) => ws,
                None => Repo::current().map(|r| r.workspace).map_err(|_| {
                    anyhow::anyhow!("specify a workspace: `bb repo list WORKSPACE` (or run inside a Bitbucket repo)")
                })?,
            };
            let limit = limit_or_all(limit);
            let list = Client::new()?.list(&list_path(&ws, limit), limit)?;
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
            more_note(&list, "repositories", "most recently updated first");
        }
        Cmd::View { repo, web } => {
            let repo = resolve(repo)?;
            if web {
                return open_browser(&repo.web());
            }
            let v = Client::new()?.get(&repo.api())?;
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
            rows.push(vec!["url:".into(), s(&v, "links.html.href")]);
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
        assert_eq!(list_path("ws", 5), "/repositories/ws?sort=-updated_on&pagelen=5");
        assert_eq!(list_path("ws", 500), "/repositories/ws?sort=-updated_on&pagelen=100");
    }
}
