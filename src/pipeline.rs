//! `bb pipeline` (alias `bb run`): list, view, run, stop and read logs of Bitbucket Pipelines.
use crate::client::{enc, limit_arg, Client};
use crate::context::{current_branch, Repo};
use crate::output::{json_mode, more_note, open_browser, print_json, s, short_time, table};
use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum Cmd {
    /// List recent pipelines
    List {
        /// Only pipelines for this branch
        #[arg(long)]
        branch: Option<String>,
        /// Maximum number of pipelines to list [default: 30; 0 = all]
        #[arg(short = 'L', long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show a pipeline and its steps
    View {
        /// Build number or UUID
        id: String,
        /// Open the pipeline in the browser
        #[arg(long)]
        web: bool,
    },
    /// Trigger a pipeline on a branch
    Run {
        /// Branch to run on (default: current branch)
        #[arg(long)]
        branch: Option<String>,
        /// Custom pipeline name (from bitbucket-pipelines.yml `custom:`)
        #[arg(long, value_name = "NAME")]
        pipeline: Option<String>,
        /// Pipeline variable KEY=VALUE (repeatable)
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
    },
    /// Stop a running pipeline
    Stop {
        /// Build number or UUID
        id: String,
    },
    /// Print step logs
    Logs {
        /// Build number or UUID
        id: String,
        /// Only this step (name, case-insensitive, or UUID)
        #[arg(long, value_name = "NAME|UUID")]
        step: Option<String>,
        /// Only failed steps
        #[arg(long)]
        failed: bool,
    },
}

pub fn run(cmd: Cmd) -> Result<()> {
    let repo = Repo::current()?;
    let c = Client::new()?;
    let base = format!("{}/pipelines", repo.api());
    match cmd {
        Cmd::List { branch, limit } => {
            let (limit, chosen) = limit_arg(limit, 30);
            let mut path = format!("{base}/?sort=-created_on&pagelen={}", limit.clamp(1, 100));
            if let Some(b) = branch {
                path += &format!("&target.ref_name={}", enc(&b));
            }
            let list = c.list(&path, limit)?;
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
        Cmd::View { id, web } => {
            let p = resolve(&c, &base, &id)?;
            let url = web_url(&repo, &p);
            if web {
                return open_browser(&url);
            }
            let steps = c.paginate(&format!("{base}/{}/steps/", enc(&s(&p, "uuid"))), usize::MAX)?;
            if json_mode() {
                print_json(&json!({"pipeline": p, "steps": steps}));
                return Ok(());
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
        }
        Cmd::Run { branch, pipeline, vars } => {
            let branch = match branch {
                Some(b) => b,
                None => current_branch()?,
            };
            let p = c.post(&format!("{base}/"), Some(&run_body(&branch, pipeline.as_deref(), &vars)?))?;
            if json_mode() {
                print_json(&p);
            } else {
                println!("Started pipeline #{}\n{}", s(&p, "build_number"), web_url(&repo, &p));
            }
        }
        Cmd::Stop { id } => {
            let p = resolve(&c, &base, &id)?;
            c.post(&format!("{base}/{}/stopPipeline", enc(&s(&p, "uuid"))), None)?;
            println!("Stopping pipeline #{}", s(&p, "build_number"));
        }
        Cmd::Logs { id, step, failed } => {
            let p = resolve(&c, &base, &id)?;
            let steps_path = format!("{base}/{}/steps/", enc(&s(&p, "uuid")));
            let steps: Vec<Value> = c
                .paginate(&steps_path, usize::MAX)?
                .into_iter()
                .filter(|st| match &step {
                    Some(want) => {
                        s(st, "name").eq_ignore_ascii_case(want) || s(st, "uuid") == brace(want)
                    }
                    None => true,
                })
                .filter(|st| !failed || matches!(s(st, "state.result.name").as_str(), "FAILED" | "ERROR"))
                .collect();
            if steps.is_empty() {
                bail!("no matching steps in pipeline #{}", s(&p, "build_number"));
            }
            for st in &steps {
                eprintln!("==> {} <==", s(st, "name"));
                match c.get_text(&format!("{steps_path}{}/log", enc(&s(st, "uuid")))) {
                    Ok(log) => print!("{log}"),
                    Err(e) if e.to_string().starts_with("HTTP 404") => eprintln!("(no log available yet)"),
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(())
}

/// Fetch a pipeline by build number (all digits) or UUID (braces optional).
fn resolve(c: &Client, base: &str, id: &str) -> Result<Value> {
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        let q = enc(&format!("build_number={id}"));
        let page = c.get(&format!("{base}/?q={q}"))?;
        // Older API behaviour ignores q, so match build_number ourselves.
        return match page["values"].as_array().and_then(|v| v.iter().find(|p| s(p, "build_number") == id)) {
            Some(p) => Ok(p.clone()),
            None => bail!("pipeline #{id} not found"),
        };
    }
    c.get(&format!("{base}/{}", enc(&brace(id))))
}

/// `abc` or `{abc}` -> `{abc}`
fn brace(id: &str) -> String {
    format!("{{{}}}", id.trim().trim_start_matches('{').trim_end_matches('}'))
}

/// Result (SUCCESSFUL/FAILED/...) beats stage (RUNNING/PAUSED) beats state (PENDING/IN_PROGRESS/...).
fn status(v: &Value) -> String {
    ["state.result.name", "state.stage.name", "state.name"]
        .iter()
        .map(|k| s(v, k))
        .find(|x| !x.is_empty())
        .unwrap_or_default()
}

fn ref_name(p: &Value) -> String {
    let r = s(p, "target.ref_name");
    if r.is_empty() {
        s(p, "target.commit.hash").chars().take(7).collect()
    } else {
        r
    }
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

fn run_body(branch: &str, pipeline: Option<&str>, vars: &[String]) -> Result<Value> {
    let mut target = json!({"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": branch});
    if let Some(name) = pipeline {
        target["selector"] = json!({"type": "custom", "pattern": name});
    }
    let mut body = json!({ "target": target });
    if !vars.is_empty() {
        let vars = vars
            .iter()
            .map(|kv| match kv.split_once('=') {
                Some((k, v)) => Ok(json!({"key": k, "value": v})),
                None => bail!("--var `{kv}` must be KEY=VALUE"),
            })
            .collect::<Result<Vec<_>>>()?;
        body["variables"] = Value::Array(vars);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brace_and_status() {
        assert_eq!(brace("abc-1"), "{abc-1}");
        assert_eq!(brace("{abc-1}"), "{abc-1}");
        assert_eq!(enc(&brace("a")), "%7Ba%7D");
        let done = json!({"state": {"name": "COMPLETED", "result": {"name": "FAILED"}}});
        let running = json!({"state": {"name": "IN_PROGRESS", "stage": {"name": "RUNNING"}}});
        assert_eq!(status(&done), "FAILED");
        assert_eq!(status(&running), "RUNNING");
        assert_eq!(status(&json!({"state": {"name": "PENDING"}})), "PENDING");
        assert_eq!(ref_name(&json!({"target": {"commit": {"hash": "0123456789"}}})), "0123456");
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
        let b = run_body("main", Some("deploy"), &["A=1".into(), "B=x=y".into()]).unwrap();
        assert_eq!(
            b,
            json!({
                "target": {"type": "pipeline_ref_target", "ref_type": "branch", "ref_name": "main",
                           "selector": {"type": "custom", "pattern": "deploy"}},
                "variables": [{"key": "A", "value": "1"}, {"key": "B", "value": "x=y"}]
            })
        );
        assert!(run_body("main", None, &[]).unwrap().get("variables").is_none());
        assert!(run_body("main", None, &["NOEQ".into()]).is_err());
    }
}
