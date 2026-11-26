//! `bb project`: list, view, create, edit, delete Bitbucket projects (a workspace's groups of
//! repositories). Same verbs and flags as `gh project`, where they mean the same thing.
use crate::client::{Client, enc, limit_arg};
use crate::context::Repo;
use crate::output::{json_mode, more_note, open_browser, print_json, s, short_time, table};
use anyhow::{Result, anyhow, bail};
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub enum Cmd {
    /// List the projects in a workspace (default: the current repo's workspace)
    #[command(visible_alias = "ls")]
    List {
        /// Workspace to list (default: the current repo's workspace)
        #[arg(long, value_name = "WORKSPACE")]
        owner: Option<String>,
        /// Maximum number of projects to list [default: 30; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
        /// Open the workspace's projects page in the browser
        #[arg(short, long)]
        web: bool,
    },
    /// Show a project
    View {
        /// Project key (e.g. PROJ)
        key: String,
        #[arg(long, value_name = "WORKSPACE")]
        owner: Option<String>,
        /// Open in the browser
        #[arg(short, long)]
        web: bool,
    },
    /// Create a project (private by default)
    Create {
        /// Project key: letters, numbers and underscores (e.g. PROJ)
        key: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        title: String,
        #[arg(long, value_name = "WORKSPACE")]
        owner: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        /// Project visibility [default: private]
        #[arg(long, value_parser = visibility)]
        visibility: Option<bool>,
    },
    /// Edit a project's name, description or visibility
    #[command(group(clap::ArgGroup::new("change").args(["title", "description", "visibility"]).required(true).multiple(true)))]
    Edit {
        /// Project key
        key: String,
        #[arg(long, value_name = "WORKSPACE")]
        owner: Option<String>,
        /// New project name
        #[arg(long, value_name = "NAME")]
        title: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New visibility
        #[arg(long, value_parser = visibility)]
        visibility: Option<bool>,
    },
    /// Delete a project (Bitbucket only deletes empty projects)
    Delete {
        /// Project key
        key: String,
        #[arg(long, value_name = "WORKSPACE")]
        owner: Option<String>,
        /// Confirm deletion
        #[arg(long)]
        yes: bool,
    },
}

/// `public`/`private` in any case (`gh project` uses PUBLIC/PRIVATE, `gh repo` lowercase) -> is_private.
pub fn visibility(v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "private" => Ok(true),
        "public" => Ok(false),
        _ => Err("expected public or private".into()),
    }
}

/// `--owner`, else the workspace of the current repo (or of -R).
fn workspace(owner: Option<String>) -> Result<String> {
    match owner {
        Some(ws) => Ok(ws),
        None => Repo::current().map(|r| r.workspace).map_err(|_| {
            anyhow!("specify a workspace: --owner WORKSPACE (or run inside a Bitbucket repo, or pass -R)")
        }),
    }
}

fn api(ws: &str, key: &str) -> String {
    format!("/workspaces/{}/projects/{}", enc(ws), enc(key))
}

fn print_url_or_json(v: &Value) {
    if json_mode() {
        print_json(v);
    } else {
        println!("{}", s(v, "links.html.href"));
    }
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { owner, limit, web } => {
            let ws = workspace(owner)?;
            if web {
                return open_browser(&format!("https://bitbucket.org/{ws}/workspace/projects/"));
            }
            let (limit, chosen) = limit_arg(limit, 30);
            let path = format!("/workspaces/{}/projects?sort=key&pagelen={}", enc(&ws), limit.clamp(1, 100));
            let list = Client::new()?.list(&path, limit)?;
            if json_mode() {
                print_json(&Value::Array(list.items));
                return Ok(());
            }
            let mut rows = vec![vec!["KEY".into(), "NAME".into(), "VISIBILITY".into(), "UPDATED".into()]];
            for p in &list.items {
                rows.push(vec![s(p, "key"), s(p, "name"), vis(p).into(), short_time(&s(p, "updated_on"))]);
            }
            table(&rows);
            if !chosen {
                more_note(&list, "projects", "sorted by key");
            }
        }
        Cmd::View { key, owner, web } => {
            let ws = workspace(owner)?;
            if web {
                return open_browser(&format!("https://bitbucket.org/{ws}/workspace/projects/{key}"));
            }
            let v = Client::new()?.get(&api(&ws, &key))?;
            if json_mode() {
                print_json(&v);
                return Ok(());
            }
            table(&[
                vec!["key:".into(), s(&v, "key")],
                vec!["name:".into(), s(&v, "name")],
                vec!["description:".into(), s(&v, "description")],
                vec!["visibility:".into(), vis(&v).into()],
                vec!["updated:".into(), short_time(&s(&v, "updated_on"))],
                vec!["url:".into(), s(&v, "links.html.href")],
            ]);
        }
        Cmd::Create { key, title, owner, description, visibility } => {
            let ws = workspace(owner)?;
            let mut body = json!({"key": key, "name": title, "is_private": visibility.unwrap_or(true)});
            if let Some(d) = description {
                body["description"] = json!(d);
            }
            print_url_or_json(&Client::new()?.post(&format!("/workspaces/{}/projects", enc(&ws)), Some(&body))?);
        }
        Cmd::Edit { key, owner, title, description, visibility } => {
            let ws = workspace(owner)?;
            let client = Client::new()?;
            // PUT creates the project when the key doesn't exist, so read it first: a mistyped key
            // must fail, not make a new project. The PUT then sends every field, changed or not.
            let cur = client.get(&api(&ws, &key))?;
            let body = json!({
                "key": cur["key"],
                "name": title.map_or(cur["name"].clone(), Value::from),
                "description": description.map_or(cur["description"].clone(), Value::from),
                "is_private": visibility.map_or(cur["is_private"].clone(), Value::from),
            });
            print_url_or_json(&client.put(&api(&ws, &key), &body)?);
        }
        Cmd::Delete { key, owner, yes } => {
            let ws = workspace(owner)?;
            if !yes {
                bail!("refusing to delete project {key} in {ws} without --yes");
            }
            Client::new()?.delete(&api(&ws, &key))?;
            println!("Deleted project {key} in {ws}");
        }
    }
    Ok(())
}

fn vis(v: &Value) -> &'static str {
    if v["is_private"].as_bool() == Some(false) { "public" } else { "private" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_values() {
        assert_eq!(visibility("PUBLIC"), Ok(false));
        assert_eq!(visibility("private"), Ok(true));
        assert!(visibility("internal").is_err());
    }
}
