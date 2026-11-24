//! `bb browse`: open the repo, a PR, a commit, a branch, a file/dir, or a repo page on bitbucket.org.
use crate::client::{Client, enc};
use crate::context::{Repo, current_branch, git};
use crate::output::{open_browser, s};
use anyhow::{Result, bail};

#[derive(clap::Args)]
#[command(group(
    clap::ArgGroup::new("section").args(["settings", "actions", "wiki"]).conflicts_with_all(["target", "branch", "commit", "blame"])
))]
pub struct Args {
    /// A PR number, a commit SHA, or a file/dir path relative to the repo root (`PATH:LINE` or `PATH:START-END` selects lines)
    #[arg(value_name = "NUMBER|PATH|SHA")]
    target: Option<String>,
    /// Branch to browse (default with PATH: current branch, else the main branch)
    #[arg(short, long)]
    branch: Option<String>,
    /// Browse at a commit: `--commit=SHA`, or the last commit when no SHA is given
    #[arg(short, long, num_args = 0..=1, require_equals = true, default_missing_value = "last", value_name = "SHA", conflicts_with = "branch")]
    commit: Option<String>,
    /// Open the blame view for a file
    #[arg(long)]
    blame: bool,
    /// Open the repository settings
    #[arg(short, long)]
    settings: bool,
    /// Open the repository's Pipelines (the `gh` Actions analog)
    #[arg(short, long)]
    actions: bool,
    /// Open the repository wiki
    #[arg(short, long)]
    wiki: bool,
    /// Print the URL instead of opening a browser
    #[arg(short, long)]
    no_browser: bool,
}

/// What the positional argument names, as in `gh browse`.
#[derive(Debug, PartialEq)]
enum Target<'a> {
    Pr(&'a str),
    Commit(&'a str),
    /// Path, plus `12` or `10-20` when given as `PATH:12` / `PATH:10-20`.
    Path(&'a str, Option<&'a str>),
}

/// A number is a PR, 7 to 40 lowercase hex characters a commit, anything else a path.
fn classify(arg: &str) -> Target<'_> {
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit());
    if digits(arg) {
        return Target::Pr(arg);
    }
    if (7..=40).contains(&arg.len()) && arg.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Target::Commit(arg);
    }
    match arg.rsplit_once(':') {
        Some((p, l)) if digits(l) || l.split_once('-').is_some_and(|(a, b)| digits(a) && digits(b)) => {
            Target::Path(p, Some(l))
        }
        _ => Target::Path(arg, None),
    }
}

/// URL of the repo home (no revision), or of `path` at `rev` in the source (or blame) view.
fn browse_url(web: &str, rev: Option<&str>, path: &str, lines: Option<&str>, blame: bool) -> String {
    let path = path.trim_start_matches("./").trim_start_matches('/');
    // Bitbucket line anchors: #lines-12, #lines-10:20
    let anchor = lines.map(|l| format!("#lines-{}", l.replace('-', ":"))).unwrap_or_default();
    let view = if blame { "annotate" } else { "src" };
    match rev {
        None => web.to_string(),
        Some(r) => format!("{web}/{view}/{r}/{path}{anchor}"),
    }
}

/// True when the current directory is a checkout of `repo` (with -R it may be another repo).
fn is_local(repo: &Repo) -> bool {
    git(&["remote", "get-url", "origin"]).ok().and_then(|r| Repo::parse(&r).ok()).as_ref() == Some(repo)
}

fn main_branch(client: &Client, repo: &Repo) -> Result<String> {
    Ok(s(&client.get(&repo.api())?, "mainbranch.name"))
}

/// The local branch if browsing the local checkout, else the repo's main branch.
fn default_branch(repo: &Repo) -> Result<String> {
    match current_branch().ok().filter(|_| is_local(repo)) {
        Some(b) => Ok(b),
        None => main_branch(&Client::new()?, repo),
    }
}

/// `--commit` without a SHA: local HEAD if browsing the local checkout, else the main branch's head.
fn last_commit(repo: &Repo) -> Result<String> {
    if is_local(repo) {
        return git(&["rev-parse", "HEAD"]);
    }
    let client = Client::new()?;
    let main = main_branch(&client, repo)?;
    let page = client.get(&format!("{}/commits/{}?pagelen=1", repo.api(), enc(&main)))?;
    Ok(s(&page["values"][0], "hash"))
}

pub fn run(args: Args) -> Result<()> {
    let repo = Repo::current()?;
    let web = repo.web();
    let section = [(args.settings, "admin"), (args.actions, "pipelines"), (args.wiki, "wiki")]
        .into_iter()
        .find_map(|(on, page)| on.then_some(page));
    let url = match (section, args.target.as_deref().map(classify)) {
        (Some(page), _) => format!("{web}/{page}"),
        (None, Some(Target::Pr(n))) => format!("{web}/pull-requests/{n}"),
        (None, Some(Target::Commit(c))) => format!("{web}/commits/{c}"),
        (None, target) => {
            let (path, lines) = match target {
                Some(Target::Path(p, l)) => (Some(p), l),
                _ => (None, None),
            };
            if args.blame && path.is_none() {
                bail!("--blame needs a file path");
            }
            let rev = match (args.commit, args.branch) {
                (Some(c), _) if c == "last" => Some(last_commit(&repo)?),
                (Some(c), _) => Some(c),
                (None, Some(b)) => Some(b),
                (None, None) if path.is_some() => Some(default_branch(&repo)?),
                (None, None) => None,
            };
            browse_url(&web, rev.as_deref(), path.unwrap_or(""), lines, args.blame)
        }
    };
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
        assert_eq!(browse_url(w, None, "", None, false), w);
        assert_eq!(browse_url(w, Some("main"), "", None, false), "https://bitbucket.org/ws/r/src/main/");
        assert_eq!(
            browse_url(w, Some("feat/x"), "./src/a.rs", None, false),
            "https://bitbucket.org/ws/r/src/feat/x/src/a.rs"
        );
        assert_eq!(
            browse_url(w, Some("main"), "a.rs", Some("12"), false),
            "https://bitbucket.org/ws/r/src/main/a.rs#lines-12"
        );
        assert_eq!(
            browse_url(w, Some("abc1234"), "/a.rs", Some("10-20"), true),
            "https://bitbucket.org/ws/r/annotate/abc1234/a.rs#lines-10:20"
        );
    }

    #[test]
    fn targets() {
        assert_eq!(classify("12"), Target::Pr("12"));
        assert_eq!(classify("1234567"), Target::Pr("1234567"));
        assert_eq!(classify("77507cd"), Target::Commit("77507cd"));
        assert_eq!(
            classify("77507cd94ccafcf568f8560cfecde965fcfa63"),
            Target::Commit("77507cd94ccafcf568f8560cfecde965fcfa63")
        );
        assert_eq!(classify("abcdef"), Target::Path("abcdef", None));
        assert_eq!(classify("ABCDEF1"), Target::Path("ABCDEF1", None));
        assert_eq!(classify("src/main.rs"), Target::Path("src/main.rs", None));
        assert_eq!(classify("main.go:312"), Target::Path("main.go", Some("312")));
        assert_eq!(classify("main.go:10-20"), Target::Path("main.go", Some("10-20")));
        assert_eq!(classify("a:b"), Target::Path("a:b", None));
        assert_eq!(classify("a:1-"), Target::Path("a:1-", None));
    }
}
