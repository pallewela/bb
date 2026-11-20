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
    print!("{}", table_string(rows));
}

/// `table`'s output as a string (one `\n`-terminated line per row).
pub fn table_string(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| rows.iter().filter_map(|r| r.get(c)).map(|x| x.chars().count()).max().unwrap_or(0))
        .collect();
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| if i + 1 == row.len() { cell.clone() } else { format!("{cell:<w$}", w = widths[i]) })
            .collect();
        out += line.join("  ").trim_end();
        out.push('\n');
    }
    out
}

/// After a list that stopped at its `-L` limit, say so on stderr (keeps pipes and --json clean).
/// `noun` is plural ("repositories"); `order` describes the sort ("newest first") or is "".
pub fn more_note(list: &crate::client::Listing, noun: &str, order: &str) {
    if list.truncated && !json_mode() {
        eprintln!("{}", more_text(list.items.len(), list.total, noun, order));
    }
}

fn more_text(shown: usize, total: Option<u64>, noun: &str, order: &str) -> String {
    let what = match total {
        Some(t) => format!("Showing {shown} of {t} {noun}"),
        None => format!("Showing the first {shown} {noun}"),
    };
    let order = if order.is_empty() { String::new() } else { format!(", {order}") };
    format!("{what}{order}. Use -L N to see more, or -L 0 for all.")
}

pub fn open_browser(url: &str) -> Result<()> {
    let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(windows) {
        // Not `cmd /c start`: cmd treats `&` in the URL (e.g. `?state=OPEN&...`) as a command separator.
        ("rundll32", &["url.dll,FileProtocolHandler"])
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
        assert_eq!(
            more_text(30, Some(3030), "repositories", "most recently updated first"),
            "Showing 30 of 3030 repositories, most recently updated first. Use -L N to see more, or -L 0 for all."
        );
        assert_eq!(
            more_text(30, None, "pull requests", ""),
            "Showing the first 30 pull requests. Use -L N to see more, or -L 0 for all."
        );
    }
}
