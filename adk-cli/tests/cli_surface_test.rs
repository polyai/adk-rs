use std::process::Command;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn run_poly(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_poly"))
        .args(args)
        .output()
        .expect("failed to execute poly")
}

fn make_temp_project_dir() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("adk-rs-cli-test-{ts}"));
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    dir.to_string_lossy().to_string()
}

#[test]
fn invalid_subcommand_returns_parser_error() {
    let output = run_poly(&["not-a-command"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn completion_accepts_supported_shells() {
    for shell in ["bash", "zsh", "fish"] {
        let output = run_poly(&["completion", shell]);
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("poly"));
        assert!(stdout.contains("adk"));
    }
}

#[test]
fn status_json_missing_project_matches_contract() {
    let output = run_poly(&["status", "--json", "--path", "/tmp"]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(payload.get("error").is_some());
}

#[test]
fn diff_hash_and_before_after_is_nonfatal() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "diff",
        "--json",
        "--path",
        &project_dir,
        "abc123",
        "--before",
        "main",
    ]);
    // Python keeps this as a non-fatal command-level error.
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Cannot specify both hash and before/after versions."));
}
