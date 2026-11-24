//! Commands with local side effects: opening a browser (`--web`, `bb browse`) and running git
//! (repository detection, `pr checkout`, `pr create --fill`, `--delete-branch`, `repo clone`).
//! The browser is a recorder (`common::Browser`) and git runs in throwaway repositories.
mod common;

use common::{Browser, GitRepo, Mock, json, route};
use serde_json::json;

const PR: &str = "/repositories/ws/r/pullrequests/7";

fn pr7() -> serde_json::Value {
    json!({
        "id": 7,
        "state": "OPEN",
        "source": {"branch": {"name": "feature/widgets"}},
        "destination": {"branch": {"name": "main"}},
        "links": {"html": {"href": "https://bitbucket.org/ws/r/pull-requests/7"}}
    })
}

// ---- browser ----

#[test]
fn pr_list_web_opens_the_state_filter() {
    let m = Mock::start(vec![]);
    assert_eq!(
        m.opened(&["pr", "list", "-R", "ws/r", "--web", "-s", "merged"]),
        "https://bitbucket.org/ws/r/pull-requests/?state=MERGED"
    );
    assert_eq!(
        m.opened(&["pr", "list", "-R", "ws/r", "--web", "-s", "all"]),
        "https://bitbucket.org/ws/r/pull-requests/?state=ALL"
    );
    assert!(m.requests().is_empty(), "--web needs no API call");
}

#[test]
fn pr_web_flags_open_the_pull_request() {
    let m = Mock::start(vec![json("GET", PR, pr7())]);
    let page = "https://bitbucket.org/ws/r/pull-requests/7";
    assert_eq!(m.opened(&["pr", "view", "7", "-R", "ws/r", "--web"]), page);
    assert_eq!(m.opened(&["pr", "comment", "7", "-R", "ws/r", "--web"]), page);
    assert_eq!(m.opened(&["pr", "checks", "7", "-R", "ws/r", "--web"]), page);
    assert_eq!(m.opened(&["pr", "diff", "7", "-R", "ws/r", "--web"]), format!("{page}/diff"));
}

#[test]
fn pr_create_web_opens_the_new_pull_request_form() {
    let m = Mock::start(vec![]);
    assert_eq!(
        m.opened(&["pr", "create", "-R", "ws/r", "-H", "feature/a b", "--web"]),
        "https://bitbucket.org/ws/r/pull-requests/new?source=feature%2Fa%20b"
    );
}

#[test]
fn repo_and_pipeline_web() {
    let m = Mock::start(vec![json(
        "GET",
        "/repositories/ws/r/pipelines/42",
        json!({"uuid": "{p}", "build_number": 42, "state": {"name": "COMPLETED"}}),
    )]);
    assert_eq!(m.opened(&["repo", "view", "ws/r", "--web"]), "https://bitbucket.org/ws/r");
    assert_eq!(m.opened(&["repo", "view", "ws/r", "--web", "-b", "dev"]), "https://bitbucket.org/ws/r/src/dev/");
    assert_eq!(
        m.opened(&["pipeline", "view", "42", "-R", "ws/r", "--web"]),
        "https://bitbucket.org/ws/r/pipelines/results/42"
    );
}

#[test]
fn browse_sections_and_targets() {
    let m = Mock::start(vec![]);
    let web = "https://bitbucket.org/ws/r";
    assert_eq!(m.opened(&["browse", "-R", "ws/r"]), web);
    assert_eq!(m.opened(&["browse", "-R", "ws/r", "12"]), format!("{web}/pull-requests/12"));
    assert_eq!(m.opened(&["browse", "-R", "ws/r", "abc1234"]), format!("{web}/commits/abc1234"));
    assert_eq!(m.opened(&["browse", "-R", "ws/r", "--settings"]), format!("{web}/admin"));
    assert_eq!(m.opened(&["browse", "-R", "ws/r", "--actions"]), format!("{web}/pipelines"));
    assert_eq!(m.opened(&["browse", "-R", "ws/r", "--wiki"]), format!("{web}/wiki"));
    assert_eq!(
        m.opened(&["browse", "-R", "ws/r", "-b", "dev", "src/main.rs:5-9"]),
        format!("{web}/src/dev/src/main.rs#lines-5:9")
    );
}

#[test]
fn browse_no_browser_prints_the_url() {
    let m = Mock::start(vec![]);
    assert_eq!(
        m.bb(&["browse", "-R", "ws/r", "-n", "--commit=abc1234", "--blame", "a.rs"]).ok(),
        "https://bitbucket.org/ws/r/annotate/abc1234/a.rs\n"
    );
    let err = m.bb(&["browse", "-R", "ws/r", "--blame"]).fails(1).to_string();
    assert!(err.contains("--blame needs a file path"), "{err}");
}

#[test]
fn browse_another_repo_uses_its_main_branch() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/other", json!({"mainbranch": {"name": "trunk"}})),
        json("GET", "/repositories/ws/other/commits/trunk?pagelen=1", json!({"values": [{"hash": "f00d"}]})),
    ]);
    assert_eq!(
        m.bb(&["browse", "-R", "ws/other", "-n", "README.md"]).ok().trim(),
        "https://bitbucket.org/ws/other/src/trunk/README.md"
    );
    assert_eq!(
        m.bb(&["browse", "-R", "ws/other", "-n", "--commit"]).ok().trim(),
        "https://bitbucket.org/ws/other/src/f00d/"
    );
}

#[test]
fn browser_falls_back_to_browser_env_var() {
    let m = Mock::start(vec![]);
    let browser = Browser::new();
    let (_, cmd) = browser.env();
    m.bb_with(&["browse", "-R", "ws/r"], &[("BB_BROWSER", ""), ("BROWSER", &cmd)]).ok();
    assert_eq!(browser.opened().as_deref(), Some("https://bitbucket.org/ws/r"));
}

#[test]
fn browser_that_cannot_run_is_an_error() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["browse", "-R", "ws/r"]).fails(1).to_string();
    assert!(err.contains("could not run browser `bb-test-no-browser` for https://bitbucket.org/ws/r"), "{err}");
}

// ---- repository detection ----

#[test]
fn repo_comes_from_the_origin_remote() {
    let repo = GitRepo::new();
    let m = Mock::start(vec![
        json("GET", "/repositories/ws", json!({"values": []})),
        json("GET", "/repositories/ws/r", json!({"full_name": "ws/r"})),
    ]);
    m.bb_in(Some(&repo.dir), &["repo", "list"], &[]).ok();
    assert_eq!(m.bb_in(Some(&repo.dir), &["repo", "view", "--json"], &[]).json()["full_name"], "ws/r");
    assert_eq!(m.lines()[0], "GET /repositories/ws?sort=-updated_on&pagelen=30");
}

#[test]
fn outside_a_repo_asks_for_dash_r() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["pr", "list"]).fails(1).to_string();
    assert!(err.contains("not in a git repo with an `origin` remote; use -R WORKSPACE/REPO"), "{err}");
}

#[test]
fn non_bitbucket_origin_asks_for_dash_r() {
    let repo = GitRepo::new();
    repo.git(&["remote", "set-url", "origin", "https://github.com/a/b.git"]);
    let err = Mock::start(vec![]).bb_in(Some(&repo.dir), &["pr", "list"], &[]).fails(1).to_string();
    assert!(err.contains("`origin` is not a Bitbucket remote"), "{err}");
}

#[test]
fn pr_view_defaults_to_the_current_branch() {
    let repo = GitRepo::new();
    repo.git(&["checkout", "-q", "-b", "feature/widgets"]);
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r/pullrequests", json!({"values": [{"id": 7, "state": "OPEN"}]})),
        json("GET", PR, pr7()),
    ]);
    assert_eq!(m.bb_in(Some(&repo.dir), &["pr", "view", "--json"], &[]).json()["id"], 7);
    assert!(m.lines()[0].contains("q=source.branch.name%3D%22feature%2Fwidgets%22"), "{:?}", m.lines());
}

#[test]
fn detached_head_has_no_current_branch() {
    let repo = GitRepo::new();
    repo.git(&["checkout", "-q", "--detach"]);
    let err = Mock::start(vec![]).bb_in(Some(&repo.dir), &["pr", "view"], &[]).fails(1).to_string();
    assert!(err.contains("not on a branch (detached HEAD)"), "{err}");
}

#[test]
fn browse_a_path_in_the_local_checkout_uses_the_local_branch_and_head() {
    let repo = GitRepo::new();
    repo.git(&["checkout", "-q", "-b", "dev"]);
    let head = repo.commit("Work", "");
    let m = Mock::start(vec![]);
    assert_eq!(
        m.bb_in(Some(&repo.dir), &["browse", "-n", "src/lib.rs:3"], &[]).ok().trim(),
        "https://bitbucket.org/ws/r/src/dev/src/lib.rs#lines-3"
    );
    assert_eq!(
        m.bb_in(Some(&repo.dir), &["browse", "-n", "--commit"], &[]).ok().trim(),
        format!("https://bitbucket.org/ws/r/src/{head}/")
    );
    assert!(m.requests().is_empty(), "the local checkout answers without the API");
}

#[test]
fn pipeline_defaults_to_the_current_branch() {
    let repo = GitRepo::new();
    let latest =
        json!({"uuid": "{p}", "build_number": 9, "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}}});
    let m = Mock::start(vec![
        json(
            "GET",
            "/repositories/ws/r/pipelines/?sort=-created_on&pagelen=1&target.branch=main",
            json!({"values": [latest]}),
        ),
        json("GET", "/repositories/ws/r/pipelines/%7Bp%7D/steps/", json!({"values": []})),
        json("POST", "/repositories/ws/r/pipelines/", json!({"build_number": 10})),
    ]);
    assert_eq!(m.bb_in(Some(&repo.dir), &["pipeline", "view", "--json"], &[]).json()["pipeline"]["build_number"], 9);
    m.bb_in(Some(&repo.dir), &["pipeline", "run"], &[]).ok();
    assert_eq!(m.one("POST", "/repositories/ws/r/pipelines/").json()["target"]["ref_name"], "main");
}

#[test]
fn pipeline_view_with_no_pipelines_for_the_branch() {
    let repo = GitRepo::new();
    let m = Mock::start(vec![json("GET", "/repositories/ws/r/pipelines/", json!({"values": []}))]);
    let err = m.bb_in(Some(&repo.dir), &["pipeline", "view"], &[]).fails(1).to_string();
    assert!(err.contains("no pipelines for branch `main`"), "{err}");
}

// ---- pr create --fill ----

/// A repo on `feature/add-widgets` with `origin/main` at the initial commit, plus `commits`.
fn feature_repo(commits: &[(&str, &str)]) -> GitRepo {
    let repo = GitRepo::new();
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&["checkout", "-q", "-b", "feature/add-widgets"]);
    for (subject, body) in commits {
        repo.commit(subject, body);
    }
    repo
}

fn fill(repo: &GitRepo, m: &Mock, flags: &[&str]) -> serde_json::Value {
    let mut args = vec!["pr", "create", "--dry-run", "--json"];
    args.extend(flags);
    m.bb_in(Some(&repo.dir), &args, &[]).json()
}

#[test]
fn fill_with_one_commit_uses_its_subject_and_body() {
    let repo = feature_repo(&[("Add widgets", "They spin.")]);
    let v = fill(&repo, &Mock::start(vec![]), &["--fill", "-B", "main"]);
    assert_eq!((v["title"].as_str(), v["description"].as_str()), (Some("Add widgets"), Some("They spin.")));
    assert_eq!(v["source"]["branch"]["name"], "feature/add-widgets");
}

#[test]
fn fill_modes_with_several_commits() {
    let repo = feature_repo(&[("First", "One."), ("Second", "")]);
    let m = Mock::start(vec![json("GET", "/repositories/ws/r", json!({"mainbranch": {"name": "main"}}))]);
    let v = fill(&repo, &m, &["--fill"]);
    assert_eq!((v["title"].as_str(), v["description"].as_str()), (Some("Add widgets"), Some("- First\n- Second")));
    assert_eq!(m.lines(), ["GET /repositories/ws/r"], "without -B the base is the repo's main branch");
    let v = fill(&repo, &m, &["--fill-verbose", "-B", "main"]);
    assert_eq!(v["description"], "First\n\nOne.\n\nSecond");
    let v = fill(&repo, &m, &["--fill-first", "-B", "main", "-t", "Mine"]);
    assert_eq!((v["title"].as_str(), v["description"].as_str()), (Some("Mine"), Some("One.")));
}

// ---- git side effects ----

/// A local clone whose `origin` is a real bare repo holding `main` and `feature/widgets`; the
/// local repo itself only has `main`.
fn repo_with_remote_branch() -> GitRepo {
    let repo = GitRepo::new();
    let bare = GitRepo::bare();
    repo.git(&["remote", "set-url", "origin", bare.to_str().unwrap()]);
    repo.git(&["checkout", "-q", "-b", "feature/widgets"]);
    repo.commit("Widgets", "");
    repo.git(&["push", "-q", "origin", "main", "feature/widgets"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["branch", "-q", "-D", "feature/widgets"]);
    repo
}

#[test]
fn pr_checkout_creates_a_tracking_branch_then_reuses_it() {
    let repo = repo_with_remote_branch();
    let m = Mock::start(vec![json("GET", PR, pr7())]);
    m.bb_in(Some(&repo.dir), &["pr", "checkout", "7", "-R", "ws/r"], &[]).ok();
    assert_eq!(repo.branch(), "feature/widgets");
    assert_eq!(repo.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]), "origin/feature/widgets");
    // Again, with the branch already there: switches to it and fast-forwards.
    repo.git(&["checkout", "-q", "main"]);
    m.bb_in(Some(&repo.dir), &["pr", "checkout", "7", "-R", "ws/r"], &[]).ok();
    assert_eq!(repo.branch(), "feature/widgets");
}

#[test]
fn pr_checkout_detach_and_custom_branch() {
    let repo = repo_with_remote_branch();
    let m = Mock::start(vec![json("GET", PR, pr7())]);
    m.bb_in(Some(&repo.dir), &["pr", "checkout", "7", "-R", "ws/r", "--detach"], &[]).ok();
    assert_eq!(repo.branch(), "", "detached");
    m.bb_in(Some(&repo.dir), &["pr", "checkout", "7", "-R", "ws/r", "-b", "review-7"], &[]).ok();
    assert_eq!(repo.branch(), "review-7");
    // --force resets an existing branch to the PR's tip.
    repo.commit("Local only", "");
    m.bb_in(Some(&repo.dir), &["pr", "checkout", "7", "-R", "ws/r", "-b", "review-7", "--force"], &[]).ok();
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), repo.git(&["rev-parse", "origin/feature/widgets"]));
}

#[test]
fn merge_delete_branch_switches_away_and_deletes_the_local_branch() {
    let repo = GitRepo::new();
    repo.git(&["checkout", "-q", "-b", "feature/widgets"]);
    let m = Mock::start(vec![json("POST", &format!("{PR}/merge"), pr7())]);
    let out = m.bb_in(Some(&repo.dir), &["pr", "merge", "7", "-R", "ws/r", "--delete-branch"], &[]);
    assert_eq!(out.ok(), "Merged pull request #7\nDeleted local branch feature/widgets\n");
    assert_eq!(repo.branch(), "main");
    assert_eq!(m.one("POST", &format!("{PR}/merge")).json(), json!({"close_source_branch": true}));
}

#[test]
fn close_delete_branch_deletes_the_remote_and_local_branch() {
    let repo = GitRepo::new();
    repo.git(&["branch", "feature/widgets"]);
    let m = Mock::start(vec![
        json("POST", &format!("{PR}/decline"), pr7()),
        route("DELETE", "/repositories/ws/r/refs/branches/feature/widgets", 204, ""),
    ]);
    let out = m.bb_in(Some(&repo.dir), &["pr", "close", "7", "-R", "ws/r", "-d"], &[]);
    assert_eq!(
        out.ok(),
        "Declined pull request #7\nDeleted branch feature/widgets on Bitbucket\nDeleted local branch feature/widgets\n"
    );
    assert_eq!(repo.git(&["branch", "--list", "feature/widgets"]), "");
}

#[test]
fn delete_branch_without_a_local_copy_is_quiet() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/merge"), pr7())]);
    let repo = GitRepo::new();
    assert_eq!(
        m.bb_in(Some(&repo.dir), &["pr", "merge", "7", "-R", "ws/r", "-d"], &[]).ok(),
        "Merged pull request #7\n"
    );
}

#[test]
fn repo_clone_uses_the_ssh_url() {
    let src = repo_with_remote_branch();
    let bare = src.git(&["remote", "get-url", "origin"]);
    let dest = common::scratch_dir();
    // Point the Bitbucket SSH URL at the local bare repo, so the clone needs no network.
    let env = [
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", &format!("url.{bare}.insteadOf")),
        ("GIT_CONFIG_VALUE_0", "git@ssh.bitbucket.org:ws/r.git"),
    ];
    let m = Mock::start(vec![]);
    m.bb_in(Some(&dest), &["repo", "clone", "ws/r", "copy", "--", "--branch", "feature/widgets"], &env).ok();
    let clone = GitRepo { dir: dest.join("copy") };
    assert_eq!(clone.branch(), "feature/widgets");
    assert!(m.requests().is_empty());
}

#[test]
fn merge_delete_branch_json_output_stays_valid_json() {
    // git prints "Deleted branch ..." on stdout; it must not end up after the JSON.
    let repo = GitRepo::new();
    repo.git(&["branch", "feature/widgets"]);
    let m = Mock::start(vec![json("POST", &format!("{PR}/merge"), pr7())]);
    let v = m.bb_in(Some(&repo.dir), &["pr", "merge", "7", "-R", "ws/r", "-d", "--json"], &[]).json();
    assert_eq!(v["id"], 7);
    assert_eq!(repo.git(&["branch", "--list", "feature/widgets"]), "");
}
