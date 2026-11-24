//! `bb repo`, `bb workspace` and `bb api` end to end against a mock Bitbucket API.
mod common;

use common::{json, route, text, Mock};
use serde_json::json;

fn repo() -> serde_json::Value {
    json!({
        "full_name": "ws/r",
        "description": "The r repo",
        "is_private": false,
        "language": "rust",
        "updated_on": "2026-03-04T05:06:07.000000+00:00",
        "mainbranch": {"name": "main"},
        "links": {
            "html": {"href": "https://bitbucket.org/ws/r"},
            "clone": [{"name": "https", "href": "https://bitbucket.org/ws/r.git"}, {"name": "ssh", "href": "git@bitbucket.org:ws/r.git"}]
        }
    })
}

#[test]
fn repo_list_prints_a_table() {
    let m = Mock::start(vec![json(
        "GET",
        "/repositories/ws?sort=-updated_on&pagelen=30",
        json!({"values": [repo(), {"full_name": "ws/secret", "is_private": true}]}),
    )]);
    let stdout = m.bb(&["repo", "list", "ws"]).ok().to_string();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert!(lines[0].starts_with("NAME") && lines[0].contains("VISIBILITY"));
    assert!(lines[1].contains("ws/r") && lines[1].contains("public") && lines[1].contains("rust"));
    assert!(lines[2].contains("ws/secret") && lines[2].contains("private"));
}

#[test]
fn repo_list_defaults_to_the_current_repos_workspace_and_filters() {
    let m = Mock::start(vec![json("GET", "/repositories/ws", json!({"values": [repo()], "next": "{base}/repositories/ws?page=2"}))]);
    let v = m.bb(&["repo", "list", "-R", "ws/other", "--json", "-L", "1", "--visibility", "private", "-l", "Rust", "--source"]).json();
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(
        m.lines(),
        ["GET /repositories/ws?sort=-updated_on&pagelen=1&q=is_private%20%3D%20true%20AND%20language%20%3D%20%22rust%22%20AND%20parent%20%3D%20null"],
        "stops at -L without fetching the next page"
    );
}

#[test]
fn repo_list_needs_a_workspace_outside_a_repo() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["repo", "list"]).fails(1).to_string();
    assert!(err.contains("specify a workspace"), "{err}");
}

#[test]
fn repo_view_with_branch() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r", repo()),
        json(
            "GET",
            "/repositories/ws/r/refs/branches/feature%2Fx",
            json!({"name": "feature/x", "links": {"html": {"href": "https://bitbucket.org/ws/r/branch/feature/x"}},
                   "target": {"hash": "0123456789abcdef0123", "date": "2026-03-04T05:06:07+00:00", "message": "Fix it\n\nDetails"}}),
        ),
    ]);
    let stdout = m.bb(&["repo", "view", "ws/r", "-b", "feature/x"]).ok().to_string();
    for want in [
        "ws/r",
        "The r repo",
        "main",
        "public",
        "clone (ssh):",
        "git@bitbucket.org:ws/r.git",
        "feature/x",
        "0123456789ab ",
        "Fix it",
        "https://bitbucket.org/ws/r/branch/feature/x",
    ] {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
    assert!(!stdout.contains("Details"), "only the commit subject is shown");
}

#[test]
fn repo_view_json() {
    let m = Mock::start(vec![json("GET", "/repositories/ws/r", repo())]);
    assert_eq!(m.bb(&["repo", "view", "-R", "ws/r", "--json"]).json()["full_name"], "ws/r");
}

#[test]
fn repo_create() {
    let m = Mock::start(vec![json("POST", "/repositories/ws/new", json!({"links": {"html": {"href": "https://bitbucket.org/ws/new"}}}))]);
    let out = m.bb(&["repo", "create", "ws/new", "--public", "-d", "Fresh", "-p", "PROJ"]);
    assert_eq!(out.ok().trim(), "https://bitbucket.org/ws/new");
    assert_eq!(
        m.one("POST", "/repositories/ws/new").json(),
        json!({"scm": "git", "is_private": false, "description": "Fresh", "project": {"key": "PROJ"}})
    );
}

#[test]
fn repo_delete_needs_yes() {
    let m = Mock::start(vec![route("DELETE", "/repositories/ws/r", 204, "")]);
    let err = m.bb(&["repo", "delete", "ws/r"]).fails(1).to_string();
    assert!(err.contains("refusing to delete ws/r without --yes"), "{err}");
    assert!(m.requests().is_empty());
    assert_eq!(m.bb(&["repo", "delete", "ws/r", "--yes"]).ok().trim(), "Deleted ws/r");
    assert_eq!(m.lines(), ["DELETE /repositories/ws/r"]);
}

#[test]
fn workspace_list() {
    let m = Mock::start(vec![json(
        "GET",
        "/user/workspaces?sort=slug&pagelen=30&administrator=true",
        json!({"values": [{"administrator": true, "workspace": {"slug": "mine"}}, {"administrator": false, "workspace": {"slug": "theirs"}}]}),
    )]);
    let stdout = m.bb(&["workspace", "list", "--admin"]).ok().to_string();
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines[0].starts_with("SLUG"));
    assert!(lines[1].contains("mine") && lines[1].contains("admin"));
    assert!(lines[2].contains("theirs") && lines[2].contains("member"));
}

#[test]
fn workspace_list_without_scope_explains_the_fix() {
    let m = Mock::start(vec![route("GET", "/user/workspaces", 403, r#"{"error":{"message":"Forbidden"}}"#)]);
    let err = m.bb(&["ws", "list"]).fails(1).to_string();
    assert!(err.contains("read:workspace:bitbucket") && err.contains("HTTP 403: Forbidden"), "{err}");
}

#[test]
fn api_get_with_query_fields() {
    let m = Mock::start(vec![json("GET", "/user", json!({"nickname": "me"}))]);
    let v = m.bb(&["api", "/user", "-X", "get", "-f", "fields=nickname"]).json();
    assert_eq!(v["nickname"], "me");
    assert_eq!(m.lines(), ["GET /user?fields=nickname"]);
}

#[test]
fn api_post_typed_fields_and_headers() {
    let m = Mock::start(vec![route("POST", "/things", 201, r#"{"id":1}"#)]);
    let out = m.bb(&["api", "things", "-F", "n=3", "-F", "on=true", "-f", "name=x", "-H", "X-Test: yes", "-i"]);
    let stdout = out.ok();
    assert!(stdout.starts_with("HTTP/1.1 201"), "{stdout}");
    let req = m.one("POST", "/things");
    assert_eq!(req.json(), json!({"n": 3, "on": true, "name": "x"}));
    assert_eq!(req.header("x-test"), Some("yes"));
    assert_eq!(req.header("content-type"), Some("application/json"));
}

#[test]
fn api_paginate_concatenates_values() {
    let m = Mock::start(vec![
        json("GET", "/items?page=1", json!({"values": [1, 2], "next": "{base}/items?page=2"})),
        json("GET", "/items?page=2", json!({"values": [3]})),
    ]);
    assert_eq!(m.bb(&["api", "/items?page=1", "--paginate"]).json(), json!([1, 2, 3]));
}

#[test]
fn api_error_prints_body_and_fails() {
    let m = Mock::start(vec![route("GET", "/nope", 404, r#"{"error":{"message":"Not here"}}"#)]);
    let out = m.bb(&["api", "/nope"]);
    let err = out.fails(1);
    assert!(err.contains("HTTP 404: Not here"), "{err}");
    assert!(out.stdout.contains("Not here"), "the body is still printed: {}", out.stdout);
}

#[test]
fn api_plain_text_body_and_verbose_hides_auth() {
    let m = Mock::start(vec![text("GET", "/raw", "plain body\n")]);
    let out = m.bb(&["api", "/raw", "--verbose"]);
    assert_eq!(out.ok(), "plain body\n");
    assert!(out.stderr.contains("> GET ") && out.stderr.contains("< HTTP/1.1 200"), "{}", out.stderr);
    assert!(!out.stderr.to_ascii_lowercase().contains("authorization"), "{}", out.stderr);
}
