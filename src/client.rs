//! Bitbucket Cloud HTTP client. All paths are relative to /2.0 unless they are full URLs.
use crate::config::{self, Creds};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::Value;
use std::time::Duration;
use ureq::http;

const API: &str = "https://api.bitbucket.org/2.0";

pub struct Client {
    agent: ureq::Agent,
    auth: String,
}

pub fn url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{API}/{}", path.trim_start_matches('/'))
    }
}

impl Client {
    /// Client using stored credentials (env vars, then credentials file).
    pub fn new() -> Result<Client> {
        // No usable credentials: exit 4, like `gh` when authentication is required.
        let (creds, _) = config::load().map_err(|e| crate::exit::Exit::err(crate::exit::AUTH_REQUIRED, format!("{e:#}")))?;
        Ok(Client::with_creds(&creds))
    }

    pub fn with_creds(creds: &Creds) -> Client {
        let raw = format!("{}:{}", creds.email, creds.token);
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            // SameHost: /diff redirects within api.bitbucket.org (needs auth);
            // step logs redirect to presigned S3 URLs (must not get auth).
            .redirect_auth_headers(ureq::config::RedirectAuthHeaders::SameHost)
            // Non-2xx responses come back as responses: `request` turns them into errors itself,
            // and `send` returns them to `bb api`.
            .http_status_as_error(false)
            .build();
        Client {
            agent: config.into(),
            auth: format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw)),
        }
    }

    /// Send one request with auth, extra headers and an optional JSON body.
    fn call(&self, method: &str, path: &str, headers: &[(String, String)], body: Option<&Value>) -> Result<http::Response<ureq::Body>> {
        let mut req = http::Request::builder()
            .method(method)
            .uri(escape_uri(&url(path)))
            .header("Authorization", &self.auth);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let res = match body {
            Some(b) => {
                // A caller's Content-Type (e.g. `bb api -H`) wins over the JSON default, as in ureq 2.
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                    req = req.header("Content-Type", "application/json");
                }
                self.agent.run(req.body(serde_json::to_vec(b)?).context("invalid request")?)
            }
            // ureq 3 sends `Transfer-Encoding: chunked` for a POST without a body; send
            // `Content-Length: 0` instead, as ureq 2 did (approve, decline, ...).
            None if matches!(method, "POST" | "PUT" | "PATCH") => self.agent.run(req.body(&[][..]).context("invalid request")?),
            None => self.agent.run(req.body(()).context("invalid request")?),
        };
        res.context("request failed")
    }

    /// Raw request; returns the response body as text. Non-2xx becomes `HTTP <code>: <message>`.
    // ponytail: body read into memory (ureq caps at 10MB); stream if huge logs/diffs matter.
    pub fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<String> {
        let mut r = self.call(method, path, &[], body)?;
        let code = r.status().as_u16();
        if !r.status().is_success() {
            return Err(status_error(code, &r.body_mut().read_to_string().unwrap_or_default()));
        }
        r.body_mut().read_to_string().context("reading response")
    }

    /// Request with extra headers that returns the full response, whatever its status (for `bb api`).
    pub fn send(&self, method: &str, path: &str, headers: &[(String, String)], body: Option<&Value>) -> Result<Response> {
        let mut r = self.call(method, path, headers, body)?;
        let status = r.status();
        let headers = r
            .headers()
            .iter()
            .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        Ok(Response {
            status: status.as_u16(),
            status_line: format!("{:?} {} {}", r.version(), status.as_u16(), status.canonical_reason().unwrap_or("")).trim_end().to_string(),
            headers,
            body: r.body_mut().read_to_string().context("reading response")?,
        })
    }

    fn json(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let text = self.request(method, path, body)?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).context("response was not JSON")
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        self.json("GET", path, None)
    }
    /// GET returning the body as text (diffs, logs).
    pub fn get_text(&self, path: &str) -> Result<String> {
        self.request("GET", path, None)
    }
    /// POST; pass `None` for endpoints that take no body (approve, decline, ...).
    pub fn post(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        self.json("POST", path, body)
    }
    #[allow(dead_code)]
    pub fn put(&self, path: &str, body: &Value) -> Result<Value> {
        self.json("PUT", path, Some(body))
    }
    pub fn delete(&self, path: &str) -> Result<Value> {
        self.json("DELETE", path, None)
    }

    /// Follow `next` links, collecting `values` until `limit` items (usize::MAX = everything).
    pub fn paginate(&self, path: &str, limit: usize) -> Result<Vec<Value>> {
        Ok(self.list(path, limit)?.items)
    }

    /// Like `paginate`, but also reports the API's total count and whether items were left out.
    pub fn list(&self, path: &str, limit: usize) -> Result<Listing> {
        let mut out = Listing { items: Vec::new(), total: None, truncated: false };
        let mut next = Some(path.to_string());
        while let Some(p) = next {
            let mut page = self.get(&p)?;
            out.total = out.total.or(page["size"].as_u64());
            if let Some(vals) = page["values"].as_array_mut() {
                out.items.append(vals);
            }
            next = page["next"].as_str().map(String::from);
            if out.items.len() >= limit {
                out.truncated = out.items.len() > limit || next.is_some();
                out.items.truncate(limit);
                break;
            }
        }
        Ok(out)
    }
}

/// Error for a non-2xx response: `HTTP <code>: <message>`, using the API's error message when there is one.
pub fn status_error(code: u16, text: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| text.to_string());
    anyhow::anyhow!("HTTP {code}: {}", msg.trim())
}

/// Result of `Client::send`.
pub struct Response {
    pub status: u16,
    /// e.g. `HTTP/1.1 200 OK`
    pub status_line: String,
    /// Lowercased names, in the order received.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Result of `Client::list`.
pub struct Listing {
    pub items: Vec<Value>,
    /// Total matching items, when the API reports it (`size`).
    pub total: Option<u64>,
    /// True when more items exist than were returned.
    pub truncated: bool,
}

/// Resolve a `-L` flag to (item limit, whether the user gave it). Unset uses `default`; `0` means no limit.
pub fn limit_arg(limit: Option<usize>, default: usize) -> (usize, bool) {
    match limit {
        None => (default, false),
        Some(0) => (usize::MAX, true),
        Some(n) => (n, true),
    }
}

/// Percent-encode the bytes RFC 3986 doesn't allow anywhere in a URI (space, `"`, `<`, `{`,
/// non-ASCII, ...), so a raw path typed into `bb api` still works; existing `%XX` escapes and
/// URI delimiters are kept. ureq 2 did this through the `url` crate; ureq 3 rejects or passes them raw.
fn escape_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'%' => out.push(b as char),
            b':' | b'/' | b'?' | b'#' | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Percent-encode a value for use in a query string or path segment.
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_and_enc() {
        assert_eq!(url("/user"), "https://api.bitbucket.org/2.0/user");
        assert_eq!(url("user"), "https://api.bitbucket.org/2.0/user");
        assert_eq!(url("https://x/y"), "https://x/y");
        assert_eq!(enc("state=\"OPEN\" a/b{1}"), "state%3D%22OPEN%22%20a%2Fb%7B1%7D");
    }

    #[test]
    fn escape_uri_encodes_only_disallowed_bytes() {
        // Delimiters and existing escapes are untouched, so encoded `next` links pass through as-is.
        let ok = "https://api.bitbucket.org/2.0/r?q=state%3D%22OPEN%22&a=b+c;d,e*(f)!$'@[x]:1#frag";
        assert_eq!(escape_uri(ok), ok);
        assert_eq!(escape_uri("/p?q=state=\"OPEN\" and a<b"), "/p?q=state=%22OPEN%22%20and%20a%3Cb");
        assert_eq!(escape_uri("/refs/branches/caf\u{e9}{1}|^`\\"), "/refs/branches/caf%C3%A9%7B1%7D%7C%5E%60%5C");
    }

    /// One request as the mock server saw it.
    #[derive(Debug)]
    struct Seen {
        method: String,
        target: String,
        /// Lowercased names.
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl Seen {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
        }
    }

    /// Minimal HTTP/1.1 server on `bind`: answers each connection with the next scripted response
    /// (`{base}` is replaced by the server's own URL) and returns what it received once done.
    fn serve(bind: std::net::SocketAddr, responses: Vec<Reply>) -> (String, std::thread::JoinHandle<Vec<Seen>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind(bind).unwrap();
        let addr = listener.local_addr().unwrap();
        let host = if bind.ip().is_loopback() && bind.ip().is_ipv6() { format!("[{}]", addr.ip()) } else { addr.ip().to_string() };
        let base = format!("http://{host}:{}", addr.port());
        let b = base.clone();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for resp in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let (method, target) = (parts.next().unwrap().to_string(), parts.next().unwrap().to_string());
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    let h = h.trim_end();
                    if h.is_empty() {
                        break;
                    }
                    let (k, v) = h.split_once(':').unwrap();
                    headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
                }
                let len = headers.iter().find(|(k, _)| k == "content-length").map_or(0, |(_, v)| v.parse().unwrap());
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                seen.push(Seen { method, target, headers, body });
                stream.write_all(resp.render(&b).as_bytes()).unwrap();
            }
            seen
        });
        (base, handle)
    }

    fn local() -> std::net::SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    /// A scripted response; `{base}` in headers and body becomes the server's URL.
    struct Reply(String);

    impl Reply {
        /// The full response. It closes the connection, so each request gets its own.
        fn render(&self, base: &str) -> String {
            let (head, body) = self.0.split_once("\n\n").unwrap();
            let (head, body) = (head.replace("{base}", base), body.replace("{base}", base));
            let head = head.replace('\n', "\r\n");
            format!("HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
        }
    }

    fn reply(status: &str, headers: &[&str], body: &str) -> Reply {
        let mut head = status.to_string();
        for h in headers {
            head.push('\n');
            head.push_str(h);
        }
        Reply(format!("{head}\n\n{body}"))
    }

    fn client() -> Client {
        Client::with_creds(&Creds { email: "me@example.com".into(), token: "tok".into() })
    }

    const AUTH: &str = "Basic bWVAZXhhbXBsZS5jb206dG9r"; // me@example.com:tok

    #[test]
    fn post_sends_auth_and_json_body() {
        let (base, srv) = serve(local(), vec![reply("201 Created", &[], r#"{"id":7}"#)]);
        let v = client().post(&format!("{base}/x"), Some(&serde_json::json!({"title": "t"}))).unwrap();
        assert_eq!(v["id"], 7);
        let seen = srv.join().unwrap();
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].target, "/x");
        assert_eq!(seen[0].header("authorization"), Some(AUTH));
        assert_eq!(seen[0].header("content-type"), Some("application/json"));
        assert_eq!(serde_json::from_slice::<Value>(&seen[0].body).unwrap(), serde_json::json!({"title": "t"}));
    }

    #[test]
    fn post_without_body_and_empty_response() {
        let (base, srv) = serve(local(), vec![reply("204 No Content", &[], ""), reply("200 OK", &[], "  \n")]);
        let c = client();
        assert_eq!(c.post(&format!("{base}/approve"), None).unwrap(), Value::Null);
        assert_eq!(c.delete(&format!("{base}/d")).unwrap(), Value::Null);
        let seen = srv.join().unwrap();
        assert!(seen[0].body.is_empty());
        assert_eq!(seen[0].header("content-length"), Some("0"));
        assert_eq!(seen[0].header("transfer-encoding"), None);
        assert_eq!(seen[0].header("content-type"), None);
        assert_eq!(seen[1].header("content-length"), None);
        assert_eq!(seen[1].header("transfer-encoding"), None);
        assert_eq!(seen[1].method, "DELETE");
    }

    #[test]
    fn error_status_uses_api_message() {
        let err = r#"{"type":"error","error":{"message":"Repository not found"}}"#;
        let (base, srv) = serve(local(), vec![reply("404 Not Found", &[], err), reply("500 Internal Server Error", &[], "boom\n")]);
        let c = client();
        assert_eq!(c.get(&format!("{base}/a")).unwrap_err().to_string(), "HTTP 404: Repository not found");
        assert_eq!(c.get_text(&format!("{base}/b")).unwrap_err().to_string(), "HTTP 500: boom");
        srv.join().unwrap();
    }

    #[test]
    fn non_json_success_is_an_error_for_get_but_text_for_get_text() {
        let (base, srv) = serve(local(), vec![reply("200 OK", &[], "diff --git"), reply("200 OK", &[], "diff --git")]);
        let c = client();
        assert_eq!(c.get(&format!("{base}/a")).unwrap_err().to_string(), "response was not JSON");
        assert_eq!(c.get_text(&format!("{base}/a")).unwrap(), "diff --git");
        srv.join().unwrap();
    }

    #[test]
    fn send_returns_any_status_with_headers() {
        let (base, srv) = serve(
            local(),
            vec![reply("403 Forbidden", &["X-A: 1", "Set-Cookie: a=1", "Set-Cookie: b=2"], r#"{"error":{}}"#)],
        );
        let headers = vec![("Accept".to_string(), "text/plain".to_string()), ("Content-Type".to_string(), "application/x-test".to_string())];
        let r = client().send("PUT", &format!("{base}/p"), &headers, Some(&serde_json::json!(1))).unwrap();
        assert_eq!(r.status, 403);
        assert_eq!(r.status_line, "HTTP/1.1 403 Forbidden");
        assert_eq!(r.body, r#"{"error":{}}"#);
        let get = |n: &str| r.headers.iter().filter(|(k, _)| k == n).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
        assert_eq!(get("x-a"), ["1"]);
        assert_eq!(get("set-cookie"), ["a=1", "b=2"]);
        let seen = srv.join().unwrap();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].header("accept"), Some("text/plain"));
        // The caller's Content-Type wins over the JSON default.
        assert_eq!(seen[0].headers.iter().filter(|(k, _)| k == "content-type").count(), 1);
        assert_eq!(seen[0].header("content-type"), Some("application/x-test"));
        assert_eq!(seen[0].body, b"1");
    }

    #[test]
    fn raw_query_characters_are_encoded() {
        let (base, srv) = serve(local(), vec![reply("200 OK", &[], "{}")]);
        client().get(&format!("{base}/p?q=state=\"OPEN\" AND x")).unwrap();
        assert_eq!(srv.join().unwrap()[0].target, "/p?q=state=%22OPEN%22%20AND%20x");
    }

    #[test]
    fn list_follows_next_links_and_truncates() {
        let page1 = r#"{"size":5,"values":[1,2],"next":"{base}/p?page=2"}"#;
        let page2 = r#"{"size":5,"values":[3,4],"next":"{base}/p?page=3"}"#;
        let (base, srv) = serve(local(), vec![reply("200 OK", &[], page1), reply("200 OK", &[], page2)]);
        let l = client().list(&format!("{base}/p"), 3).unwrap();
        assert_eq!(l.items, [1, 2, 3].map(Value::from));
        assert_eq!(l.total, Some(5));
        assert!(l.truncated);
        let seen = srv.join().unwrap();
        assert_eq!(seen[1].target, "/p?page=2");
        assert_eq!(seen[1].header("authorization"), Some(AUTH));
    }

    #[test]
    fn redirect_keeps_auth_on_same_host_only() {
        // Another host name for the loopback interface, so the second server counts as a different host.
        let other: std::net::SocketAddr = std::net::ToSocketAddrs::to_socket_addrs(&("localhost", 0)).unwrap().next().unwrap();
        let (s3, s3_srv) = serve(other, vec![reply("200 OK", &[], "log text")]);
        let s3 = s3.replace(&other.ip().to_string(), "localhost").replace("[localhost]", "localhost");
        let (base, srv) = serve(
            local(),
            vec![
                reply("302 Found", &["Location: {base}/diff2"], ""),
                reply("200 OK", &[], "the diff"),
                reply("307 Temporary Redirect", &[&format!("Location: {s3}/log")], ""),
            ],
        );
        let c = client();
        assert_eq!(c.get_text(&format!("{base}/diff")).unwrap(), "the diff");
        assert_eq!(c.get_text(&format!("{base}/steps/log")).unwrap(), "log text");
        let seen = srv.join().unwrap();
        assert_eq!(seen[1].target, "/diff2");
        assert_eq!(seen[1].header("authorization"), Some(AUTH));
        let s3_seen = s3_srv.join().unwrap();
        assert_eq!(s3_seen[0].target, "/log");
        assert_eq!(s3_seen[0].header("authorization"), None);
    }

    #[test]
    fn limit_args() {
        assert_eq!(limit_arg(None, 30), (30, false));
        assert_eq!(limit_arg(Some(5), 30), (5, true));
        assert_eq!(limit_arg(Some(0), 30), (usize::MAX, true));
    }
}
