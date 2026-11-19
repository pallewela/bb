//! `bb api`: authenticated raw request.
use crate::client::Client;
use crate::output::print_json;
use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};
use std::io::Read;

#[derive(clap::Args)]
pub struct Args {
    /// API path relative to /2.0, or a full URL
    path: String,
    /// HTTP method (default GET, or POST when a body is given)
    #[arg(short = 'X', long)]
    method: Option<String>,
    /// Add a string field to a JSON object body (repeatable)
    #[arg(short = 'f', long = "field", value_name = "KEY=VALUE", conflicts_with = "input")]
    fields: Vec<String>,
    /// Raw JSON body from FILE, or `-` for stdin
    #[arg(long, value_name = "FILE")]
    input: Option<String>,
    /// Follow `next` links and print the concatenated `values` array (GET only)
    #[arg(long, conflicts_with_all = ["method", "fields", "input"])]
    paginate: bool,
}

pub fn run(args: Args) -> Result<()> {
    let client = Client::new()?;
    if args.paginate {
        print_json(&Value::Array(client.paginate(&args.path, usize::MAX)?));
        return Ok(());
    }
    let body = match &args.input {
        Some(f) => {
            let text = if f == "-" {
                let mut t = String::new();
                std::io::stdin().read_to_string(&mut t)?;
                t
            } else {
                std::fs::read_to_string(f).with_context(|| format!("reading {f}"))?
            };
            Some(serde_json::from_str(&text).context("--input is not valid JSON")?)
        }
        None if !args.fields.is_empty() => Some(fields_body(&args.fields)?),
        None => None,
    };
    let method = args.method.unwrap_or_else(|| if body.is_some() { "POST" } else { "GET" }.into());
    let text = client.request(&method.to_uppercase(), &args.path, body.as_ref())?;
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => print_json(&v),
        Err(_) => print!("{text}"),
    }
    Ok(())
}

/// `["a=1", "b=x=y"]` -> `{"a": "1", "b": "x=y"}`
fn fields_body(fields: &[String]) -> Result<Value> {
    let mut m = Map::new();
    for f in fields {
        let (k, v) = f.split_once('=').ok_or_else(|| anyhow!("-f expects key=value, got `{f}`"))?;
        m.insert(k.to_string(), Value::String(v.to_string()));
    }
    Ok(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_field_body() {
        let f = |v: &[&str]| fields_body(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(f(&["a=1", "b=x=y", "c="]).unwrap(), json!({"a": "1", "b": "x=y", "c": ""}));
        assert!(f(&["nope"]).is_err());
    }
}
