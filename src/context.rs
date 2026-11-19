//! Which repository are we talking about, plus small git helpers.
use anyhow::{anyhow, bail, Context, Result};
use std::{process::Command, sync::OnceLock};

static REPO_FLAG: OnceLock<Option<String>> = OnceLock::new();

pub fn set_repo_flag(flag: Option<String>) {
    let _ = REPO_FLAG.set(flag);
}

#[derive(Debug, PartialEq, Clone)]
pub struct Repo {
    pub workspace: String,
    pub slug: String,
}

impl Repo {
    /// From the global -R flag, else the `origin` remote of the current directory.
    pub fn current() -> Result<Repo> {
        if let Some(Some(flag)) = REPO_FLAG.get() {
            return Repo::parse(flag);
        }
        let remote = git(&["remote", "get-url", "origin"])
            .map_err(|_| anyhow!("not in a git repo with an `origin` remote; use -R WORKSPACE/REPO"))?;
        Repo::parse(&remote).context("`origin` is not a Bitbucket remote; use -R WORKSPACE/REPO")
    }

    /// Accepts `ws/repo`, `git@bitbucket.org:ws/repo.git`, `https://[user@]bitbucket.org/ws/repo[.git][/...]`.
    pub fn parse(s: &str) -> Result<Repo> {
        let s = s.trim();
        let path = match s.split_once("bitbucket.org") {
            Some((_, rest)) => rest.trim_start_matches([':', '/']),
            None if !s.contains(':') => s,
            None => bail!("cannot parse repository `{s}`"),
        };
        let mut parts = path.split('/');
        match (parts.next(), parts.next()) {
            (Some(ws), Some(slug)) if !ws.is_empty() && !slug.is_empty() => Ok(Repo {
                workspace: ws.to_string(),
                slug: slug.trim_end_matches(".git").to_string(),
            }),
            _ => bail!("cannot parse repository `{s}` (expected WORKSPACE/REPO)"),
        }
    }

    /// API path prefix: `/repositories/{workspace}/{slug}`
    pub fn api(&self) -> String {
        format!("/repositories/{}/{}", self.workspace, self.slug)
    }

    /// Web URL: `https://bitbucket.org/{workspace}/{slug}`
    pub fn web(&self) -> String {
        format!("https://bitbucket.org/{}/{}", self.workspace, self.slug)
    }

    pub fn ssh_url(&self) -> String {
        format!("git@bitbucket.org:{}/{}.git", self.workspace, self.slug)
    }
}

/// Run git and capture trimmed stdout. Fails (with git's stderr) on non-zero exit.
pub fn git(args: &[&str]) -> Result<String> {
    let out = Command::new("git").args(args).output().context("failed to run git")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run git with inherited stdio (clone, fetch, checkout: user sees progress).
pub fn git_passthrough(args: &[&str]) -> Result<()> {
    let status = Command::new("git").args(args).status().context("failed to run git")?;
    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

pub fn current_branch() -> Result<String> {
    let b = git(&["branch", "--show-current"])?;
    if b.is_empty() {
        bail!("not on a branch (detached HEAD)");
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_forms() {
        let want = Repo { workspace: "ws".into(), slug: "repo".into() };
        for s in [
            "ws/repo",
            "git@bitbucket.org:ws/repo.git",
            "ssh://git@bitbucket.org/ws/repo.git",
            "https://user@bitbucket.org/ws/repo.git",
            "https://bitbucket.org/ws/repo",
            "https://bitbucket.org/ws/repo/pull-requests/12",
        ] {
            assert_eq!(Repo::parse(s).unwrap(), want, "{s}");
        }
        for s in ["repo", "git@github.com:ws/repo.git", "", "ws/"] {
            assert!(Repo::parse(s).is_err(), "{s}");
        }
    }
}
