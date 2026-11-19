//! Bitbucket Cloud OAuth 2.0 authorization-code flow for `bb auth login --web`.
//!
//! Bitbucket has no device flow and no PKCE for public clients, so this needs an OAuth
//! consumer (key + secret) whose callback URL points at the loopback listener
//! (`http://127.0.0.1:<port>/callback`). No `redirect_uri` is sent: Bitbucket redirects to the
//! consumer's registered callback URL, and the listener accepts any path on its port.
use crate::client::enc;
use crate::config::{self, Creds, OAuth};
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const AUTHORIZE: &str = "https://bitbucket.org/site/oauth2/authorize";
const TOKEN: &str = "https://bitbucket.org/site/oauth2/access_token";
pub const DEFAULT_PORT: u16 = 8765;
/// How long to wait for the user to approve in the browser.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

pub fn authorize_url(client_id: &str, state: &str) -> String {
    format!("{AUTHORIZE}?client_id={}&response_type=code&state={}", enc(client_id), enc(state))
}

/// Run the browser flow; returns credentials with an empty `user` (the caller fills it in).
pub fn login(client_id: &str, client_secret: &str, port: u16) -> Result<Creds> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("could not listen on 127.0.0.1:{port} (in use? try --port)"))?;
    let state = random_state();
    let url = authorize_url(client_id, &state);
    eprintln!("Opening the browser to authorize bb. If it doesn't open, visit:\n  {url}");
    if crate::output::open_browser(&url).is_err() {
        eprintln!("(could not launch a browser)");
    }
    eprintln!("Waiting for the callback on http://127.0.0.1:{port}/ ...");
    let code = config::with_timeout(LOGIN_TIMEOUT, move || wait_for_code(&listener, &state))
        .ok_or_else(|| anyhow!("timed out after {}s waiting for the browser", LOGIN_TIMEOUT.as_secs()))??;
    token_request(client_id, client_secret, &[("grant_type", "authorization_code"), ("code", &code)], None)
}

/// Exchange the refresh token for a new access token, updating `creds` in place.
pub fn refresh(creds: &mut Creds) -> Result<()> {
    let o = creds.oauth.as_ref().ok_or_else(|| anyhow!("not an OAuth login"))?;
    let form = [("grant_type", "refresh_token"), ("refresh_token", o.refresh_token.as_str())];
    let new = token_request(&o.client_id, &o.client_secret, &form, Some(&o.refresh_token))?;
    creds.token = new.token;
    creds.oauth = new.oauth;
    Ok(())
}

/// POST to the token endpoint with the consumer key/secret as Basic auth.
fn token_request(id: &str, secret: &str, form: &[(&str, &str)], old_refresh: Option<&str>) -> Result<Creds> {
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{id}:{secret}"));
    let res = ureq::post(TOKEN)
        .timeout(Duration::from_secs(30))
        .set("Authorization", &format!("Basic {basic}"))
        .send_form(form);
    let body: Value = match res {
        Ok(r) => r.into_json().context("token response was not JSON")?,
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error_description"].as_str().or(v["error"].as_str()).map(String::from))
                .unwrap_or(text);
            bail!("HTTP {code}: {}", msg.trim())
        }
        Err(e) => return Err(e).context("token request failed"),
    };
    parse_token(&body, id, secret, old_refresh)
}

fn parse_token(v: &Value, id: &str, secret: &str, old_refresh: Option<&str>) -> Result<Creds> {
    let access = v["access_token"].as_str().ok_or_else(|| anyhow!("token response has no access_token"))?;
    // A refresh response may omit the refresh token; keep the old one then.
    let refresh = v["refresh_token"]
        .as_str()
        .or(old_refresh)
        .ok_or_else(|| anyhow!("token response has no refresh_token"))?;
    Ok(Creds {
        user: String::new(),
        token: access.to_string(),
        oauth: Some(OAuth {
            client_id: id.to_string(),
            client_secret: secret.to_string(),
            refresh_token: refresh.to_string(),
            expires_at: config::now() + v["expires_in"].as_u64().unwrap_or(3600),
        }),
    })
}

/// Serve the loopback listener until the OAuth redirect arrives. Other requests (favicon,
/// browser preconnects that never send anything) are answered or dropped and ignored.
fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let q = query(target);
        let result = if let Some(err) = q.get("error") {
            let desc = q.get("error_description").map(String::as_str).unwrap_or("");
            Err(anyhow!("authorization failed: {err} {desc}"))
        } else if let Some(code) = q.get("code") {
            if q.get("state").map(String::as_str) == Some(state) {
                Ok(code.clone())
            } else {
                Err(anyhow!("authorization failed: state mismatch (stale or forged callback)"))
            }
        } else {
            respond(&mut stream, "404 Not Found", "Not found");
            continue;
        };
        match &result {
            Ok(_) => respond(&mut stream, "200 OK", "bb is authorized. You can close this tab."),
            Err(e) => respond(&mut stream, "400 Bad Request", &format!("bb login failed: {e}")),
        }
        return result;
    }
    bail!("callback listener stopped")
}

fn respond(stream: &mut TcpStream, status: &str, msg: &str) {
    let body = format!("<!doctype html><meta charset=utf-8><title>bb</title><p>{}</p>", html_escape(msg));
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Query parameters of a request target like `/callback?code=x&state=y`.
fn query(target: &str) -> HashMap<String, String> {
    let Some((_, q)) = target.split_once('?') else { return HashMap::new() };
    q.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (decode(k), decode(v))
        })
        .collect()
}

/// application/x-www-form-urlencoded decoding (`+` is a space, `%XX` a byte).
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = |c: u8| (c as char).to_digit(16);
                match (hex(b[i + 1]), hex(b[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 128 bits for the CSRF `state`, from std's per-process random hash keys.
fn random_state() -> String {
    use std::hash::{BuildHasher, RandomState};
    let seed = (std::process::id(), std::time::SystemTime::now());
    format!("{:016x}{:016x}", RandomState::new().hash_one(seed), RandomState::new().hash_one(seed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn urls_and_queries() {
        assert_eq!(
            authorize_url("k y", "s1"),
            "https://bitbucket.org/site/oauth2/authorize?client_id=k%20y&response_type=code&state=s1"
        );
        let q = query("/callback?code=ab%2Fc&state=xyz&error_description=a+b");
        assert_eq!(q["code"], "ab/c");
        assert_eq!(q["state"], "xyz");
        assert_eq!(q["error_description"], "a b");
        assert!(query("/favicon.ico").is_empty());
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz%41"), "%zzA");
    }

    #[test]
    fn state_is_random_hex() {
        let (a, b) = (random_state(), random_state());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn token_parsing() {
        let c = parse_token(&json!({"access_token": "a", "refresh_token": "r", "expires_in": 7200}), "id", "sec", None)
            .unwrap();
        let o = c.oauth.unwrap();
        assert_eq!((c.token.as_str(), o.refresh_token.as_str(), o.client_id.as_str()), ("a", "r", "id"));
        assert!(o.expires_at >= config::now() + 7190);
        // refresh responses may omit the refresh token
        let c = parse_token(&json!({"access_token": "b"}), "id", "sec", Some("old")).unwrap();
        assert_eq!(c.oauth.unwrap().refresh_token, "old");
        assert!(parse_token(&json!({"access_token": "b"}), "id", "sec", None).is_err());
        assert!(parse_token(&json!({"error": "invalid_grant"}), "id", "sec", None).is_err());
    }

    #[test]
    fn callback_server() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = std::thread::spawn(move || wait_for_code(&listener, "good"));
        let get = |path: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut resp = String::new();
            let _ = std::io::Read::read_to_string(&mut s, &mut resp);
            resp
        };
        assert!(get("/favicon.ico").starts_with("HTTP/1.1 404"));
        assert!(get("/callback?code=C0DE&state=good").starts_with("HTTP/1.1 200"));
        assert_eq!(h.join().unwrap().unwrap(), "C0DE");
    }

    #[test]
    fn callback_state_mismatch() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = std::thread::spawn(move || wait_for_code(&listener, "good"));
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET /callback?code=C0DE&state=evil HTTP/1.1\r\n\r\n").unwrap();
        assert!(h.join().unwrap().unwrap_err().to_string().contains("state mismatch"));
    }
}
