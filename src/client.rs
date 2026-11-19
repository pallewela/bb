//! Bitbucket Cloud HTTP client. All paths are relative to /2.0 unless they are full URLs.
use crate::config::{self, Creds};
use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::Value;
use std::time::Duration;

const API: &str = "https://api.bitbucket.org/2.0";

pub struct Client {
    agent: ureq::Agent,
    auth: String,
}

fn url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{API}/{}", path.trim_start_matches('/'))
    }
}

impl Client {
    /// Client using stored credentials (env vars, then credentials file).
    pub fn new() -> Result<Client> {
        Ok(Client::with_creds(&config::load()?.0))
    }

    pub fn with_creds(creds: &Creds) -> Client {
        let auth = if creds.oauth.is_some() {
            format!("Bearer {}", creds.token)
        } else {
            let raw = format!("{}:{}", creds.user, creds.token);
            format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw))
        };
        Client {
            // SameHost: /diff redirects within api.bitbucket.org (needs auth);
            // step logs redirect to presigned S3 URLs (must not get auth).
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .redirect_auth_headers(ureq::RedirectAuthHeaders::SameHost)
                .build(),
            auth,
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
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                let msg = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["error"]["message"].as_str().map(String::from))
                    .unwrap_or(text);
                bail!("HTTP {code}: {}", msg.trim())
            }
            Err(e) => Err(e).context("request failed"),
        }
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
        let mut out = Vec::new();
        let mut next = Some(path.to_string());
        while let Some(p) = next {
            let mut page = self.get(&p)?;
            if let Some(vals) = page["values"].as_array_mut() {
                out.append(vals);
            }
            if out.len() >= limit {
                out.truncate(limit);
                break;
            }
            next = page["next"].as_str().map(String::from);
        }
        Ok(out)
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
}
