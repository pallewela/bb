//! API JSON handling: parsing a response body, `--json` printing (`print_json`), dotted field
//! lookups (`output::s`, used for every human-readable field) and error-message extraction
//! (`client::status_error`). Input: first line is a dotted path, the rest is the response body.
#![no_main]
use bitbucket_bb_cli::{client::status_error, output};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let (path, body) = text.split_once('\n').unwrap_or((text, ""));

    let _ = status_error(500, body).to_string();
    let _ = output::short_time(body);

    let Ok(v) = serde_json::from_str::<Value>(body) else { return };
    // `print_json` unwraps this, so it must never fail for a parsed value.
    let pretty = serde_json::to_string_pretty(&v).expect("to_string_pretty");
    serde_json::from_str::<Value>(&pretty).expect("--json output is not valid JSON");

    let field = output::s(&v, path);
    let _ = output::table_string(&[vec![path.to_string(), field.clone()], vec![field, String::new()]]);
});
