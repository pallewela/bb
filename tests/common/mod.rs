//! Shared helpers for the CLI tests: a mock Bitbucket API and a way to run the real `bb` against it.
//!
//! `bb` is a debug build here, so it honours `BB_TEST_API_URL` (see `api_base` in src/client.rs)
//! and sends every API request to the mock. Credentials come from `BITBUCKET_EMAIL` and
//! `BITBUCKET_TOKEN`, so no keychain or config file is touched.
#![allow(dead_code)] // each tests/*.rs file uses a different subset

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

/// One request as the mock saw it.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// Path and query, exactly as sent (percent-encoded).
    pub target: String,
    /// Lowercased names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("{} {} body is not JSON ({e}): {}", self.method, self.target, self.body))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// `METHOD target`, for compact assertions.
    pub fn line(&self) -> String {
        format!("{} {}", self.method, self.target)
    }
}

/// A canned response. A target with `?` must match the request's path and query exactly; one
/// without matches the path whatever the query. `{base}` in the body becomes the mock's URL
/// (for `next` links). The first matching route answers, every time it matches.
pub struct Route {
    method: String,
    target: String,
    status: u16,
    content_type: &'static str,
    body: String,
}

pub fn route(method: &str, target: &str, status: u16, body: impl Into<String>) -> Route {
    Route { method: method.into(), target: target.into(), status, content_type: "application/json", body: body.into() }
}

/// `route` with a JSON body.
pub fn json(method: &str, target: &str, body: Value) -> Route {
    route(method, target, 200, body.to_string())
}

/// `route` with a plain-text body (diffs, logs).
pub fn text(method: &str, target: &str, body: &str) -> Route {
    Route { content_type: "text/plain", ..route(method, target, 200, body) }
}

impl Route {
    fn matches(&self, method: &str, target: &str) -> bool {
        let want = if self.target.contains('?') { target } else { target.split('?').next().unwrap_or(target) };
        self.method == method && self.target == want
    }
}

pub struct Mock {
    pub base: String,
    seen: Arc<Mutex<Vec<Request>>>,
}

impl Mock {
    /// Serve `routes` on a free localhost port until the test process exits. Requests no route
    /// matches get a Bitbucket-style 404, so a missing route shows up as `HTTP 404: no mock for ...`.
    pub fn start(routes: Vec<Route>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (b, s) = (base.clone(), seen.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(req) = read_request(&mut stream) else { continue };
                let resp = match routes.iter().find(|r| r.matches(&req.method, &req.target)) {
                    Some(r) => render(r.status, r.content_type, &r.body.replace("{base}", &b)),
                    None => {
                        let msg = serde_json::json!({"type": "error", "error": {"message": format!("no mock for {}", req.line())}});
                        render(404, "application/json", &msg.to_string())
                    }
                };
                s.lock().unwrap().push(req);
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        Mock { base, seen }
    }

    /// Every request received so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.seen.lock().unwrap().clone()
    }

    /// `METHOD target` of every request so far.
    pub fn lines(&self) -> Vec<String> {
        self.requests().iter().map(Request::line).collect()
    }

    /// The only request with this method and path (query ignored); panics if there are 0 or 2+.
    pub fn one(&self, method: &str, path: &str) -> Request {
        let all = self.requests();
        let hits: Vec<_> =
            all.iter().filter(|r| r.method == method && r.target.split('?').next() == Some(path)).collect();
        assert_eq!(hits.len(), 1, "expected one {method} {path}, saw: {:#?}", self.lines());
        hits[0].clone()
    }

    /// Run `bb args...` against this mock.
    pub fn bb(&self, args: &[&str]) -> Run {
        self.bb_with(args, &[])
    }

    /// Like `bb`, with extra environment variables.
    pub fn bb_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        self.bb_in(None, args, env)
    }

    /// Like `bb_with`, run in `dir` (e.g. a `GitRepo`) instead of an empty directory.
    pub fn bb_in(&self, dir: Option<&Path>, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut cmd = bb_command(args);
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        cmd.env("BB_TEST_API_URL", &self.base).env("BITBUCKET_EMAIL", "me@example.com").env("BITBUCKET_TOKEN", "tok");
        for (k, v) in env {
            cmd.env(k, v);
        }
        Run::from(cmd.output().expect("could not run bb"))
    }
}

/// `bb args...` with a clean environment: no credentials, no API override, config in an empty
/// temp dir, and a working directory outside any git repo. The browser is a command that does
/// not exist, so a test that opens one without a `Browser` fails instead of opening a real one.
pub fn bb_command(args: &[&str]) -> Command {
    let dir = scratch_dir();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bb"));
    cmd.args(args)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .env_remove("BITBUCKET_EMAIL")
        .env_remove("BITBUCKET_TOKEN")
        .env_remove("BB_TEST_API_URL")
        .env_remove("BROWSER")
        .env("BB_BROWSER", "bb-test-no-browser")
        .env("XDG_CONFIG_HOME", dir.join("config"))
        // Git must not search above the scratch dir, or it finds the checkout of bb itself.
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap())
        .env("NO_COLOR", "1");
    cmd
}

/// A fresh empty directory under target/.
pub fn scratch_dir() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "cli-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The outcome of one `bb` run.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for Run {
    fn from(o: Output) -> Run {
        Run {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}

impl Run {
    /// Assert exit 0 and return stdout.
    #[track_caller]
    pub fn ok(&self) -> &str {
        assert_eq!(self.code, 0, "bb failed:\n--- stdout\n{}--- stderr\n{}", self.stdout, self.stderr);
        &self.stdout
    }

    /// Assert exit `code` and return stderr.
    #[track_caller]
    pub fn fails(&self, code: i32) -> &str {
        assert_eq!(self.code, code, "wrong exit code:\n--- stdout\n{}--- stderr\n{}", self.stdout, self.stderr);
        &self.stderr
    }

    /// stdout parsed as JSON (after asserting success).
    #[track_caller]
    pub fn json(&self) -> Value {
        serde_json::from_str(self.ok()).unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", self.stdout))
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
    let mut r = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        r.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':')?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
    }
    let len = headers.iter().find(|(k, _)| k == "content-length").map_or(0, |(_, v)| v.parse().unwrap_or(0));
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    Some(Request { method, target, headers, body: String::from_utf8_lossy(&body).into_owned() })
}

/// A full response that closes the connection, so each request gets its own.
fn render(status: u16, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A stand-in browser that records the URL it is asked to open. It is `git config --file F
/// test.url`, which stores its last argument (the URL) in F: git is on every CI runner, unlike a
/// shell script, and runs without a shell, so `&` and `?` in URLs survive on Windows too.
pub struct Browser {
    file: PathBuf,
}

impl Browser {
    pub fn new() -> Browser {
        Browser { file: scratch_dir().join("opened") }
    }

    /// The `("BB_BROWSER", ...)` pair to pass to `bb_with`/`bb_in`.
    pub fn env(&self) -> (&'static str, String) {
        ("BB_BROWSER", format!("git config --file {} test.url", self.file.display()))
    }

    /// The URL `bb` opened, or None if it opened nothing.
    pub fn opened(&self) -> Option<String> {
        let out =
            Command::new("git").args(["config", "--file"]).arg(&self.file).args(["--get", "test.url"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Mock {
    /// Run `bb args...` with `browser` as the browser and return the URL it opened.
    #[track_caller]
    pub fn opened(&self, args: &[&str]) -> String {
        let browser = Browser::new();
        let (k, v) = browser.env();
        self.bb_with(args, &[(k, &v)]).ok();
        browser.opened().expect("bb opened no URL")
    }
}

/// A throwaway git repository on branch `main` with one commit, and `origin` set to a
/// Bitbucket URL (`git@bitbucket.org:ws/r.git`) so `bb` finds the repository without -R.
pub struct GitRepo {
    pub dir: PathBuf,
}

impl GitRepo {
    pub fn new() -> GitRepo {
        let dir = scratch_dir().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        let repo = GitRepo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["remote", "add", "origin", "git@bitbucket.org:ws/r.git"]);
        repo.commit("Initial commit", "");
        repo
    }

    /// A bare repository to use as a real, fetchable remote (for `git fetch`/`clone`).
    pub fn bare() -> PathBuf {
        let dir = scratch_dir().join("remote.git");
        let out = Command::new("git").args(["init", "-q", "--bare", "-b", "main"]).arg(&dir).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        dir
    }

    /// Run git here with a fixed identity; panics on failure; returns trimmed stdout.
    #[track_caller]
    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("could not run git");
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// An empty commit with this subject and body; returns its hash.
    pub fn commit(&self, subject: &str, body: &str) -> String {
        let msg = if body.is_empty() { subject.to_string() } else { format!("{subject}\n\n{body}") };
        self.git(&["commit", "-q", "--allow-empty", "-m", &msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn branch(&self) -> String {
        self.git(&["branch", "--show-current"])
    }
}
