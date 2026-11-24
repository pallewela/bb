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

    /// Accepts `ws/repo`, `git@[ssh.]bitbucket.org:ws/repo.git`, `https://[user@]bitbucket.org/ws/repo[.git][/...]`.
    /// A URL's query or fragment is ignored. Both parts must be Bitbucket-style names (ASCII
    /// letters, digits, `-`, `_`, `.`): they go into API paths and URLs unescaped.
    pub fn parse(s: &str) -> Result<Repo> {
        let s = s.trim();
        let path = match s.split_once("bitbucket.org") {
            Some((_, rest)) if rest.starts_with([':', '/']) => rest.trim_start_matches([':', '/']),
            _ if !s.contains(':') => s,
            _ => bail!("cannot parse repository `{s}`"),
        };
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let mut parts = path.split('/');
        let name = |p: &str| {
            !matches!(p, "" | "." | "..") && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        match (parts.next(), parts.next().map(|p| p.trim_end_matches(".git"))) {
            (Some(ws), Some(slug)) if name(ws) && name(slug) => Ok(Repo { workspace: ws.to_string(), slug: slug.to_string() }),
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
        format!("git@ssh.bitbucket.org:{}/{}.git", self.workspace, self.slug)
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
            "git@ssh.bitbucket.org:ws/repo.git",
            "ssh://git@bitbucket.org/ws/repo.git",
            "https://user@bitbucket.org/ws/repo.git",
            "https://bitbucket.org/ws/repo",
            "https://bitbucket.org/ws/repo/pull-requests/12",
            "https://bitbucket.org/ws/repo?at=main",
            "https://bitbucket.org/ws/repo#readme",
        ] {
            assert_eq!(Repo::parse(s).unwrap(), want, "{s}");
        }
        let odd = Repo { workspace: "my_ws-1".into(), slug: "bitbucket.org".into() };
        assert_eq!(Repo::parse("my_ws-1/bitbucket.org").unwrap(), odd);
        assert_eq!(Repo::parse("https://bitbucket.org/my_ws-1/bitbucket.org").unwrap(), odd);
        // Found by fuzz/fuzz_targets/repo_parse.rs: these used to parse to a bad or empty slug.
        for s in [
            "repo",
            "git@github.com:ws/repo.git",
            "",
            "ws/",
            "ws/.git",
            "ws/..",
            "git@ssh.bitbucket.org:ws/repo.gi\0t",
            "ws/re po",
            "ws/repo\u{301}",
            "bitbucket.org.evil:ws/repo",
        ] {
            assert!(Repo::parse(s).is_err(), "{s}");
        }
    }
}
