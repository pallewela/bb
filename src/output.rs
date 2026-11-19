//! Output helpers shared by every command.
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

static JSON: AtomicBool = AtomicBool::new(false);

pub fn set_json(on: bool) {
    JSON.store(on, Ordering::Relaxed);
}

/// True when the global --json flag was given: print raw API JSON via `print_json`.
pub fn json_mode() -> bool {
    JSON.load(Ordering::Relaxed)
}

pub fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

/// String at a dotted path (`"author.display_name"`); "" if absent. Numbers/bools are stringified.
pub fn s(v: &Value, path: &str) -> String {
    let mut cur = v;
    for key in path.split('.') {
        cur = &cur[key];
    }
    match cur {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `2026-11-19T10:20:30.123+00:00` -> `2026-11-19 10:20`
pub fn short_time(iso: &str) -> String {
    iso.chars().take(16).collect::<String>().replace('T', " ")
}

/// Print rows as left-aligned columns. First row may be a header; no decoration is added.
pub fn table(rows: &[Vec<String>]) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| rows.iter().filter_map(|r| r.get(c)).map(|x| x.chars().count()).max().unwrap_or(0))
        .collect();
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| if i + 1 == row.len() { cell.clone() } else { format!("{cell:<w$}", w = widths[i]) })
            .collect();
        println!("{}", line.join("  ").trim_end());
    }
}

pub fn open_browser(url: &str) -> Result<()> {
    let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(windows) {
        ("cmd", &["/c", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    std::process::Command::new(cmd).args(args).arg(url).status().with_context(|| format!("could not open {url}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn helpers() {
        let v = json!({"a": {"b": "x", "n": 3}});
        assert_eq!(s(&v, "a.b"), "x");
        assert_eq!(s(&v, "a.n"), "3");
        assert_eq!(s(&v, "a.missing.deep"), "");
        assert_eq!(short_time("2026-11-19T10:20:30.123+00:00"), "2026-11-19 10:20");
    }
}
