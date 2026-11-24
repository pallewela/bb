//! `bb pipeline` (alias `bb run`): list, view, watch, run, cancel and read logs of Bitbucket Pipelines.
//! Also `bb workflow run`, where a custom pipeline plays the part of a `gh` workflow.
use crate::client::{Client, enc, limit_arg};
use crate::context::{Repo, current_branch};
use crate::output::{json_mode, more_note, open_browser, print_json, s, short_time, table};
use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum Cmd {
    /// List recent pipelines
    #[command(visible_alias = "ls")]
    List {
        /// Only pipelines for this branch (including pull request pipelines from it)
        #[arg(short, long)]
        branch: Option<String>,
        /// Only pipelines with this status: queued|pending|requested|in_progress|waiting|action_required|completed|success|failure|cancelled|startup_failure, or Bitbucket's successful|failed|error|stopped|running|paused|halted|parsing
        #[arg(short, long)]
        status: Option<String>,
        /// Only pipelines started by this user (nickname, display name, account ID, UUID or @me)
        #[arg(short, long)]
        user: Option<String>,
        /// Only pipelines for this commit (full or abbreviated SHA)
        #[arg(short, long, value_name = "SHA")]
        commit: Option<String>,
        /// Only runs of this pipeline (custom pipeline name or branch pattern)
        #[arg(short, long, value_name = "NAME")]
        workflow: Option<String>,
        /// Only pipelines with this trigger: push|pull_request|workflow_dispatch (= manual)|schedule|parent_step
        #[arg(short, long)]
        event: Option<String>,
        /// Maximum number of pipelines to list [default: 20; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show a pipeline and its steps, or its logs
    View {
        /// Build number or UUID (default: the latest pipeline for the current branch)
        id: Option<String>,
        /// Only this step (name, case-insensitive, or UUID)
        #[arg(short, long, value_name = "STEP")]
        job: Option<String>,
        /// Print the full log (of all steps, or of --job)
        #[arg(long)]
        log: bool,
        /// Print the log of failed steps only
        #[arg(long, conflicts_with = "log")]
        log_failed: bool,
        /// Exit with non-zero status if the pipeline failed
        #[arg(long)]
        exit_status: bool,
        /// Steps are always shown; accepted for gh compatibility
        #[arg(short, long)]
        verbose: bool,
        /// Open the pipeline in the browser
        #[arg(short, long)]
        web: bool,
    },
    /// Watch a pipeline until it finishes
    Watch {
        /// Build number or UUID (default: the latest pipeline for the current branch)
        id: Option<String>,
        /// Exit with non-zero status if the pipeline fails
        #[arg(long)]
        exit_status: bool,
        /// Refresh interval in seconds
        #[arg(short, long, value_name = "N", default_value_t = 3)]
        interval: u64,
    },
    /// Trigger a pipeline on a branch
    Run {
        #[command(flatten)]
        trigger: Trigger,
        /// Custom pipeline name (from bitbucket-pipelines.yml `custom:`)
        #[arg(long, value_name = "NAME")]
        pipeline: Option<String>,
    },
    /// Cancel (stop) a running pipeline
    #[command(visible_alias = "stop")]
    Cancel {
        /// Build number or UUID
        id: String,
    },
    /// Print step logs
    Logs {
        /// Build number or UUID (default: the latest pipeline for the current branch)
        id: Option<String>,
        /// Only this step (name, case-insensitive, or UUID)
        #[arg(long, value_name = "NAME|UUID")]
        step: Option<String>,
        /// Only failed steps
        #[arg(long)]
        failed: bool,
    },
}

/// `bb workflow`: the `gh workflow` analog. A workflow is a custom pipeline.
#[derive(Subcommand)]
pub enum WorkflowCmd {
    /// Run a custom pipeline (or, with no NAME, the default pipeline for the ref)
    Run {
        /// Custom pipeline name (from bitbucket-pipelines.yml `custom:`)
        name: Option<String>,
        #[command(flatten)]
        trigger: Trigger,
    },
}

/// Where and with what to run a pipeline; shared by `pipeline run` and `workflow run`.
#[derive(Args)]
pub struct Trigger {
    /// Branch to run on (pipeline run: default is the current branch; workflow run: the main branch)
    #[arg(short = 'r', long = "ref", visible_alias = "branch", value_name = "BRANCH")]
    r#ref: Option<String>,
    /// Pipeline variable KEY=VALUE (repeatable)
    #[arg(short = 'f', long = "raw-field", visible_alias = "var", value_name = "KEY=VALUE")]
    raw_fields: Vec<String>,
    /// Pipeline variable KEY=VALUE; VALUE `@file` reads a file, `@-` stdin (repeatable)
    #[arg(short = 'F', long = "field", value_name = "KEY=VALUE")]
    fields: Vec<String>,
}

pub fn run(cmd: Cmd) -> Result<()> {
    let repo = Repo::current()?;
    let c = Client::new()?;
    let base = format!("{}/pipelines", repo.api());
    match cmd {
        Cmd::List { branch, status: state, user, commit, workflow, event, limit } => {
            let (limit, chosen) = limit_arg(limit, 20);
            let commit = match commit {
                // The API only matches full hashes, so expand abbreviated ones first.
                Some(sha) if sha.len() < 40 => {
                    Some(s(&c.get(&format!("{}/commit/{}", repo.api(), enc(&sha)))?, "hash"))
                }
                other => other,
            };
            let user_uuid = match user.as_deref().map(user_filter) {
                Some(UserFilter::Me) => Some(s(&c.get("/user")?, "uuid")),
                Some(UserFilter::Uuid(u)) => Some(u),
                Some(UserFilter::Name(n)) => Some(creator_uuid(&c, &base, &n)?),
                None => None,
            };
            let filters = Filters { branch, status: state, commit, workflow, event, user_uuid };
            let list = c.list(&list_path(&base, limit.clamp(1, 100), &filters)?, limit)?;
            if json_mode() {
                print_json(&Value::Array(list.items));
                return Ok(());
            }
            let mut rows = vec![["#", "STATUS", "REF", "TRIGGER", "CREATOR", "CREATED"].map(String::from).to_vec()];
            rows.extend(list.items.iter().map(|p| {
                vec![
                    format!("#{}", s(p, "build_number")),
                    status(p),
                    ref_name(p),
                    s(p, "trigger.name"),
                    s(p, "creator.display_name"),
                    short_time(&s(p, "created_on")),
                ]
            }));
            table(&rows);
            if !chosen {
                more_note(&list, "pipelines", "newest first");
            }
        }
        Cmd::View { id, job, log, log_failed, exit_status, verbose: _, web } => {
            let p = pick(&c, &base, id.as_deref())?;
            let url = web_url(&repo, &p);
            if web {
                return open_browser(&url);
            }
            if log || log_failed {
                print_logs(&c, &base, &p, job.as_deref(), log_failed)?;
                return check_exit(exit_status, &p);
            }
            let steps: Vec<Value> = c
                .paginate(&format!("{base}/{}/steps/", enc(&s(&p, "uuid"))), usize::MAX)?
                .into_iter()
                .filter(|st| job.as_deref().is_none_or(|j| step_matches(st, j)))
                .collect();
            if let (Some(j), true) = (&job, steps.is_empty()) {
                bail!("no step `{j}` in pipeline #{}", s(&p, "build_number"));
            }
            if json_mode() {
                print_json(&json!({"pipeline": p, "steps": steps}));
                return check_exit(exit_status, &p);
            }
            println!("Pipeline #{}  {}", s(&p, "build_number"), status(&p));
            println!("Ref:      {}", ref_name(&p));
            println!("Commit:   {}", s(&p, "target.commit.hash"));
            println!("Creator:  {}", s(&p, "creator.display_name"));
            println!("Created:  {}", short_time(&s(&p, "created_on")));
            println!("Duration: {}", duration(&p["duration_in_seconds"]));
            println!("URL:      {url}\n");
            let mut rows = vec![["STEP", "STATUS", "DURATION"].map(String::from).to_vec()];
            rows.extend(steps.iter().map(|st| vec![s(st, "name"), status(st), duration(&st["duration_in_seconds"])]));
            table(&rows);
            check_exit(exit_status, &p)?;
        }
        Cmd::Watch { id, exit_status, interval } => {
            let mut p = pick(&c, &base, id.as_deref())?;
            let path = format!("{base}/{}", enc(&s(&p, "uuid")));
            let mut seen = HashMap::new();
            loop {
                if !json_mode() {
                    let steps = c.paginate(&format!("{path}/steps/"), usize::MAX)?;
                    for line in changes(&mut seen, &p, &steps) {
                        println!("{line}");
                    }
                }
                if finished(&p) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
                p = c.get(&path)?;
            }
            if json_mode() {
                print_json(&p);
            } else if s(&p, "state.name") != "COMPLETED" {
                eprintln!(
                    "Pipeline #{} is {}: it needs a manual action to continue.",
                    s(&p, "build_number"),
                    status(&p)
                );
            }
            check_exit(exit_status, &p)?;
        }
        Cmd::Run { trigger, pipeline } => {
            let branch = match &trigger.r#ref {
                Some(b) => b.clone(),
                None => current_branch()?,
            };
            start(&c, &repo, &branch, pipeline.as_deref(), &trigger)?;
        }
        Cmd::Cancel { id } => {
            let p = resolve(&c, &base, &id)?;
            c.post(&format!("{base}/{}/stopPipeline", enc(&s(&p, "uuid"))), None)?;
            println!("Stopping pipeline #{}", s(&p, "build_number"));
        }
        Cmd::Logs { id, step, failed } => {
            let p = pick(&c, &base, id.as_deref())?;
            print_logs(&c, &base, &p, step.as_deref(), failed)?;
        }
    }
    Ok(())
}

/// `bb workflow ...`
pub fn workflow(cmd: WorkflowCmd) -> Result<()> {
    let repo = Repo::current()?;
    let c = Client::new()?;
    match cmd {
        WorkflowCmd::Run { name, trigger } => {
            // Like `gh workflow run`, the ref defaults to the repository's default branch.
            let branch = match &trigger.r#ref {
                Some(b) => b.clone(),
                None => s(&c.get(&repo.api())?, "mainbranch.name"),
            };
            if branch.is_empty() {
                bail!("repository has no main branch; pass --ref BRANCH");
            }
            start(&c, &repo, &branch, name.as_deref(), &trigger)
        }
    }
}

/// POST a new pipeline run and report it.
fn start(c: &Client, repo: &Repo, branch: &str, pipeline: Option<&str>, t: &Trigger) -> Result<()> {
    let mut vars = t.raw_fields.iter().map(|kv| split_var(kv)).collect::<Result<Vec<_>>>()?;
    for kv in &t.fields {
        let (k, v) = split_var(kv)?;
        vars.push((k, field_value(&v)?));
    }
    let p = c.post(&format!("{}/pipelines/", repo.api()), Some(&run_body(branch, pipeline, &vars)))?;
    if json_mode() {
        print_json(&p);
    } else {
        println!("Started pipeline #{}\n{}", s(&p, "build_number"), web_url(repo, &p));
    }
    Ok(())
}

/// Print the logs of the matching steps of pipeline `p`.
fn print_logs(c: &Client, base: &str, p: &Value, step: Option<&str>, failed: bool) -> Result<()> {
    let steps_path = format!("{base}/{}/steps/", enc(&s(p, "uuid")));
    let steps: Vec<Value> = c
        .paginate(&steps_path, usize::MAX)?
        .into_iter()
        .filter(|st| step.is_none_or(|want| step_matches(st, want)))
        .filter(|st| !failed || matches!(s(st, "state.result.name").as_str(), "FAILED" | "ERROR"))
        .collect();
    if steps.is_empty() {
        bail!("no matching steps in pipeline #{}", s(p, "build_number"));
    }
    for st in &steps {
        eprintln!("==> {} <==", s(st, "name"));
        match c.get_text(&format!("{steps_path}{}/log", enc(&s(st, "uuid")))) {
            Ok(log) => print!("{log}"),
            Err(e) if e.to_string().starts_with("HTTP 404") => eprintln!("(no log available yet)"),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// A step by name (case-insensitive) or UUID (braces optional).
fn step_matches(st: &Value, want: &str) -> bool {
    s(st, "name").eq_ignore_ascii_case(want.trim()) || s(st, "uuid") == brace(want)
}

/// The pipeline given by `id`, or the latest one for the current git branch.
fn pick(c: &Client, base: &str, id: Option<&str>) -> Result<Value> {
    if let Some(id) = id {
        return resolve(c, base, id);
    }
    let branch = current_branch().context("no pipeline given")?;
    let page = c.get(&format!("{base}/?sort=-created_on&pagelen=1&target.branch={}", enc(&branch)))?;
    match page["values"].get(0) {
        Some(p) => Ok(p.clone()),
        None => bail!("no pipelines for branch `{branch}`"),
    }
}

/// Fetch a pipeline by build number (all digits) or UUID (braces optional).
fn resolve(c: &Client, base: &str, id: &str) -> Result<Value> {
    c.get(&pipeline_path(base, id)).with_context(|| format!("pipeline {id}"))
}

/// GET /pipelines/{id} takes the build number in place of the UUID (the list endpoint ignores `q`).
fn pipeline_path(base: &str, id: &str) -> String {
    let id = id.trim().trim_start_matches('#');
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        format!("{base}/{id}")
    } else {
        format!("{base}/{}", enc(&brace(id)))
    }
}

/// `abc` or `{abc}` -> `{abc}`
fn brace(id: &str) -> String {
    format!("{{{}}}", id.trim().trim_start_matches('{').trim_end_matches('}'))
}

/// `run list` filters that the API applies server-side.
#[derive(Default)]
struct Filters {
    branch: Option<String>,
    status: Option<String>,
    commit: Option<String>,
    workflow: Option<String>,
    event: Option<String>,
    user_uuid: Option<String>,
}

/// GET path for `run list`. Every filter here is a documented query parameter of
/// GET /pipelines (checked live; repeated `status`/`trigger_type` values are OR-ed).
fn list_path(base: &str, pagelen: usize, f: &Filters) -> Result<String> {
    let mut path = format!("{base}/?sort=-created_on&pagelen={pagelen}");
    let mut add = |k: &str, v: &str| path += &format!("&{k}={}", enc(v));
    // target.branch also matches pull request pipelines whose source is the branch (gh's head branch);
    // target.ref_name would not.
    if let Some(b) = &f.branch {
        add("target.branch", b);
    }
    if let Some(st) = &f.status {
        status_values(st)?.iter().for_each(|v| add("status", v));
    }
    if let Some(h) = &f.commit {
        add("target.commit.hash", h);
    }
    if let Some(w) = &f.workflow {
        add("target.selector.pattern", w);
    }
    if let Some(e) = &f.event {
        let (k, v) = event_param(e)?;
        add(k, v);
    }
    if let Some(u) = &f.user_uuid {
        add("creator.uuid", u);
    }
    Ok(path)
}

/// Map a `--status` value (gh's or Bitbucket's) to the API's `status` filter values.
fn status_values(st: &str) -> Result<&'static [&'static str]> {
    Ok(match st.to_ascii_lowercase().replace('-', "_").as_str() {
        "queued" | "pending" | "requested" => &["PENDING", "PARSING"],
        "in_progress" => &["BUILDING", "PAUSED", "HALTED"],
        "waiting" | "action_required" => &["PAUSED", "HALTED"],
        "completed" => &["PASSED", "FAILED", "ERROR", "STOPPED"],
        "success" | "successful" | "passed" => &["PASSED"],
        "failure" | "failed" => &["FAILED"],
        "cancelled" | "stopped" => &["STOPPED"],
        "startup_failure" | "error" => &["ERROR"],
        "running" | "building" => &["BUILDING"],
        "paused" => &["PAUSED"],
        "halted" => &["HALTED"],
        "parsing" => &["PARSING"],
        _ => bail!("unsupported --status `{st}` (neutral, skipped, stale and timed_out have no Bitbucket equivalent)"),
    })
}

/// Map an `--event` value (gh's or Bitbucket's trigger name) to an API filter parameter.
fn event_param(e: &str) -> Result<(&'static str, &'static str)> {
    Ok(match e.to_ascii_lowercase().replace('-', "_").as_str() {
        "push" => ("trigger_type", "PUSH"),
        "workflow_dispatch" | "manual" => ("trigger_type", "MANUAL"),
        "schedule" | "scheduled" => ("trigger_type", "SCHEDULED"),
        "parent_step" => ("trigger_type", "PARENT_STEP"),
        // Pull request pipelines are PUSH-triggered; their selector type tells them apart.
        "pull_request" | "pull_requests" | "pullrequests" => ("target.selector.type", "PULLREQUESTS"),
        _ => bail!("unsupported --event `{e}` (use push, pull_request, workflow_dispatch, schedule or parent_step)"),
    })
}

#[derive(Debug, PartialEq)]
enum UserFilter {
    Me,
    Uuid(String),
    Name(String),
}

/// `@me`, a UUID (braces optional) or anything else (matched against the creator's names).
fn user_filter(u: &str) -> UserFilter {
    let bare = u.trim().trim_start_matches('{').trim_end_matches('}');
    let is_uuid = bare.len() == 36
        && bare
            .chars()
            .enumerate()
            .all(|(i, ch)| if [8, 13, 18, 23].contains(&i) { ch == '-' } else { ch.is_ascii_hexdigit() });
    match u.trim() {
        "@me" => UserFilter::Me,
        _ if is_uuid => UserFilter::Uuid(brace(bare)),
        other => UserFilter::Name(other.to_string()),
    }
}

fn creator_is(p: &Value, name: &str) -> bool {
    ["creator.nickname", "creator.display_name", "creator.account_id"]
        .iter()
        .any(|k| s(p, k).eq_ignore_ascii_case(name))
}

/// The API filters by creator UUID only, so look a user name up among recent pipelines (client-side).
fn creator_uuid(c: &Client, base: &str, name: &str) -> Result<String> {
    const PAGES: usize = 5;
    let mut next = Some(format!("{base}/?sort=-created_on&pagelen=100"));
    for _ in 0..PAGES {
        let Some(path) = next else { break };
        let page = c.get(&path)?;
        if let Some(p) = page["values"].as_array().into_iter().flatten().find(|p| creator_is(p, name)) {
            return Ok(s(p, "creator.uuid"));
        }
        next = page["next"].as_str().map(String::from);
    }
    bail!("no pipelines by `{name}` among the {} most recent; pass the user's UUID to -u", PAGES * 100)
}

/// Result (SUCCESSFUL/FAILED/...) beats stage (RUNNING/PAUSED) beats state (PENDING/IN_PROGRESS/...).
fn status(v: &Value) -> String {
    ["state.result.name", "state.stage.name", "state.name"]
        .iter()
        .map(|k| s(v, k))
        .find(|x| !x.is_empty())
        .unwrap_or_default()
}

/// Done, or waiting for a manual action (PAUSED/HALTED) that watching won't bring about.
fn finished(p: &Value) -> bool {
    s(p, "state.name") == "COMPLETED" || matches!(s(p, "state.stage.name").as_str(), "PAUSED" | "HALTED")
}

/// With `--exit-status`, fail unless the pipeline completed successfully (or is still going).
fn check_exit(exit_status: bool, p: &Value) -> Result<()> {
    let r = s(p, "state.result.name");
    if exit_status && !r.is_empty() && r != "SUCCESSFUL" {
        bail!("pipeline #{} {r}", s(p, "build_number"));
    }
    Ok(())
}

/// Status lines for the pipeline and steps whose status changed since the last call.
fn changes(seen: &mut HashMap<String, String>, p: &Value, steps: &[Value]) -> Vec<String> {
    let pipeline = (String::new(), format!("Pipeline #{}", s(p, "build_number")), status(p));
    let steps = steps.iter().map(|st| (s(st, "uuid"), format!("  {}", s(st, "name")), status(st)));
    std::iter::once(pipeline)
        .chain(steps)
        .filter(|(key, _, st)| seen.insert(key.clone(), st.clone()).as_ref() != Some(st))
        .map(|(_, label, st)| format!("{label}: {st}"))
        .collect()
}

/// Branch (or PR source branch) of a pipeline, else its short commit hash.
fn ref_name(p: &Value) -> String {
    let r = [s(p, "target.ref_name"), s(p, "target.source")].into_iter().find(|r| !r.is_empty());
    r.unwrap_or_else(|| s(p, "target.commit.hash").chars().take(7).collect())
}

fn web_url(repo: &Repo, p: &Value) -> String {
    format!("{}/pipelines/results/{}", repo.web(), s(p, "build_number"))
}

fn duration(v: &Value) -> String {
    let Some(t) = v.as_u64() else { return String::new() };
    match (t / 3600, t / 60 % 60, t % 60) {
        (0, 0, sec) => format!("{sec}s"),
        (0, m, sec) => format!("{m}m {sec}s"),
        (h, m, sec) => format!("{h}h {m}m {sec}s"),
    }
}

fn split_var(kv: &str) -> Result<(String, String)> {
    match kv.split_once('=') {
        Some((k, v)) => Ok((k.to_string(), v.to_string())),
        None => bail!("variable `{kv}` must be KEY=VALUE"),
    }
}

/// `-F` value: `@-` reads stdin, `@path` reads a file, anything else is literal.
fn field_value(v: &str) -> Result<String> {
    match v.strip_prefix('@') {
        Some("-") => std::io::read_to_string(std::io::stdin()).context("reading stdin"),
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("reading {path}")),
        None => Ok(v.to_string()),
    }
}

fn run_body(branch: &str, pipeline: Option<&str>, vars: &[(String, String)]) -> Value {
    let mut target = json!({"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": branch});
    if let Some(name) = pipeline {
        target["selector"] = json!({"type": "custom", "pattern": name});
    }
    let mut body = json!({ "target": target });
    if !vars.is_empty() {
        body["variables"] = vars.iter().map(|(k, v)| json!({"key": k, "value": v})).collect();
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brace_and_status() {
        assert_eq!(brace("abc-1"), "{abc-1}");
        assert_eq!(brace("{abc-1}"), "{abc-1}");
        assert_eq!(enc(&brace("a")), "%7Ba%7D");
        assert_eq!(pipeline_path("/p", "194"), "/p/194");
        assert_eq!(pipeline_path("/p", "#194"), "/p/194");
        assert_eq!(pipeline_path("/p", "ab-1"), "/p/%7Bab-1%7D");
        let done = json!({"state": {"name": "COMPLETED", "result": {"name": "FAILED"}}});
        let running = json!({"state": {"name": "IN_PROGRESS", "stage": {"name": "RUNNING"}}});
        assert_eq!(status(&done), "FAILED");
        assert_eq!(status(&running), "RUNNING");
        assert_eq!(status(&json!({"state": {"name": "PENDING"}})), "PENDING");
        assert_eq!(ref_name(&json!({"target": {"commit": {"hash": "0123456789"}}})), "0123456");
        assert_eq!(ref_name(&json!({"target": {"source": "feat", "commit": {"hash": "0123456789"}}})), "feat");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(&json!(45)), "45s");
        assert_eq!(duration(&json!(125)), "2m 5s");
        assert_eq!(duration(&json!(3725)), "1h 2m 5s");
        assert_eq!(duration(&Value::Null), "");
    }

    #[test]
    fn run_bodies() {
        let vars = vec![split_var("A=1").unwrap(), split_var("B=x=y").unwrap()];
        assert_eq!(
            run_body("main", Some("deploy"), &vars),
            json!({
                "target": {"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": "main",
                           "selector": {"type": "custom", "pattern": "deploy"}},
                "variables": [{"key": "A", "value": "1"}, {"key": "B", "value": "x=y"}]
            })
        );
        // No NAME (`workflow run` without one): the branch's default pipeline, no selector.
        assert_eq!(
            run_body("dev", None, &[]),
            json!({"target": {"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": "dev"}})
        );
        assert!(split_var("NOEQ").is_err());
    }

    #[test]
    fn field_values() {
        assert_eq!(field_value("plain").unwrap(), "plain");
        let f = std::env::temp_dir().join(format!("bb-field-{}", std::process::id()));
        std::fs::write(&f, "from file").unwrap();
        assert_eq!(field_value(&format!("@{}", f.display())).unwrap(), "from file");
        std::fs::remove_file(&f).unwrap();
        assert!(field_value("@/no/such/file").is_err());
    }

    #[test]
    fn list_paths() {
        let base = "/repositories/w/r/pipelines";
        assert_eq!(list_path(base, 20, &Filters::default()).unwrap(), format!("{base}/?sort=-created_on&pagelen=20"));
        let f = Filters {
            branch: Some("feat/x".into()),
            status: Some("completed".into()),
            commit: Some("abc".into()),
            workflow: Some("deploy".into()),
            event: Some("pull_request".into()),
            user_uuid: Some("{u}".into()),
        };
        assert_eq!(
            list_path(base, 5, &f).unwrap(),
            format!(
                "{base}/?sort=-created_on&pagelen=5&target.branch=feat%2Fx&status=PASSED&status=FAILED&status=ERROR\
                 &status=STOPPED&target.commit.hash=abc&target.selector.pattern=deploy\
                 &target.selector.type=PULLREQUESTS&creator.uuid=%7Bu%7D"
            )
        );
        let bad = Filters { status: Some("neutral".into()), ..Default::default() };
        assert!(list_path(base, 5, &bad).is_err());
    }

    #[test]
    fn status_and_event_mapping() {
        assert_eq!(status_values("success").unwrap(), ["PASSED"]);
        assert_eq!(status_values("SUCCESSFUL").unwrap(), ["PASSED"]);
        assert_eq!(status_values("failure").unwrap(), ["FAILED"]);
        assert_eq!(status_values("cancelled").unwrap(), ["STOPPED"]);
        assert_eq!(status_values("in_progress").unwrap(), ["BUILDING", "PAUSED", "HALTED"]);
        assert_eq!(status_values("in-progress").unwrap(), ["BUILDING", "PAUSED", "HALTED"]);
        assert!(status_values("timed_out").is_err());
        assert_eq!(event_param("push").unwrap(), ("trigger_type", "PUSH"));
        assert_eq!(event_param("workflow_dispatch").unwrap(), ("trigger_type", "MANUAL"));
        assert_eq!(event_param("MANUAL").unwrap(), ("trigger_type", "MANUAL"));
        assert_eq!(event_param("schedule").unwrap(), ("trigger_type", "SCHEDULED"));
        assert!(event_param("release").is_err());
    }

    #[test]
    fn user_filters() {
        let u = "d93127e7-9e58-4b61-956d-b0cf978482aa";
        assert_eq!(user_filter("@me"), UserFilter::Me);
        assert_eq!(user_filter(u), UserFilter::Uuid(format!("{{{u}}}")));
        assert_eq!(user_filter(&format!("{{{u}}}")), UserFilter::Uuid(format!("{{{u}}}")));
        assert_eq!(user_filter("jdoe"), UserFilter::Name("jdoe".into()));
        let p = json!({"creator": {"nickname": "jdoe", "display_name": "Jane Doe", "account_id": "5f1"}});
        assert!(creator_is(&p, "JDOE") && creator_is(&p, "jane doe") && creator_is(&p, "5f1"));
        assert!(!creator_is(&p, "jane"));
    }

    #[test]
    fn steps_and_watch() {
        let st = json!({"name": "Build & Test", "uuid": "{s-1}", "state": {"name": "IN_PROGRESS"}});
        assert!(step_matches(&st, "build & test"));
        assert!(step_matches(&st, "s-1") && step_matches(&st, "{s-1}"));
        assert!(!step_matches(&st, "build"));

        let mut seen = HashMap::new();
        let p = json!({"build_number": 7, "state": {"name": "IN_PROGRESS", "stage": {"name": "RUNNING"}}});
        assert_eq!(
            changes(&mut seen, &p, std::slice::from_ref(&st)),
            ["Pipeline #7: RUNNING", "  Build & Test: IN_PROGRESS"]
        );
        assert!(changes(&mut seen, &p, &[st]).is_empty());
        let done_st = json!({"name": "Build & Test", "uuid": "{s-1}", "state": {"name": "COMPLETED", "result": {"name": "FAILED"}}});
        let done = json!({"build_number": 7, "state": {"name": "COMPLETED", "result": {"name": "FAILED"}}});
        assert_eq!(changes(&mut seen, &done, &[done_st]), ["Pipeline #7: FAILED", "  Build & Test: FAILED"]);

        assert!(!finished(&p) && finished(&done));
        assert!(finished(&json!({"state": {"name": "IN_PROGRESS", "stage": {"name": "PAUSED"}}})));
        assert!(check_exit(true, &done).is_err());
        assert!(check_exit(false, &done).is_ok());
        assert!(check_exit(true, &p).is_ok());
        assert!(check_exit(true, &json!({"state": {"result": {"name": "SUCCESSFUL"}}})).is_ok());
    }
}
