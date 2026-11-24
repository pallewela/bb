//! `bb pipeline` and `bb workflow` end to end against a mock Bitbucket API.
mod common;

use common::{Mock, json, route, text};
use serde_json::{Value, json};

const BASE: &str = "/repositories/ws/r/pipelines";

fn pipeline(result: &str) -> Value {
    json!({
        "uuid": "{p-42}",
        "build_number": 42,
        "state": {"name": "COMPLETED", "result": {"name": result}},
        "target": {"ref_name": "main", "commit": {"hash": "abcdef1234567890"}},
        "trigger": {"name": "PUSH"},
        "creator": {"display_name": "Ada", "nickname": "ada", "uuid": "{u-ada}"},
        "created_on": "2026-05-06T07:08:09.000000+00:00",
        "duration_in_seconds": 125
    })
}

fn steps() -> Value {
    json!({"values": [
        {"uuid": "{s-1}", "name": "Build", "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}}, "duration_in_seconds": 30},
        {"uuid": "{s-2}", "name": "Test", "state": {"name": "COMPLETED", "result": {"name": "FAILED"}}, "duration_in_seconds": 3700}
    ]})
}

const STEPS: &str = "/repositories/ws/r/pipelines/%7Bp-42%7D/steps/";

#[test]
fn list_prints_a_table_with_filters() {
    let m = Mock::start(vec![json("GET", &format!("{BASE}/"), json!({"values": [pipeline("SUCCESSFUL")]}))]);
    let stdout = m
        .bb(&["pipeline", "list", "-R", "ws/r", "-b", "main", "-s", "completed", "-e", "manual", "-w", "deploy"])
        .ok()
        .to_string();
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines[0].starts_with('#') && lines[0].contains("STATUS") && lines[0].contains("CREATOR"), "{stdout}");
    assert!(
        lines[1].contains("#42")
            && lines[1].contains("SUCCESSFUL")
            && lines[1].contains("main")
            && lines[1].contains("Ada"),
        "{stdout}"
    );
    assert_eq!(
        m.lines(),
        [format!(
            "GET {BASE}/?sort=-created_on&pagelen=20&target.branch=main&status=PASSED&status=FAILED&status=ERROR&status=STOPPED\
             &target.selector.pattern=deploy&trigger_type=MANUAL"
        )]
    );
}

#[test]
fn list_resolves_short_commits_and_user_names() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r/commit/abc123", json!({"hash": "abc123full"})),
        json("GET", &format!("{BASE}/?sort=-created_on&pagelen=100"), json!({"values": [pipeline("SUCCESSFUL")]})),
        json("GET", &format!("{BASE}/"), json!({"values": []})),
    ]);
    let v = m.bb(&["run", "list", "-R", "ws/r", "-c", "abc123", "-u", "Ada", "--json"]).json();
    assert_eq!(v, json!([]));
    let last = m.lines().pop().unwrap();
    assert!(last.ends_with("&target.commit.hash=abc123full&creator.uuid=%7Bu-ada%7D"), "{last}");
}

#[test]
fn list_rejects_unknown_status() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["pipeline", "list", "-R", "ws/r", "-s", "stale"]).fails(1).to_string();
    assert!(err.contains("unsupported --status `stale`"), "{err}");
}

#[test]
fn view_shows_steps() {
    let m = Mock::start(vec![json("GET", &format!("{BASE}/42"), pipeline("FAILED")), json("GET", STEPS, steps())]);
    let stdout = m.bb(&["pipeline", "view", "42", "-R", "ws/r"]).ok().to_string();
    for want in [
        "Pipeline #42  FAILED",
        "Ref:      main",
        "Commit:   abcdef1234567890",
        "Duration: 2m 5s",
        "https://bitbucket.org/ws/r/pipelines/results/42",
        "Build",
        "30s",
        "1h 1m 40s",
    ] {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
}

#[test]
fn view_exit_status_fails_for_a_failed_pipeline() {
    let m = Mock::start(vec![json("GET", &format!("{BASE}/42"), pipeline("FAILED")), json("GET", STEPS, steps())]);
    let err = m.bb(&["pipeline", "view", "#42", "-R", "ws/r", "--exit-status", "--json"]).fails(1).to_string();
    assert!(err.contains("pipeline #42 FAILED"), "{err}");
}

#[test]
fn view_by_uuid_and_unknown_job() {
    let m = Mock::start(vec![
        json("GET", &format!("{BASE}/%7Bp-42%7D"), pipeline("SUCCESSFUL")),
        json("GET", STEPS, steps()),
    ]);
    let err = m.bb(&["pipeline", "view", "p-42", "-R", "ws/r", "-j", "deploy"]).fails(1).to_string();
    assert!(err.contains("no step `deploy` in pipeline #42"), "{err}");
}

#[test]
fn view_log_failed_prints_only_failed_step_logs() {
    let m = Mock::start(vec![
        json("GET", &format!("{BASE}/42"), pipeline("FAILED")),
        json("GET", STEPS, steps()),
        text("GET", &format!("{STEPS}%7Bs-2%7D/log"), "test output\n"),
    ]);
    let out = m.bb(&["pipeline", "view", "42", "-R", "ws/r", "--log-failed"]);
    assert_eq!(out.ok(), "test output\n");
    assert!(out.stderr.contains("==> Test <=="), "{}", out.stderr);
    assert!(!m.lines().iter().any(|l| l.contains("s-1")), "the passing step's log is not fetched");
}

#[test]
fn logs_for_a_step_without_a_log_yet() {
    let m = Mock::start(vec![
        json("GET", &format!("{BASE}/42"), pipeline("FAILED")),
        json("GET", STEPS, steps()),
        route("GET", &format!("{STEPS}%7Bs-1%7D/log"), 404, "{}"),
    ]);
    let out = m.bb(&["pipeline", "logs", "42", "-R", "ws/r", "--step", "build"]);
    out.ok();
    assert!(out.stderr.contains("(no log available yet)"), "{}", out.stderr);
}

#[test]
fn watch_returns_once_finished() {
    let m = Mock::start(vec![json("GET", &format!("{BASE}/42"), pipeline("SUCCESSFUL")), json("GET", STEPS, steps())]);
    let stdout = m.bb(&["pipeline", "watch", "42", "-R", "ws/r", "--exit-status"]).ok().to_string();
    assert!(stdout.contains("Build") && stdout.contains("Test"), "{stdout}");
}

#[test]
fn run_posts_variables() {
    let m = Mock::start(vec![json("POST", &format!("{BASE}/"), json!({"build_number": 43}))]);
    let out = m.bb(&[
        "pipeline",
        "run",
        "-R",
        "ws/r",
        "--ref",
        "release",
        "--pipeline",
        "deploy",
        "-f",
        "ENV=prod",
        "-F",
        "N=1",
    ]);
    assert_eq!(out.ok(), "Started pipeline #43\nhttps://bitbucket.org/ws/r/pipelines/results/43\n");
    assert_eq!(
        m.one("POST", &format!("{BASE}/")).json(),
        json!({
            "target": {"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": "release",
                       "selector": {"type": "custom", "pattern": "deploy"}},
            "variables": [{"key": "ENV", "value": "prod"}, {"key": "N", "value": "1"}]
        })
    );
}

#[test]
fn workflow_run_defaults_to_the_main_branch() {
    let m = Mock::start(vec![
        json("GET", "/repositories/ws/r", json!({"mainbranch": {"name": "trunk"}})),
        json("POST", &format!("{BASE}/"), json!({"build_number": 44})),
    ]);
    let v = m.bb(&["workflow", "run", "nightly", "-R", "ws/r", "--json"]).json();
    assert_eq!(v["build_number"], 44);
    let body = m.one("POST", &format!("{BASE}/")).json();
    assert_eq!(body["target"]["ref_name"], "trunk");
    assert_eq!(body["target"]["selector"]["pattern"], "nightly");
}

#[test]
fn cancel_stops_the_pipeline() {
    let m = Mock::start(vec![
        json("GET", &format!("{BASE}/42"), pipeline("SUCCESSFUL")),
        route("POST", &format!("{BASE}/%7Bp-42%7D/stopPipeline"), 204, ""),
    ]);
    assert_eq!(m.bb(&["pipeline", "cancel", "42", "-R", "ws/r"]).ok().trim(), "Stopping pipeline #42");
    m.one("POST", &format!("{BASE}/%7Bp-42%7D/stopPipeline"));
}

#[test]
fn unknown_pipeline_error_names_it() {
    let m = Mock::start(vec![]);
    let err = m.bb(&["pipeline", "cancel", "99", "-R", "ws/r"]).fails(1).to_string();
    assert!(err.contains("pipeline 99") && err.contains("HTTP 404"), "{err}");
}
