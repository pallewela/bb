//! Runs tests/skill/run.sh: the agent skill's recipes against a mocked `bb`, then a check that
//! the real `bb` (this build) accepts every command line they ran. Needs bash and jq.
use std::process::Command;

#[test]
fn skill_recipes() {
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("skipping skill recipe tests: jq is not installed");
        return;
    }
    let dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new("bash")
        .arg(format!("{dir}/tests/skill/run.sh"))
        .env("BB_REAL", env!("CARGO_BIN_EXE_bb"))
        .output()
        .expect("could not run bash");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "tests/skill/run.sh failed:\n{text}{}", String::from_utf8_lossy(&out.stderr));
}
