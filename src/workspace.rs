//! `bb workspace`: list the workspaces the caller can access.
use crate::client::{Client, limit_arg};
use crate::output::{json_mode, more_note, print_json, s, table};
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;

#[derive(Subcommand)]
pub enum Cmd {
    /// List workspaces you can access
    #[command(visible_alias = "ls")]
    List {
        /// Only workspaces you administer
        #[arg(long)]
        admin: bool,
        /// Maximum number of workspaces to list [default: 30; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
    },
}

fn list_path(admin: bool, limit: usize) -> String {
    let admin = if admin { "&administrator=true" } else { "" };
    format!("/user/workspaces?sort=slug&pagelen={}{admin}", limit.clamp(1, 100))
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { admin, limit } => {
            let (limit, chosen) = limit_arg(limit, 30);
            let list = Client::new()?.list(&list_path(admin, limit), limit).map_err(|e| {
                if e.to_string().starts_with("HTTP 403") {
                    e.context("listing workspaces needs the read:workspace:bitbucket scope; create a token with it and run `bb auth login`")
                } else {
                    e
                }
            })?;
            if json_mode() {
                print_json(&Value::Array(list.items));
                return Ok(());
            }
            let mut rows = vec![vec!["SLUG".into(), "ROLE".into()]];
            for w in &list.items {
                let role = if w["administrator"].as_bool() == Some(true) { "admin" } else { "member" };
                rows.push(vec![s(w, "workspace.slug"), role.into()]);
            }
            table(&rows);
            if !chosen {
                more_note(&list, "workspaces", "sorted by slug");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_paths() {
        assert_eq!(list_path(false, 30), "/user/workspaces?sort=slug&pagelen=30");
        assert_eq!(list_path(true, 500), "/user/workspaces?sort=slug&pagelen=100&administrator=true");
    }
}
