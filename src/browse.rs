//! `bb browse`: open the repo, a branch, or a file/dir on bitbucket.org.
use crate::client::Client;
use crate::context::{current_branch, git, Repo};
use crate::output::{open_browser, s};
use anyhow::Result;

#[derive(clap::Args)]
pub struct Args {
    /// File or directory path within the repository (relative to its root)
    path: Option<String>,
    /// Branch to browse (default with PATH: current branch, else the main branch)
    #[arg(short, long)]
    branch: Option<String>,
    /// Print the URL instead of opening a browser
    #[arg(long)]
    no_browser: bool,
}

fn browse_url(web: &str, branch: Option<&str>, path: Option<&str>) -> String {
    let path = path.map(|p| p.trim_start_matches("./").trim_start_matches('/')).unwrap_or("");
    match branch {
        None => web.to_string(),
        Some(b) => format!("{web}/src/{b}/{path}"),
    }
}

pub fn run(args: Args) -> Result<()> {
    let repo = Repo::current()?;
    let branch = match (args.branch, &args.path) {
        (Some(b), _) => Some(b),
        (None, None) => None,
        (None, Some(_)) => {
            // Only trust the local branch when the checkout is the repo being browsed (-R may differ).
            let local = git(&["remote", "get-url", "origin"]).ok().and_then(|r| Repo::parse(&r).ok());
            match local.filter(|l| *l == repo).and_then(|_| current_branch().ok()) {
                Some(b) => Some(b),
                None => Some(s(&Client::new()?.get(&repo.api())?, "mainbranch.name")),
            }
        }
    };
    let url = browse_url(&repo.web(), branch.as_deref(), args.path.as_deref());
    if args.no_browser {
        println!("{url}");
        Ok(())
    } else {
        open_browser(&url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let w = "https://bitbucket.org/ws/r";
        assert_eq!(browse_url(w, None, None), w);
        assert_eq!(browse_url(w, Some("main"), None), "https://bitbucket.org/ws/r/src/main/");
        assert_eq!(browse_url(w, Some("feat/x"), Some("./src/a.rs")), "https://bitbucket.org/ws/r/src/feat/x/src/a.rs");
    }
}
