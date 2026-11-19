//! `bb workspace`: list the workspaces the caller can access.
use crate::client::Client;
use crate::output::{json_mode, print_json, s, table};
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;

#[derive(Subcommand)]
pub enum Cmd {
    /// List workspaces you can access
    List {
        /// Only workspaces you administer
        #[arg(long)]
        admin: bool,
        /// Maximum number of workspaces to list
        #[arg(short = 'L', long, default_value_t = 100)]
        limit: usize,
    },
}

fn list_path(admin: bool, limit: usize) -> String {
    let admin = if admin { "&administrator=true" } else { "" };
    format!("/user/workspaces?sort=slug&pagelen={}{admin}", limit.clamp(1, 100))
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::List { admin, limit } => {
            let list = Client::new()?.paginate(&list_path(admin, limit), limit).map_err(|e| {
                if e.to_string().starts_with("HTTP 403") {
                    e.context("listing workspaces needs the read:workspace:bitbucket scope; create a token with it and run `bb auth login`")
                } else {
                    e
                }
            })?;
            if json_mode() {
                print_json(&Value::Array(list));
                return Ok(());
            }
            let mut rows = vec![vec!["SLUG".into(), "ROLE".into()]];
            for w in &list {
                let role = if w["administrator"].as_bool() == Some(true) { "admin" } else { "member" };
                rows.push(vec![s(w, "workspace.slug"), role.into()]);
            }
            table(&rows);
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
