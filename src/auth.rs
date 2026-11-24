//! `bb auth`: login, status, logout, token.
use crate::client::Client;
use crate::config::{self, Creds, Source};
use crate::output::s;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use std::io::{self, BufRead, Read, Write};

#[derive(Subcommand)]
pub enum Cmd {
    /// Verify an Atlassian email + API token and save them (token in the system keychain)
    Login {
        /// Atlassian account email (prompted if absent)
        #[arg(long)]
        email: Option<String>,
        /// Read the API token from stdin
        #[arg(long)]
        with_token: bool,
        /// Store the token in plaintext in the config file instead of the system keychain
        #[arg(long)]
        insecure_storage: bool,
    },
    /// Show where credentials come from and whether they work
    Status {
        /// Also print the API token
        #[arg(short = 't', long)]
        show_token: bool,
    },
    /// Delete the saved login (keychain entry and config file)
    Logout,
    /// Print the API token (for scripting)
    Token,
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Login { email, with_token, insecure_storage } => login(email, with_token, insecure_storage),
        Cmd::Status { show_token } => status(show_token),
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

/// Check the token works. Ok(Some(name)) if /user is readable, Ok(None) if only the
/// repository fallback works (scoped token without read:user).
fn verify(client: &Client) -> Result<Option<String>> {
    match client.get("/user") {
        Ok(u) => Ok(Some(s(&u, "display_name"))),
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
    let creds = Creds { email, token };
    let name = verify(&Client::with_creds(&creds)).context("token verification failed; nothing saved")?;
    let src = config::save(&creds, insecure)?;
    match name {
        Some(n) => eprintln!("Logged in as {n} ({})", creds.email),
        None => eprintln!("Logged in as {} (token lacks read:user; account name unavailable)", creds.email),
    }
    eprintln!("Token saved to {}", describe(src));
    Ok(())
}

fn status(show_token: bool) -> Result<()> {
    let (creds, src) = config::load()?;
    println!("Credentials: {}", describe(src));
    println!("Email:       {}", creds.email);
    if show_token {
        println!("Token value: {}", creds.token);
    }
    match verify(&Client::with_creds(&creds)) {
        Ok(Some(name)) => println!("Account:     {name}\nToken:       valid"),
        Ok(None) => println!("Account:     (not readable: token lacks read:user)\nToken:       valid"),
        Err(e) => {
            println!("Token:       invalid");
            return Err(e.context("token check failed"));
        }
    }
    Ok(())
}
