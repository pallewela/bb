//! `bb project`, `bb repo list --project` and `bb repo edit` end to end against a mock Bitbucket API.
mod common;

use common::{Mock, json, route};
use serde_json::json;

fn project() -> serde_json::Value {
    json!({
        "key": "NGPG",
        "name": "NGA-Playground",
        "description": "Try things",
        "is_private": true,
        "updated_on": "2026-03-04T05:06:07.000000+00:00",
        "links": {"html": {"href": "https://bitbucket.org/ws/workspace/projects/NGPG"}}
    })
}

#[test]
fn project_list_prints_a_table() {
    let m = Mock::start(vec![json(
        "GET",
        "/workspaces/ws/projects?sort=key&pagelen=30",
        json!({"values": [project(), {"key": "PUB", "name": "Public", "is_private": false}]}),
    )]);
    let stdout = m.bb(&["project", "ls", "--owner", "ws"]).ok().to_string();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert!(lines[0].starts_with("KEY") && lines[0].contains("NAME"));
    assert!(lines[1].contains("NGPG") && lines[1].contains("NGA-Playground") && lines[1].contains("private"));
    assert!(lines[2].contains("PUB") && lines[2].contains("public"));
}

#[test]
fn project_list_uses_the_repo_flags_workspace_or_needs_owner() {
    let m = Mock::start(vec![json("GET", "/workspaces/ws/projects", json!({"values": [project()]}))]);
    assert_eq!(m.bb(&["project", "list", "-R", "ws/r", "--json"]).json()[0]["key"], "NGPG");
    let err = m.bb(&["project", "list"]).fails(1).to_string();
    assert!(err.contains("--owner WORKSPACE"), "{err}");
}

#[test]
fn project_view() {
    let m = Mock::start(vec![json("GET", "/workspaces/ws/projects/NGPG", project())]);
    let stdout = m.bb(&["project", "view", "NGPG", "--owner", "ws"]).ok().to_string();
    for want in ["NGPG", "NGA-Playground", "Try things", "private", "https://bitbucket.org/ws/workspace/projects/NGPG"]
    {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
}

#[test]
fn project_create_is_private_by_default() {
    let m = Mock::start(vec![json("POST", "/workspaces/ws/projects", project())]);
    let out = m.bb(&["project", "create", "NGPG", "--title", "NGA-Playground", "--owner", "ws", "-d", "Try things"]);
    assert_eq!(out.ok().trim(), "https://bitbucket.org/ws/workspace/projects/NGPG");
    assert_eq!(
        m.one("POST", "/workspaces/ws/projects").json(),
        json!({"key": "NGPG", "name": "NGA-Playground", "is_private": true, "description": "Try things"})
    );
    m.bb(&["project", "create", "P2", "--title", "P2", "--owner", "ws", "--visibility", "PUBLIC"]).ok();
    assert_eq!(m.requests()[1].json()["is_private"], false);
}

#[test]
fn project_edit_keeps_unchanged_fields() {
    let m = Mock::start(vec![
        json("GET", "/workspaces/ws/projects/NGPG", project()),
        json("PUT", "/workspaces/ws/projects/NGPG", project()),
    ]);
    m.bb(&["project", "edit", "NGPG", "--owner", "ws", "--title", "Playground"]).ok();
    assert_eq!(
        m.one("PUT", "/workspaces/ws/projects/NGPG").json(),
        json!({"key": "NGPG", "name": "Playground", "description": "Try things", "is_private": true})
    );
}

#[test]
fn project_edit_of_a_missing_project_does_not_create_it() {
    let m =
        Mock::start(vec![route("GET", "/workspaces/ws/projects/NOPE", 404, r#"{"error":{"message":"Not found"}}"#)]);
    let err = m.bb(&["project", "edit", "NOPE", "--owner", "ws", "--title", "x"]).fails(1).to_string();
    assert!(err.contains("HTTP 404"), "{err}");
    assert_eq!(m.lines(), ["GET /workspaces/ws/projects/NOPE"]);
}

#[test]
fn project_edit_needs_a_change() {
    let m = Mock::start(vec![]);
    m.bb(&["project", "edit", "NGPG", "--owner", "ws"]).fails(2);
    assert!(m.requests().is_empty());
}

#[test]
fn project_delete_needs_yes() {
    let m = Mock::start(vec![route("DELETE", "/workspaces/ws/projects/NGPG", 204, "")]);
    let err = m.bb(&["project", "delete", "NGPG", "--owner", "ws"]).fails(1).to_string();
    assert!(err.contains("without --yes"), "{err}");
    assert!(m.requests().is_empty());
    assert_eq!(
        m.bb(&["project", "delete", "NGPG", "--owner", "ws", "--yes"]).ok().trim(),
        "Deleted project NGPG in ws"
    );
    assert_eq!(m.lines(), ["DELETE /workspaces/ws/projects/NGPG"]);
}

#[test]
fn repo_list_by_project() {
    let m = Mock::start(vec![json("GET", "/repositories/ws", json!({"values": []}))]);
    m.bb(&["repo", "list", "ws", "-p", "NGPG"]).ok();
    assert_eq!(m.lines(), ["GET /repositories/ws?sort=-updated_on&pagelen=30&q=project.key%20%3D%20%22NGPG%22"]);
}

#[test]
fn repo_edit_moves_to_a_project() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r", json!({"full_name": "ws/r"})),
        json("PUT", "/repositories/ws/r", json!({"full_name": "ws/r"})),
    ]);
    assert_eq!(m.bb(&["repo", "edit", "ws/r", "--project", "NGPG"]).ok().trim(), "Edited repository ws/r");
    assert_eq!(m.lines(), ["GET /repositories/ws/r?fields=full_name", "PUT /repositories/ws/r"]);
    assert_eq!(m.one("PUT", "/repositories/ws/r").json(), json!({"project": {"key": "NGPG"}}));
}

#[test]
fn repo_edit_of_a_missing_repo_does_not_create_it() {
    let m = Mock::start(vec![route("GET", "/repositories/ws/typo", 404, r#"{"error":{"message":"Not found"}}"#)]);
    m.bb(&["repo", "edit", "ws/typo", "-d", "x"]).fails(1);
    assert_eq!(m.lines(), ["GET /repositories/ws/typo?fields=full_name"]);
}

#[test]
fn repo_edit_visibility_needs_acceptance() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r", json!({"full_name": "ws/r"})),
        json("PUT", "/repositories/ws/r", json!({"full_name": "ws/r"})),
    ]);
    m.bb(&["repo", "edit", "ws/r", "--visibility", "public"]).fails(2);
    assert!(m.requests().is_empty());
    m.bb(&["repo", "edit", "ws/r", "--visibility", "public", "--accept-visibility-change-consequences"]).ok();
    assert_eq!(m.one("PUT", "/repositories/ws/r").json(), json!({"is_private": false}));
}
