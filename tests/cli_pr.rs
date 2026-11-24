//! `bb pr ...` end to end against a mock Bitbucket API.
mod common;

use common::{json, route, text, Mock};
use serde_json::{json, Value};

const PR: &str = "/repositories/ws/r/pullrequests/7";

fn pr7() -> Value {
    json!({
        "id": 7,
        "title": "Add widgets",
        "state": "OPEN",
        "draft": false,
        "author": {"display_name": "Ada"},
        "source": {"branch": {"name": "feature/widgets"}},
        "destination": {"branch": {"name": "main"}},
        "description": "Adds the widget API.",
        "links": {"html": {"href": "https://bitbucket.org/ws/r/pull-requests/7"}},
        "participants": [
            {"role": "REVIEWER", "approved": true, "user": {"display_name": "Bob"}},
            {"role": "REVIEWER", "approved": false, "state": "changes_requested", "user": {"display_name": "Cy"}},
            {"role": "REVIEWER", "approved": false, "user": {"display_name": "Di"}},
            {"role": "PARTICIPANT", "approved": false, "user": {"display_name": "Eve"}}
        ]
    })
}

fn comments() -> Value {
    json!({"values": [
        {"id": 1, "user": {"display_name": "Bob", "account_id": "bob"}, "created_on": "2026-01-02T03:04:05.000000+00:00",
         "content": {"raw": "Looks good"}},
        {"id": 2, "user": {"display_name": "Me", "account_id": "me"}, "created_on": "2026-01-02T04:00:00.000000+00:00",
         "inline": {"path": "src/lib.rs", "to": 12}, "content": {"raw": "nit: rename"}},
        {"id": 3, "user": {"display_name": "Me", "account_id": "me"}, "created_on": "2026-01-03T00:00:00.000000+00:00",
         "deleted": true, "content": {"raw": "gone"}}
    ]})
}

#[test]
fn list_prints_a_table() {
    let m = Mock::start(vec![json(
        "GET",
        "/repositories/ws/r/pullrequests?state=OPEN&pagelen=30",
        json!({"size": 2, "values": [
            pr7(),
            {"id": 8, "title": "WIP", "state": "OPEN", "draft": true, "author": {"display_name": "Bob"},
             "source": {"branch": {"name": "wip"}}}
        ]}),
    )]);
    let out = m.bb(&["pr", "list", "-R", "ws/r"]);
    let stdout = out.ok();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert!(lines[0].starts_with("ID") && lines[0].contains("TITLE") && lines[0].contains("STATE"));
    assert!(lines[1].contains("#7") && lines[1].contains("Add widgets") && lines[1].contains("feature/widgets") && lines[1].contains("Ada"));
    assert!(lines[2].contains("#8") && lines[2].contains("OPEN (draft)"));
    assert!(out.stderr.is_empty(), "no `more` note when everything fit: {}", out.stderr);
}

#[test]
fn list_follows_pages_and_notes_truncation() {
    let page = |ids: std::ops::Range<u64>, next: bool| {
        let mut v = json!({"size": 45, "values": ids.map(|i| json!({"id": i, "title": format!("PR {i}"), "state": "OPEN"})).collect::<Vec<_>>()});
        if next {
            v["next"] = json!("{base}/repositories/ws/r/pullrequests?page=2");
        }
        v
    };
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r/pullrequests?state=OPEN&pagelen=30", page(0..20, true)),
        json("GET", "/repositories/ws/r/pullrequests?page=2", page(20..40, true)),
    ]);
    let out = m.bb(&["pr", "list", "-R", "ws/r"]);
    assert_eq!(out.ok().lines().count(), 31, "header + 30 rows");
    assert_eq!(out.stderr.trim(), "Showing 30 of 45 pull requests. Use -L N to see more, or -L 0 for all.");
    assert_eq!(m.requests().len(), 2);
}

#[test]
fn list_sends_filters_and_prints_json() {
    let m = Mock::start(vec![json("GET", "/repositories/ws/r/pullrequests", json!({"values": [pr7()]}))]);
    let v = m
        .bb(&["pr", "list", "-R", "ws/r", "--json", "-s", "merged", "-A", "ada", "-B", "main", "-S", "fix \"x\"", "-L", "5"])
        .json();
    assert_eq!(v[0]["id"], 7);
    let q = r#"author.username="ada" AND destination.branch.name="main" AND (title ~ "fix \"x\"" OR description ~ "fix \"x\"")"#;
    let want = format!("/repositories/ws/r/pullrequests?state=MERGED&q={}&pagelen=5", enc(q));
    assert_eq!(m.lines(), [format!("GET {want}")]);
}

#[test]
fn list_with_no_results() {
    let m = Mock::start(vec![json("GET", "/repositories/ws/r/pullrequests", json!({"values": []}))]);
    let out = m.bb(&["pr", "list", "-R", "ws/r"]);
    assert_eq!(out.ok(), "");
    assert_eq!(out.stderr.trim(), "No pull requests found");
}

#[test]
fn view_shows_reviewers_description_and_comments() {
    let m = Mock::start(vec![json("GET", PR, pr7()), json("GET", &format!("{PR}/comments"), comments())]);
    let out = m.bb(&["pr", "view", "7", "-R", "ws/r", "--comments"]);
    let stdout = out.ok();
    for want in [
        "Add widgets #7",
        "OPEN • Ada • feature/widgets → main",
        "Reviewer: Bob (approved)",
        "Reviewer: Cy (changes requested)",
        "Reviewer: Di (pending)",
        "Adds the widget API.",
        "https://bitbucket.org/ws/r/pull-requests/7",
        "Looks good",
        "src/lib.rs:12",
        "nit: rename",
    ] {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
    assert!(!stdout.contains("Eve"), "participants who aren't reviewers are not listed");
    assert!(!stdout.contains("gone"), "deleted comments are hidden");
    let auth = m.one("GET", PR);
    assert_eq!(auth.header("authorization"), Some("Basic bWVAZXhhbXBsZS5jb206dG9r")); // me@example.com:tok
}

#[test]
fn view_accepts_a_pull_request_url() {
    let m = Mock::start(vec![json("GET", PR, pr7())]);
    let v = m.bb(&["pr", "view", "https://bitbucket.org/ws/r/pull-requests/7", "--json"]).json();
    assert_eq!(v["title"], "Add widgets");
}

#[test]
fn view_by_branch_prefers_the_open_pull_request() {
    let q = format!(
        "/repositories/ws/r/pullrequests?q={}&state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&sort=-updated_on&pagelen=10",
        enc(r#"source.branch.name="feature/widgets""#)
    );
    let m = Mock::start(vec![
        json("GET", &q, json!({"values": [{"id": 5, "state": "MERGED"}, {"id": 7, "state": "OPEN"}]})),
        json("GET", PR, pr7()),
    ]);
    let v = m.bb(&["pr", "view", "feature/widgets", "-R", "ws/r", "--json"]).json();
    assert_eq!(v["id"], 7);
    assert_eq!(m.lines(), [format!("GET {q}"), format!("GET {PR}")]);
}

#[test]
fn view_by_branch_without_pull_requests_fails() {
    let m = Mock::start(vec![json("GET", "/repositories/ws/r/pullrequests", json!({"values": []}))]);
    let err = m.bb(&["pr", "view", "nope", "-R", "ws/r"]).fails(1).to_string();
    assert!(err.contains("no pull requests found for branch `nope`"), "{err}");
}

#[test]
fn api_errors_exit_1_with_the_api_message() {
    let m = Mock::start(vec![route("GET", PR, 404, r#"{"type":"error","error":{"message":"Pull request not found"}}"#)]);
    let err = m.bb(&["pr", "view", "7", "-R", "ws/r"]).fails(1).to_string();
    assert_eq!(err.trim(), "bb: HTTP 404: Pull request not found");
}

#[test]
fn missing_credentials_exit_4() {
    let out = common::Run::from(common::bb_command(&["pr", "view", "7", "-R", "ws/r"]).output().unwrap());
    let err = out.fails(4);
    assert!(err.contains("not logged in"), "{err}");
}

#[test]
fn create_posts_the_pull_request() {
    let m = Mock::start(vec![json("POST", "/repositories/ws/r/pullrequests", pr7())]);
    let out = m.bb(&[
        "pr", "create", "-R", "ws/r", "-t", "Add widgets", "-b", "Body", "-B", "main", "-H", "feature/widgets", "-r", "{u-1}", "-r",
        "acct-2", "-d",
    ]);
    assert_eq!(out.ok().trim(), "https://bitbucket.org/ws/r/pull-requests/7");
    let body = m.one("POST", "/repositories/ws/r/pullrequests").json();
    assert_eq!(
        body,
        json!({
            "title": "Add widgets",
            "description": "Body",
            "source": {"branch": {"name": "feature/widgets"}},
            "destination": {"branch": {"name": "main"}},
            "close_source_branch": false,
            "draft": true,
            "reviewers": [{"uuid": "{u-1}"}, {"account_id": "acct-2"}]
        })
    );
}

#[test]
fn create_dry_run_sends_nothing() {
    let m = Mock::start(vec![]);
    let out = m.bb(&["pr", "create", "-R", "ws/r", "-t", "T", "-b", "Hello", "-H", "feat", "-r", "acct-2", "--dry-run"]);
    let stdout = out.ok();
    assert!(stdout.contains("Would create pull request in ws/r: feat → (main branch)"), "{stdout}");
    assert!(stdout.contains("title: T") && stdout.contains("reviewers: acct-2") && stdout.contains("Hello"), "{stdout}");
    assert!(m.requests().is_empty());
}

#[test]
fn create_requires_a_title() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["pr", "create", "-R", "ws/r", "-H", "feat"]).fails(1).to_string();
    assert!(err.contains("--title is required"), "{err}");
}

#[test]
fn merge_sends_strategy_message_and_close_flag() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/merge"), pr7())]);
    let out = m.bb(&["pr", "merge", "7", "-R", "ws/r", "--squash", "--subject", "Subj", "--body", "Text", "--close-source-branch"]);
    assert_eq!(out.ok().trim(), "Merged pull request #7");
    let body = m.one("POST", &format!("{PR}/merge")).json();
    assert_eq!(body, json!({"merge_strategy": "squash", "message": "Subj\n\nText", "close_source_branch": true}));
}

#[test]
fn close_comments_then_declines() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/comments"), json!({"id": 9})), json("POST", &format!("{PR}/decline"), pr7())]);
    assert_eq!(m.bb(&["pr", "close", "7", "-R", "ws/r", "-c", "Superseded"]).ok().trim(), "Declined pull request #7");
    assert_eq!(m.lines(), [format!("POST {PR}/comments"), format!("POST {PR}/decline")]);
    assert_eq!(m.requests()[0].json(), json!({"content": {"raw": "Superseded"}}));
    let decline = m.one("POST", &format!("{PR}/decline"));
    assert_eq!(decline.header("content-length"), Some("0"), "bodyless POST sends Content-Length: 0");
}

#[test]
fn approve_unapprove_and_request_changes() {
    let m = Mock::start(vec![
        json("POST", &format!("{PR}/approve"), json!({"approved": true})),
        route("DELETE", &format!("{PR}/approve"), 204, ""),
        json("POST", &format!("{PR}/request-changes"), json!({})),
        route("DELETE", &format!("{PR}/request-changes"), 204, ""),
    ]);
    assert_eq!(m.bb(&["pr", "approve", "7", "-R", "ws/r"]).ok().trim(), "Approved pull request #7");
    assert_eq!(m.bb(&["pr", "unapprove", "7", "-R", "ws/r"]).ok().trim(), "Removed approval from pull request #7");
    assert_eq!(m.bb(&["pr", "request-changes", "7", "-R", "ws/r"]).ok().trim(), "Requested changes on pull request #7");
    assert_eq!(m.bb(&["pr", "request-changes", "7", "-R", "ws/r", "--undo"]).ok().trim(), "Withdrew change request on pull request #7");
    assert_eq!(
        m.lines(),
        [format!("POST {PR}/approve"), format!("DELETE {PR}/approve"), format!("POST {PR}/request-changes"), format!("DELETE {PR}/request-changes")]
    );
    // --json prints the raw response instead of the message.
    assert_eq!(m.bb(&["pr", "approve", "7", "-R", "ws/r", "--json"]).json(), json!({"approved": true}));
}

#[test]
fn review_request_changes_comments_first() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/comments"), json!({})), json("POST", &format!("{PR}/request-changes"), json!({}))]);
    let out = m.bb(&["pr", "review", "7", "-R", "ws/r", "--request-changes", "-b", "Please add tests"]);
    assert_eq!(out.ok().trim(), "Requested changes on pull request #7");
    assert_eq!(m.lines(), [format!("POST {PR}/comments"), format!("POST {PR}/request-changes")]);
}

#[test]
fn review_without_body_fails_before_any_request() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["pr", "review", "7", "-R", "ws/r", "--comment"]).fails(1).to_string();
    assert!(err.contains("--body or --body-file is required"), "{err}");
    assert!(m.requests().is_empty());
}

#[test]
fn comment_inline() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/comments"), json!({"id": 10}))]);
    let out = m.bb(&["pr", "comment", "7", "-R", "ws/r", "-b", "Why?", "--file", "src/lib.rs", "--line", "3"]);
    assert_eq!(out.ok().trim(), "Commented on pull request #7");
    assert_eq!(m.one("POST", &format!("{PR}/comments")).json(), json!({"content": {"raw": "Why?"}, "inline": {"path": "src/lib.rs", "to": 3}}));
}

#[test]
fn comment_body_from_file() {
    let m = Mock::start(vec![json("POST", &format!("{PR}/comments"), json!({"id": 10}))]);
    let file = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("comment-body.md");
    std::fs::write(&file, "From a file\n").unwrap();
    m.bb(&["pr", "comment", "7", "-R", "ws/r", "-F", file.to_str().unwrap()]).ok();
    assert_eq!(m.one("POST", &format!("{PR}/comments")).json()["content"]["raw"], "From a file\n");
}

#[test]
fn comment_edit_last_updates_your_newest_comment() {
    let m = Mock::start(vec![
        json("GET", "/user", json!({"account_id": "me"})),
        json("GET", &format!("{PR}/comments"), comments()),
        json("PUT", &format!("{PR}/comments/2"), json!({"id": 2})),
    ]);
    let out = m.bb(&["pr", "comment", "7", "-R", "ws/r", "--edit-last", "-b", "renamed"]);
    assert_eq!(out.ok().trim(), "Edited your last comment on pull request #7");
    // Comment 3 is newer but deleted, so comment 2 is edited.
    assert_eq!(m.one("PUT", &format!("{PR}/comments/2")).json(), json!({"content": {"raw": "renamed"}}));
}

#[test]
fn comment_edit_last_without_comments() {
    let m = Mock::start(vec![
        json("GET", "/user", json!({"account_id": "nobody"})),
        json("GET", &format!("{PR}/comments"), comments()),
        json("POST", &format!("{PR}/comments"), json!({"id": 11})),
    ]);
    let err = m.bb(&["pr", "comment", "7", "-R", "ws/r", "--edit-last", "-b", "x"]).fails(1).to_string();
    assert!(err.contains("--create-if-none"), "{err}");
    m.bb(&["pr", "comment", "7", "-R", "ws/r", "--edit-last", "--create-if-none", "-b", "x"]).ok();
    m.one("POST", &format!("{PR}/comments"));
}

#[test]
fn comment_delete_last() {
    let m = Mock::start(vec![
        json("GET", "/user", json!({"account_id": "me"})),
        json("GET", &format!("{PR}/comments"), comments()),
        route("DELETE", &format!("{PR}/comments/2"), 204, ""),
    ]);
    let err = m.bb(&["pr", "comment", "7", "-R", "ws/r", "--delete-last"]).fails(1).to_string();
    assert!(err.contains("without --yes"), "{err}");
    assert!(m.requests().is_empty());
    let out = m.bb(&["pr", "comment", "7", "-R", "ws/r", "--delete-last", "--yes"]);
    assert_eq!(out.ok().trim(), "Deleted your last comment on pull request #7");
    m.one("DELETE", &format!("{PR}/comments/2"));
}

#[test]
fn comments_json() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/comments"), comments())]);
    let v = m.bb(&["pr", "comments", "7", "-R", "ws/r", "--json"]).json();
    assert_eq!(v.as_array().unwrap().len(), 3, "--json keeps deleted comments: it is the raw API data");
}

fn statuses(states: &[&str]) -> Value {
    json!({"values": states.iter().enumerate().map(|(i, st)| json!({"state": st, "name": format!("check-{i}"), "url": format!("https://ci/{i}")})).collect::<Vec<_>>()})
}

#[test]
fn checks_all_passing() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/statuses"), statuses(&["SUCCESSFUL", "SUCCESSFUL"]))]);
    let stdout = m.bb(&["pr", "checks", "7", "-R", "ws/r"]).ok().to_string();
    assert!(stdout.contains("check-0") && stdout.contains("https://ci/1"), "{stdout}");
}

#[test]
fn checks_failed_exit_1() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/statuses"), statuses(&["SUCCESSFUL", "FAILED", "STOPPED"]))]);
    let err = m.bb(&["pr", "checks", "7", "-R", "ws/r"]).fails(1).to_string();
    assert!(err.contains("2 check(s) failed"), "{err}");
}

#[test]
fn checks_pending_exit_8() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/statuses"), statuses(&["SUCCESSFUL", "INPROGRESS"]))]);
    let err = m.bb(&["pr", "checks", "7", "-R", "ws/r", "--json"]).fails(8).to_string();
    assert!(err.contains("1 check(s) pending"), "{err}");
}

#[test]
fn checks_watch_stops_once_nothing_is_pending() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/statuses"), statuses(&["SUCCESSFUL"]))]);
    let stdout = m.bb(&["pr", "checks", "7", "-R", "ws/r", "--watch"]).ok().to_string();
    assert!(stdout.contains("0 failed, 1 passed, 0 pending"), "{stdout}");
}

#[test]
fn checks_none_reported() {
    let m = Mock::start(vec![json("GET", &format!("{PR}/statuses"), json!({"values": []}))]);
    let out = m.bb(&["pr", "checks", "7", "-R", "ws/r"]);
    out.ok();
    assert_eq!(out.stderr.trim(), "No checks reported");
}

const DIFF: &str = "\
diff --git a/Cargo.lock b/Cargo.lock
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1 +1 @@
-old
+new
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-a
+b
";

#[test]
fn diff_excludes_files() {
    let m = Mock::start(vec![text("GET", &format!("{PR}/diff"), DIFF)]);
    let stdout = m.bb(&["pr", "diff", "7", "-R", "ws/r", "--exclude", "*.lock"]).ok().to_string();
    assert!(stdout.starts_with("diff --git a/src/lib.rs"), "{stdout}");
    assert!(!stdout.contains("Cargo.lock"));
    assert!(!stdout.contains('\x1b'), "no color when stdout is not a terminal");
}

#[test]
fn diff_color_always() {
    let m = Mock::start(vec![text("GET", &format!("{PR}/patch"), DIFF)]);
    let stdout = m.bb(&["pr", "diff", "7", "-R", "ws/r", "--patch", "--color", "always"]).ok().to_string();
    assert!(stdout.contains("\x1b[32m+new"), "{stdout:?}");
}

#[test]
fn diff_name_only_uses_diffstat() {
    let m = Mock::start(vec![json(
        "GET",
        &format!("{PR}/diffstat"),
        json!({"values": [{"new": {"path": "src/lib.rs"}}, {"new": null, "old": {"path": "gone.txt"}}, {"new": {"path": "Cargo.lock"}}]}),
    )]);
    let stdout = m.bb(&["pr", "diff", "7", "-R", "ws/r", "--name-only", "-e", "Cargo.lock"]).ok().to_string();
    assert_eq!(stdout, "src/lib.rs\ngone.txt\n");
}

/// Percent-encoding as `bb` does it for `q=` values.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
