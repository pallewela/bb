//! Credentials, stored the way `gh` does it. Precedence:
//! 1. BITBUCKET_EMAIL + BITBUCKET_TOKEN env vars
//! 2. saved login: identity in ~/.config/bb/hosts.json, secret in the OS keychain
//!    (or in hosts.json itself with --insecure-storage / when no keychain is available).
//!
//! A saved login is either an API token (identity = email, secret = the token) or an OAuth
//! login (identity = Bitbucket username, secret = JSON with access/refresh tokens and the
//! consumer key/secret needed to refresh). Expired OAuth access tokens are refreshed on load.
use anyhow::{anyhow, bail, Context, Result};
use keyring::Entry;
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

const SERVICE: &str = "bb:bitbucket.org";

/// Run `f` on a thread and give up after `timeout` (the thread is abandoned; we exit soon anyway).
pub fn with_timeout<T: Send + 'static>(timeout: Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout).ok()
}

/// One keychain operation for `account`'s entry, with a timeout like `gh`'s: on Linux a locked
/// or stuck Secret Service daemon can block a D-Bus call forever. Elsewhere the wait is long
/// because macOS may be showing the user a Keychain access prompt.
fn keychain<T: Send + 'static>(
    account: &str,
    op: impl FnOnce(Entry) -> keyring::Result<T> + Send + 'static,
) -> Result<T> {
    let secs = if cfg!(target_os = "linux") { 3 } else { 60 };
    let account = account.to_string();
    match with_timeout(Duration::from_secs(secs), move || Entry::new(SERVICE, &account).and_then(op)) {
        Some(res) => Ok(res?),
        None => bail!("timed out after {secs}s waiting for the system keychain"),
    }
}

pub struct Creds {
    /// Email (API token login) or Bitbucket username (OAuth login); also the keychain account.
    pub user: String,
    /// API token, or OAuth access token.
    pub token: String,
    pub oauth: Option<OAuth>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OAuth {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    /// Unix seconds when the access token expires.
    pub expires_at: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The string kept in the keychain (or hosts.json `token`).
fn secret(c: &Creds) -> String {
    match &c.oauth {
        None => c.token.clone(),
        Some(o) => json!({
            "access_token": c.token,
            "refresh_token": o.refresh_token,
            "client_id": o.client_id,
            "client_secret": o.client_secret,
            "expires_at": o.expires_at,
        })
        .to_string(),
    }
}

fn from_secret(user: String, oauth: bool, secret: String) -> Result<Creds> {
    if !oauth {
        return Ok(Creds { user, token: secret, oauth: None });
    }
    let v: Value = serde_json::from_str(&secret).context("corrupt saved OAuth login; run `bb auth login --web`")?;
    let f = |k: &str| v[k].as_str().map(String::from).ok_or_else(|| anyhow!("saved OAuth login has no `{k}`"));
    Ok(Creds {
        user,
        token: f("access_token")?,
        oauth: Some(OAuth {
            client_id: f("client_id")?,
            client_secret: f("client_secret")?,
            refresh_token: f("refresh_token")?,
            expires_at: v["expires_at"].as_u64().unwrap_or(0),
        }),
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Env,
    Keyring,
    File,
}

pub fn path() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("bb").join("hosts.json")
}

#[derive(Debug, PartialEq)]
struct Saved {
    user: String,
    oauth: bool,
    /// The secret, if stored in plaintext.
    token: Option<String>,
}

/// hosts.json: `{"email": E}` for an API token, `{"auth": "oauth", "user": U}` for OAuth;
/// plus `"token"` holding the secret when stored in plaintext.
fn read_file(p: &Path) -> Result<Saved> {
    let text = fs::read_to_string(p).map_err(|_| anyhow!("not logged in; run `bb auth login`"))?;
    let v: Value = serde_json::from_str(&text).with_context(|| format!("corrupt {}", p.display()))?;
    let oauth = v["auth"] == "oauth";
    let key = if oauth { "user" } else { "email" };
    let user = v[key].as_str().ok_or_else(|| anyhow!("{} has no `{key}`", p.display()))?;
    Ok(Saved { user: user.to_string(), oauth, token: v["token"].as_str().map(String::from) })
}

/// Always mode 0600: the file may hold the token.
fn write_file(p: &Path, user: &str, oauth: bool, token: Option<&str>) -> Result<()> {
    fs::create_dir_all(p.parent().unwrap())?;
    let mut v = if oauth { json!({ "auth": "oauth", "user": user }) } else { json!({ "email": user }) };
    if let Some(t) = token {
        v["token"] = json!(t);
    }
    let body = serde_json::to_string_pretty(&v)?;
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt, os::unix::fs::PermissionsExt};
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?;
        // mode() only applies on create; enforce on pre-existing files too
        fs::set_permissions(p, fs::Permissions::from_mode(0o600))?;
        f.write_all(body.as_bytes())?;
    }
    #[cfg(not(unix))]
    fs::write(p, body)?;
    Ok(())
}

/// Load credentials and report where they came from. A saved OAuth login whose access
/// token has expired (or is about to) is refreshed and re-saved first.
pub fn load() -> Result<(Creds, Source)> {
    if let (Ok(email), Ok(token)) = (env::var("BITBUCKET_EMAIL"), env::var("BITBUCKET_TOKEN"))
        && !email.is_empty()
        && !token.is_empty()
    {
        return Ok((Creds { user: email, token, oauth: None }, Source::Env));
    }
    let saved = read_file(&path())?;
    let (secret, src) = match saved.token {
        Some(t) => (t, Source::File),
        None => {
            let user = saved.user.clone();
            let t = keychain(&saved.user, |e| e.get_password())
                .map_err(|e| anyhow!("no token in the system keychain for {user} ({e}); run `bb auth login`"))?;
            (t, Source::Keyring)
        }
    };
    let mut creds = from_secret(saved.user, saved.oauth, secret)?;
    if creds.oauth.as_ref().is_some_and(|o| o.expires_at <= now() + 60) {
        crate::oauth::refresh(&mut creds).context("could not refresh the OAuth login; run `bb auth login --web`")?;
        save(&creds, src == Source::File)?;
    }
    Ok((creds, src))
}

/// Save a login. The secret goes to the OS keychain unless `insecure` or no keychain is
/// available, in which case it is written to the hosts file. Returns where it went.
pub fn save(creds: &Creds, insecure: bool) -> Result<Source> {
    let _ = delete(); // drop any previous login (other account, or a copy in the other store)
    let oauth = creds.oauth.is_some();
    if !insecure {
        let secret = secret(creds);
        match keychain(&creds.user, move |e| e.set_password(&secret)) {
            Ok(()) => {
                write_file(&path(), &creds.user, oauth, None)?;
                return Ok(Source::Keyring);
            }
            Err(e) => eprintln!("warning: system keychain unavailable ({e}); storing the token in plaintext"),
        }
    }
    write_file(&path(), &creds.user, oauth, Some(&secret(creds)))?;
    Ok(Source::File)
}

/// Delete the saved login (keychain entry and hosts file). Returns false if there was none.
pub fn delete() -> Result<bool> {
    let p = path();
    if let Ok(saved) = read_file(&p) {
        let _ = keychain(&saved.user, |e| e.delete_credential());
    }
    match fs::remove_file(&p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_file_roundtrip() {
        let p = env::temp_dir().join(format!("bb-test-{}", std::process::id())).join("hosts.json");
        let saved = |user: &str, oauth, token: Option<&str>| Saved { user: user.into(), oauth, token: token.map(String::from) };
        write_file(&p, "a@b.c", false, None).unwrap();
        assert_eq!(read_file(&p).unwrap(), saved("a@b.c", false, None));
        write_file(&p, "nick", true, None).unwrap();
        assert_eq!(read_file(&p).unwrap(), saved("nick", true, None));
        write_file(&p, "a@b.c", false, Some("tok")).unwrap();
        assert_eq!(read_file(&p).unwrap(), saved("a@b.c", false, Some("tok")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
        assert!(read_file(&p).is_err());
    }

    #[test]
    fn secret_roundtrip() {
        let basic = Creds { user: "a@b.c".into(), token: "tok".into(), oauth: None };
        let back = from_secret("a@b.c".into(), false, secret(&basic)).unwrap();
        assert_eq!((back.token, back.oauth), ("tok".to_string(), None));

        let o = OAuth { client_id: "id".into(), client_secret: "sec".into(), refresh_token: "ref".into(), expires_at: 42 };
        let oauth = Creds { user: "nick".into(), token: "acc".into(), oauth: Some(o.clone()) };
        let back = from_secret("nick".into(), true, secret(&oauth)).unwrap();
        assert_eq!((back.token, back.oauth), ("acc".to_string(), Some(o)));
        assert!(from_secret("nick".into(), true, "not json".into()).is_err());
    }

    #[test]
    fn timeout() {
        assert_eq!(with_timeout(Duration::from_secs(5), || 7), Some(7));
        let slow = || thread::sleep(Duration::from_secs(2));
        assert_eq!(with_timeout(Duration::from_millis(20), slow), None);
    }

    /// Touches the real OS keychain (dummy entry, removed afterwards): `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn keychain_roundtrip() {
        let who = "bb-test@example.invalid";
        keychain(who, |e| e.set_password("dummy")).unwrap();
        assert_eq!(keychain(who, |e| e.get_password()).unwrap(), "dummy");
        keychain(who, |e| e.delete_credential()).unwrap();
        assert!(keychain(who, |e| e.get_password()).is_err());
    }
}
