//! Credentials, stored the way `gh` does it. Precedence:
//! 1. BITBUCKET_EMAIL + BITBUCKET_TOKEN env vars
//! 2. saved login: email in ~/.config/bb/hosts.json (%APPDATA%\bb\hosts.json on Windows),
//!    token in the OS keychain
//!    (or in hosts.json itself with --insecure-storage / when no keychain is available).
use anyhow::{anyhow, bail, Context, Result};
use keyring::Entry;
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

const SERVICE: &str = "bb:bitbucket.org";

/// Run `f` on a thread and give up after `timeout` (the thread is abandoned; we exit soon anyway).
fn with_timeout<T: Send + 'static>(timeout: Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout).ok()
}

/// One keychain operation for `email`'s entry, with a timeout like `gh`'s: on Linux a locked
/// or stuck Secret Service daemon can block a D-Bus call forever. Elsewhere the wait is long
/// because macOS may be showing the user a Keychain access prompt.
fn keychain<T: Send + 'static>(
    email: &str,
    op: impl FnOnce(Entry) -> keyring::Result<T> + Send + 'static,
) -> Result<T> {
    let secs = if cfg!(target_os = "linux") { 3 } else { 60 };
    let email = email.to_string();
    match with_timeout(Duration::from_secs(secs), move || Entry::new(SERVICE, &email).and_then(op)) {
        Some(res) => Ok(res?),
        None => bail!("timed out after {secs}s waiting for the system keychain"),
    }
}

pub struct Creds {
    pub email: String,
    pub token: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Env,
    Keyring,
    File,
}

pub fn path() -> PathBuf {
    config_dir(|k| env::var_os(k), cfg!(windows)).join("bb").join("hosts.json")
}

/// The directory that holds `bb/hosts.json`, as `gh` picks it: `$XDG_CONFIG_HOME` if set, else
/// `%APPDATA%` on Windows (which usually has no `HOME`), else `~/.config`.
fn config_dir(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> PathBuf {
    let get = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = get("XDG_CONFIG_HOME") {
        return dir;
    }
    if windows && let Some(dir) = get("APPDATA") {
        return dir;
    }
    let home = get("HOME").or_else(|| if windows { get("USERPROFILE") } else { None }).unwrap_or_default();
    home.join(".config")
}

/// (email, token if stored in plaintext)
fn read_file(p: &Path) -> Result<(String, Option<String>)> {
    let text = fs::read_to_string(p).map_err(|_| anyhow!("not logged in; run `bb auth login`"))?;
    let v: Value = serde_json::from_str(&text).with_context(|| format!("corrupt {}", p.display()))?;
    let email = v["email"].as_str().ok_or_else(|| anyhow!("{} has no `email`", p.display()))?;
    Ok((email.to_string(), v["token"].as_str().map(String::from)))
}

/// Always mode 0600: the file may hold the token.
fn write_file(p: &Path, email: &str, token: Option<&str>) -> Result<()> {
    fs::create_dir_all(p.parent().unwrap())?;
    let mut v = json!({ "email": email });
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

/// Load credentials and report where they came from.
pub fn load() -> Result<(Creds, Source)> {
    if let (Ok(email), Ok(token)) = (env::var("BITBUCKET_EMAIL"), env::var("BITBUCKET_TOKEN"))
        && !email.is_empty()
        && !token.is_empty()
    {
        return Ok((Creds { email, token }, Source::Env));
    }
    let (email, token) = read_file(&path())?;
    if let Some(token) = token {
        return Ok((Creds { email, token }, Source::File));
    }
    let token = keychain(&email, |e| e.get_password())
        .map_err(|e| anyhow!("no token in the system keychain for {email} ({e}); run `bb auth login`"))?;
    Ok((Creds { email, token }, Source::Keyring))
}

/// Save a login. Token goes to the OS keychain unless `insecure` or no keychain is
/// available, in which case it is written to the hosts file. Returns where it went.
pub fn save(creds: &Creds, insecure: bool) -> Result<Source> {
    let _ = delete(); // drop any previous login (other email, or a copy in the other store)
    if !insecure {
        let token = creds.token.clone();
        match keychain(&creds.email, move |e| e.set_password(&token)) {
            Ok(()) => {
                write_file(&path(), &creds.email, None)?;
                return Ok(Source::Keyring);
            }
            Err(e) => eprintln!("warning: system keychain unavailable ({e}); storing the token in plaintext"),
        }
    }
    write_file(&path(), &creds.email, Some(&creds.token))?;
    Ok(Source::File)
}

/// Delete the saved login (keychain entry and hosts file). Returns false if there was none.
pub fn delete() -> Result<bool> {
    let p = path();
    if let Ok((email, _)) = read_file(&p) {
        let _ = keychain(&email, |e| e.delete_credential());
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
        write_file(&p, "a@b.c", None).unwrap();
        assert_eq!(read_file(&p).unwrap(), ("a@b.c".to_string(), None));
        write_file(&p, "a@b.c", Some("tok")).unwrap();
        assert_eq!(read_file(&p).unwrap(), ("a@b.c".to_string(), Some("tok".to_string())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
        assert!(read_file(&p).is_err());
    }

    #[test]
    fn config_dir_like_gh() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
        };
        let all = &[("XDG_CONFIG_HOME", "/xdg"), ("APPDATA", "C:\\AppData"), ("HOME", "/home/u"), ("USERPROFILE", "C:\\Users\\u")];
        assert_eq!(config_dir(env(all), false), PathBuf::from("/xdg"));
        assert_eq!(config_dir(env(all), true), PathBuf::from("/xdg"));
        let no_xdg = &[("XDG_CONFIG_HOME", ""), ("APPDATA", "C:\\AppData"), ("HOME", "/home/u"), ("USERPROFILE", "C:\\Users\\u")];
        assert_eq!(config_dir(env(no_xdg), false), PathBuf::from("/home/u/.config"));
        assert_eq!(config_dir(env(no_xdg), true), PathBuf::from("C:\\AppData"));
        // Windows without APPDATA: HOME, else USERPROFILE.
        assert_eq!(config_dir(env(&[("USERPROFILE", "C:\\Users\\u")]), true), PathBuf::from("C:\\Users\\u").join(".config"));
        assert_eq!(config_dir(env(&[("USERPROFILE", "C:\\Users\\u")]), false), PathBuf::from(".config"));
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
