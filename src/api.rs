//! `bb api`: authenticated raw request.
use crate::client::{Client, enc, status_error, url};
use crate::output::print_json;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value};
use std::io::Read;

#[derive(clap::Args)]
pub struct Args {
    /// API path relative to /2.0, or a full URL
    path: String,
    /// HTTP method (default GET, or POST when a body or fields are given)
    #[arg(short = 'X', long)]
    method: Option<String>,
    /// Add a string field (repeatable). Fields form a JSON body, or the query string for GET or with --input
    #[arg(short = 'f', long = "raw-field", value_name = "KEY=VALUE")]
    raw_fields: Vec<String>,
    /// Add a typed field (repeatable): true/false/null/integers become JSON values; `@FILE` or `@-` reads the value
    #[arg(short = 'F', long = "field", value_name = "KEY=VALUE")]
    fields: Vec<String>,
    /// Add a request header (repeatable)
    #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
    headers: Vec<String>,
    /// Raw JSON body from FILE, or `-` for stdin
    #[arg(long, value_name = "FILE")]
    input: Option<String>,
    /// Print the response status line and headers before the body
    #[arg(short, long)]
    include: bool,
    /// Do not print the response body
    #[arg(long)]
    silent: bool,
    /// Print the request and response headers to stderr (the Authorization header is never shown)
    #[arg(long)]
    verbose: bool,
    /// Follow `next` links and print the concatenated `values` array (GET only)
    #[arg(long, conflicts_with_all = ["method", "raw_fields", "fields", "input", "headers", "include", "verbose"])]
    paginate: bool,
}

fn read_source(src: &str, what: &str) -> Result<String> {
    if src == "-" {
        let mut t = String::new();
        std::io::stdin().read_to_string(&mut t)?;
        Ok(t)
    } else {
        std::fs::read_to_string(src).with_context(|| format!("reading {what} {src}"))
    }
}

pub fn run(args: Args) -> Result<()> {
    let client = Client::new()?;
    if args.paginate {
        let items = client.paginate(&args.path, usize::MAX)?;
        if !args.silent {
            print_json(&Value::Array(items));
        }
        return Ok(());
    }
    let fields = fields_map(&args.raw_fields, &args.fields)?;
    let headers = args.headers.iter().map(|h| parse_header(h)).collect::<Result<Vec<_>>>()?;
    let input = match &args.input {
        Some(f) => {
            Some(serde_json::from_str::<Value>(&read_source(f, "--input")?).context("--input is not valid JSON")?)
        }
        None => None,
    };
    let method = match args.method {
        Some(m) => m.to_uppercase(),
        None if input.is_some() || !fields.is_empty() => "POST".into(),
        None => "GET".into(),
    };
    // As in `gh api`: fields go in the query string for GET, or when --input supplies the body.
    let (path, body) = match input {
        Some(b) => (with_query(&args.path, &fields), Some(b)),
        None if method == "GET" || fields.is_empty() => (with_query(&args.path, &fields), None),
        None => (args.path.clone(), Some(Value::Object(fields))),
    };
    if args.verbose {
        let mut sent = headers.clone();
        if body.is_some() && !sent.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            sent.push(("Content-Type".into(), "application/json".into()));
        }
        eprint!("{}", head("> ", &format!("{method} {}", url(&path)), &sent));
        if let Some(b) = &body {
            eprintln!("{}", serde_json::to_string_pretty(b)?);
        }
    }
    let res = client.send(&method, &path, &headers, body.as_ref())?;
    if args.verbose {
        eprint!("{}", head("< ", &res.status_line, &res.headers));
    }
    if args.include {
        print!("{}", head("", &res.status_line, &res.headers));
    }
    if !args.silent {
        match serde_json::from_str::<Value>(&res.body) {
            Ok(v) => print_json(&v),
            Err(_) => print!("{}", res.body),
        }
    }
    if !(200..300).contains(&res.status) {
        return Err(status_error(res.status, &res.body));
    }
    Ok(())
}

/// Status line and headers, each line prefixed, then a blank line. Never includes Authorization.
fn head(prefix: &str, first: &str, headers: &[(String, String)]) -> String {
    let mut out = format!("{prefix}{first}\n");
    for (k, v) in headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("authorization")) {
        out += &format!("{prefix}{k}: {v}\n");
    }
    out + prefix.trim_end() + "\n"
}

/// `"Accept: text/plain"` -> `("Accept", "text/plain")`
fn parse_header(h: &str) -> Result<(String, String)> {
    match h.split_once(':') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_string(), v.trim().to_string())),
        _ => bail!("-H expects key:value, got `{h}`"),
    }
}

/// `-f` fields are strings; `-F` fields are typed (see `typed_value`). Later keys replace earlier ones.
fn fields_map(raw: &[String], typed: &[String]) -> Result<Map<String, Value>> {
    let mut m = Map::new();
    for (f, is_typed) in raw.iter().map(|f| (f, false)).chain(typed.iter().map(|f| (f, true))) {
        let (k, v) = f
            .split_once('=')
            .ok_or_else(|| anyhow!("{} expects key=value, got `{f}`", if is_typed { "-F" } else { "-f" }))?;
        let v = if is_typed { typed_value(v)? } else { Value::String(v.to_string()) };
        m.insert(k.to_string(), v);
    }
    Ok(m)
}

/// As `gh api -F`: `true`, `false`, `null` and integers become JSON values; `@FILE` (or `@-` for stdin)
/// reads the value from a file; anything else is a string.
fn typed_value(v: &str) -> Result<Value> {
    Ok(match v {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ if v.starts_with('@') => Value::String(read_source(&v[1..], "-F")?),
        _ => v.parse::<i64>().map_or_else(|_| Value::String(v.to_string()), Value::from),
    })
}

/// Append fields to a path as query parameters.
fn with_query(path: &str, fields: &Map<String, Value>) -> String {
    let params: Vec<String> = fields
        .iter()
        .map(|(k, v)| {
            let v = match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            format!("{}={}", enc(k), enc(&v))
        })
        .collect();
    if params.is_empty() {
        return path.to_string();
    }
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("{path}{sep}{}", params.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builds_field_body() {
        let f = |raw: &[&str], typed: &[&str]| fields_map(&strs(raw), &strs(typed)).map(Value::Object);
        assert_eq!(f(&["a=1", "b=x=y", "c="], &[]).unwrap(), json!({"a": "1", "b": "x=y", "c": ""}));
        assert_eq!(
            f(&["s=true"], &["n=42", "neg=-3", "t=true", "x=false", "z=null", "fl=1.5", "w=hi"]).unwrap(),
            json!({"s": "true", "n": 42, "neg": -3, "t": true, "x": false, "z": null, "fl": "1.5", "w": "hi"})
        );
        assert!(f(&["nope"], &[]).is_err());
        assert!(f(&[], &["nope"]).is_err());
    }

    #[test]
    fn typed_value_reads_files() {
        let p = std::env::temp_dir().join(format!("bb-api-test-{}", std::process::id()));
        std::fs::write(&p, "file body\n").unwrap();
        assert_eq!(typed_value(&format!("@{}", p.display())).unwrap(), json!("file body\n"));
        std::fs::remove_file(&p).unwrap();
        assert!(typed_value(&format!("@{}", p.display())).is_err());
    }

    #[test]
    fn query_strings() {
        let m = fields_map(&strs(&["q=state=\"OPEN\""]), &strs(&["pagelen=5", "x=null"])).unwrap();
        assert_eq!(with_query("/p", &m), "/p?pagelen=5&q=state%3D%22OPEN%22&x=");
        assert_eq!(with_query("/p?a=1", &m), "/p?a=1&pagelen=5&q=state%3D%22OPEN%22&x=");
        assert_eq!(with_query("/p", &Map::new()), "/p");
    }

    #[test]
    fn headers() {
        assert_eq!(parse_header("Accept: text/plain").unwrap(), ("Accept".into(), "text/plain".into()));
        assert_eq!(parse_header("X-A:b:c").unwrap(), ("X-A".into(), "b:c".into()));
        assert!(parse_header("nocolon").is_err());
        assert!(parse_header(": v").is_err());
        let h = vec![("content-type".into(), "application/json".into()), ("Authorization".into(), "Basic x".into())];
        assert_eq!(head("", "HTTP/1.1 200 OK", &h), "HTTP/1.1 200 OK\ncontent-type: application/json\n\n");
        assert_eq!(head("< ", "HTTP/1.1 200 OK", &h), "< HTTP/1.1 200 OK\n< content-type: application/json\n<\n");
    }
}
