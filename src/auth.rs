//! `bb auth`: login, status, logout, token.
use crate::client::Client;
use crate::config::{self, Creds, Source};
use crate::oauth;
use crate::output::s;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::io::{self, BufRead, Read, Write};

#[derive(Subcommand)]
pub enum Cmd {
    /// Log in with an Atlassian email + API token, or with --web in the browser (OAuth);
    /// the token is saved in the system keychain
    Login {
        /// Atlassian account email (prompted if absent)
        #[arg(long, conflicts_with = "web")]
        email: Option<String>,
        /// Read the API token from stdin
        #[arg(long, conflicts_with = "web")]
        with_token: bool,
        /// Store the token in plaintext in the config file instead of the system keychain
        #[arg(long)]
        insecure_storage: bool,
        /// Log in through the browser with an OAuth consumer instead of an API token
        #[arg(short, long)]
        web: bool,
        /// OAuth consumer key (default: $BB_OAUTH_CLIENT_ID)
        #[arg(long, requires = "web")]
        client_id: Option<String>,
        /// OAuth consumer secret (default: $BB_OAUTH_CLIENT_SECRET, else prompted)
        #[arg(long, requires = "web")]
        client_secret: Option<String>,
        /// Local port for the OAuth callback; the consumer's callback URL must be http://127.0.0.1:PORT/callback
        #[arg(long, requires = "web", default_value_t = oauth::DEFAULT_PORT)]
        port: u16,
    },
    /// Show where credentials come from and whether they work
    Status,
    /// Delete the saved login (keychain entry and config file)
    Logout,
    /// Print the API token (for scripting)
    Token,
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Login { web: true, client_id, client_secret, port, insecure_storage, .. } => {
            login_web(client_id, client_secret, port, insecure_storage)
        }
        Cmd::Login { email, with_token, insecure_storage, .. } => login(email, with_token, insecure_storage),
        Cmd::Status => status(),
        Cmd::Logout => {
            if config::delete()? {
                eprintln!("Logged out: removed the saved login");
            } else {
                eprintln!("No saved login");
            }
            if config::load().is_ok_and(|(_, src)| src == Source::Env) {
                eprintln!("Note: BITBUCKET_EMAIL/BITBUCKET_TOKEN are still set in the environment.");
            }
            Ok(())
        }
        Cmd::Token => {
            println!("{}", config::load()?.0.token);
            Ok(())
        }
    }
}

/// Check the token works. Ok(Some(user)) if /user is readable, Ok(None) if the token
/// authenticated but lacks read:user (or the OAuth consumer lacks the Account permission).
fn verify(client: &Client) -> Result<Option<serde_json::Value>> {
    match client.get("/user") {
        Ok(u) => Ok(Some(u)),
        // 403 (not 401) means the token authenticated but lacks the read:user scope.
        Err(e) if e.to_string().starts_with("HTTP 403") => Ok(None),
        Err(e) => Err(e),
    }
}

fn prompt(label: &str) -> Result<String> {
    eprint!("{label}: ");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn describe(src: Source) -> String {
    match src {
        Source::Env => "environment (BITBUCKET_EMAIL/BITBUCKET_TOKEN)".to_string(),
        Source::Keyring => "system keychain".to_string(),
        Source::File => format!("{} (plaintext)", config::path().display()),
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn login_web(id: Option<String>, secret: Option<String>, port: u16, insecure: bool) -> Result<()> {
    let Some(id) = id.or_else(|| env_nonempty("BB_OAUTH_CLIENT_ID")) else {
        bail!(
            "--web needs a Bitbucket OAuth consumer. Create one under Workspace settings > OAuth consumers \
             with callback URL http://127.0.0.1:{port}/callback and the permissions bb needs \
             (Account: Read; Repositories: Admin; Pull requests: Write; Pipelines: Write), \
             then pass --client-id/--client-secret or set BB_OAUTH_CLIENT_ID/BB_OAUTH_CLIENT_SECRET"
        )
    };
    let secret = match secret.or_else(|| env_nonempty("BB_OAUTH_CLIENT_SECRET")) {
        Some(s) => s,
        None => prompt("OAuth consumer secret")?,
    };
    if secret.is_empty() {
        bail!("the OAuth consumer secret is required");
    }
    let mut creds = oauth::login(&id, &secret, port)?;
    let user = verify(&Client::with_creds(&creds)).context("token verification failed; nothing saved")?;
    let name = user.as_ref().map(|u| (s(u, "username"), s(u, "display_name")));
    // The username keys the keychain entry; without the Account permission there is none to read.
    creds.user = name.as_ref().map(|(u, _)| u.clone()).filter(|u| !u.is_empty()).unwrap_or_else(|| "oauth".into());
    let src = config::save(&creds, insecure)?;
    match name {
        Some((u, d)) => eprintln!("Logged in as {d} ({u})"),
        None => eprintln!("Logged in (the consumer lacks the Account permission; account name unavailable)"),
    }
    eprintln!("Token saved to {}", describe(src));
    Ok(())
}

fn login(email: Option<String>, with_token: bool, insecure: bool) -> Result<()> {
    eprintln!("Create a scoped API token at https://id.atlassian.com/manage-profile/security/api-tokens");
    let email = match email {
        Some(e) => e,
        None if with_token => bail!("--with-token needs --email"),
        None => prompt("Email")?,
    };
    let token = if with_token {
        let mut t = String::new();
        io::stdin().read_to_string(&mut t)?;
        t.trim().to_string()
    } else {
        // ponytail: plain read_line echoes the token; add rpassword-style no-echo if that matters.
        prompt("API token")?
    };
    if email.is_empty() || token.is_empty() {
        bail!("email and token are required");
    }
    let creds = Creds { user: email, token, oauth: None };
    let user = verify(&Client::with_creds(&creds)).context("token verification failed; nothing saved")?;
    let src = config::save(&creds, insecure)?;
    match user {
        Some(u) => eprintln!("Logged in as {} ({})", s(&u, "display_name"), creds.user),
        None => eprintln!("Logged in as {} (token lacks read:user; account name unavailable)", creds.user),
    }
    eprintln!("Token saved to {}", describe(src));
    Ok(())
}

fn status() -> Result<()> {
    let (creds, src) = config::load()?;
    match &creds.oauth {
        None => {
            println!("Credentials: {} (API token)", describe(src));
            println!("Email:       {}", creds.user);
        }
        Some(o) => {
            println!("Credentials: {} (OAuth)", describe(src));
            println!("Consumer:    {}", o.client_id);
            let mins = o.expires_at.saturating_sub(config::now()) / 60;
            println!("Expires:     in {mins}m (refreshed automatically)");
        }
    }
    match verify(&Client::with_creds(&creds)) {
        Ok(Some(u)) => println!("Account:     {} ({})\nToken:       valid", s(&u, "display_name"), s(&u, "username")),
        Ok(None) => println!("Account:     (not readable: token lacks read:user)\nToken:       valid"),
        Err(e) => {
            println!("Token:       invalid");
            return Err(e.context("token check failed"));
        }
    }
    Ok(())
}
