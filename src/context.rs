//! Which repository are we talking about, plus small git helpers.
use anyhow::{Context, Result, anyhow, bail};
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

    /// Accepts `ws/repo`, `[user@][ssh.]bitbucket.org:ws/repo[.git]` (scp-style), and
    /// `scheme://[user@][ssh.]bitbucket.org[:port]/ws/repo[.git][/...][?...][#...]`.
    /// The host must be exactly `bitbucket.org` or `ssh.bitbucket.org` (any case).
    pub fn parse(s: &str) -> Result<Repo> {
        let s = s.trim();
        let bad = || anyhow!("cannot parse repository `{s}` (expected WORKSPACE/REPO or a Bitbucket URL)");
        // `extra_ok`: URLs may carry more path after ws/repo (e.g. `/pull-requests/12`).
        let (path, extra_ok) = if let Some((_, rest)) = s.split_once("://") {
            let rest = rest.split(['?', '#']).next().unwrap_or("");
            let (authority, path) = rest.split_once('/').ok_or_else(bad)?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            let host = host.split_once(':').map_or(host, |(h, _)| h);
            check_host(host).ok_or_else(bad)?;
            (path, true)
        } else if let Some((authority, path)) = s.split_once(':') {
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            check_host(host).ok_or_else(bad)?;
            (path.trim_start_matches('/'), false)
        } else {
            (s, false)
        };
        let path = path.trim_end_matches('/');
        let mut parts = path.split('/');
        let (ws, slug) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let slug = slug.strip_suffix(".git").unwrap_or(slug);
        if ws.is_empty() || slug.is_empty() || (!extra_ok && parts.next().is_some()) {
            return Err(bad());
        }
        Ok(Repo { workspace: ws.to_string(), slug: slug.to_string() })
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

fn check_host(host: &str) -> Option<()> {
    (host.eq_ignore_ascii_case("bitbucket.org") || host.eq_ignore_ascii_case("ssh.bitbucket.org")).then_some(())
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
            // whitespace (e.g. a trailing newline from git output)
            "  ws/repo\n",
            "\tgit@bitbucket.org:ws/repo.git ",
            // trailing slash, query and fragment
            "https://bitbucket.org/ws/repo/",
            "https://bitbucket.org/ws/repo.git/",
            "https://bitbucket.org/ws/repo?at=main",
            "https://bitbucket.org/ws/repo#readme",
            // host case, explicit port, scp path with a leading slash
            "https://BitBucket.ORG/ws/repo",
            "ssh://git@ssh.bitbucket.org:22/ws/repo.git",
            "git@bitbucket.org:/ws/repo.git",
            "bitbucket.org:ws/repo",
        ] {
            assert_eq!(Repo::parse(s).unwrap(), want, "{s}");
        }
    }

    #[test]
    fn parse_repo_keeps_unusual_names() {
        let r = Repo::parse("wørk-space/répo.name").unwrap();
        assert_eq!((r.workspace.as_str(), r.slug.as_str()), ("wørk-space", "répo.name"));
        // only one `.git` suffix is stripped
        assert_eq!(Repo::parse("ws/repo.git.git").unwrap().slug, "repo.git");
        assert_eq!(Repo::parse("ws/repo.github").unwrap().slug, "repo.github");
    }

    #[test]
    fn parse_repo_rejects() {
        for s in [
            "",
            "   ",
            "repo",
            "ws/",
            "/repo",
            "/",
            "//",
            "ws//repo",
            "ws/.git",
            "ws/repo/extra",
            "git@bitbucket.org:ws/repo/extra",
            "git@bitbucket.org:",
            "git@bitbucket.org:ws",
            "https://bitbucket.org",
            "https://bitbucket.org/",
            "https://bitbucket.org/ws",
            // other hosts, including ones that merely contain `bitbucket.org`
            "git@github.com:ws/repo.git",
            "https://github.com/ws/repo",
            "https://evil.example/bitbucket.org/ws/repo",
            "https://bitbucket.org.evil.example/ws/repo",
            "https://notbitbucket.org/ws/repo",
            "git@bitbucket.org-work:ws/repo.git",
            "https://bitbucket.org@evil.example/ws/repo",
        ] {
            assert!(Repo::parse(s).is_err(), "{s:?} should be rejected");
        }
    }
}
