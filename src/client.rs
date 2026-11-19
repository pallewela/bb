//! Bitbucket Cloud HTTP client. All paths are relative to /2.0 unless they are full URLs.
use crate::config::{self, Creds};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::Value;
use std::time::Duration;

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
        let (creds, _) = config::load().map_err(|e| crate::exit::Exit::new(crate::exit::AUTH_REQUIRED, format!("{e:#}")))?;
        Ok(Client::with_creds(&creds))
    }

    pub fn with_creds(creds: &Creds) -> Client {
        let raw = format!("{}:{}", creds.email, creds.token);
        Client {
            // SameHost: /diff redirects within api.bitbucket.org (needs auth);
            // step logs redirect to presigned S3 URLs (must not get auth).
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .redirect_auth_headers(ureq::RedirectAuthHeaders::SameHost)
                .build(),
            auth: format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw)),
        }
    }

    /// Raw request; returns the response body as text. Non-2xx becomes `HTTP <code>: <message>`.
    // ponytail: body read into memory (ureq caps at 10MB); stream if huge logs/diffs matter.
    pub fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<String> {
        let req = self.agent.request(method, &url(path)).set("Authorization", &self.auth);
        let res = match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        };
        match res {
            Ok(r) => Ok(r.into_string()?),
            Err(ureq::Error::Status(code, r)) => Err(status_error(code, &r.into_string().unwrap_or_default())),
            Err(e) => Err(e).context("request failed"),
        }
    }

    /// Request with extra headers that returns the full response, whatever its status (for `bb api`).
    pub fn send(&self, method: &str, path: &str, headers: &[(String, String)], body: Option<&Value>) -> Result<Response> {
        let mut req = self.agent.request(method, &url(path)).set("Authorization", &self.auth);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        let res = match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        };
        let r = match res {
            Ok(r) | Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(e).context("request failed"),
        };
        let mut names = r.headers_names();
        names.dedup();
        let headers = names.iter().flat_map(|n| r.all(n).into_iter().map(move |v| (n.clone(), v.to_string()))).collect();
        Ok(Response {
            status: r.status(),
            status_line: format!("{} {} {}", r.http_version(), r.status(), r.status_text()),
            headers,
            body: r.into_string()?,
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
    fn limit_args() {
        assert_eq!(limit_arg(None, 30), (30, false));
        assert_eq!(limit_arg(Some(5), 30), (5, true));
        assert_eq!(limit_arg(Some(0), 30), (usize::MAX, true));
    }
}
