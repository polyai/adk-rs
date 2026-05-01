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

fn make_temp_invalid_yaml_project_dir() -> String {
    let dir = make_temp_project_dir();
    let p = std::path::PathBuf::from(&dir);
    fs::create_dir_all(p.join("topics")).expect("mkdir topics");
    fs::write(
        p.join("topics/bad.yaml"),
        "name: bad\ncontent: [unterminated\n",
    )
    .expect("write invalid yaml");
    dir
}

fn make_temp_unformatted_json_project_dir() -> String {
    let dir = make_temp_project_dir();
    let p = std::path::PathBuf::from(&dir);
    fs::write(p.join("sample.json"), "{\"b\":2,\"a\":1}").expect("write unformatted json");
    dir
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

#[test]
fn validate_json_reports_parse_errors() {
    let project_dir = make_temp_invalid_yaml_project_dir();
    let output = run_poly(&["validate", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    let errors = payload
        .get("errors")
        .and_then(|v| v.as_array())
        .expect("errors array");
    assert!(!errors.is_empty(), "expected validation errors");
}

#[test]
fn format_check_json_reports_unformatted_files() {
    let project_dir = make_temp_unformatted_json_project_dir();
    let output = run_poly(&["format", "--json", "--check", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    let changed = payload
        .get("changed_files")
        .and_then(|v| v.as_array())
        .expect("changed_files array");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].as_str(), Some("sample.json"));
}

#[test]
fn branch_current_reads_branch_from_project_config() {
    let project_dir = make_temp_project_dir();
    let p = std::path::PathBuf::from(&project_dir);
    fs::write(
        p.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: feature-x\n",
    )
    .expect("rewrite config");
    let output = run_poly(&["branch", "current", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("branch").and_then(|v| v.as_str()),
        Some("feature-x")
    );
}

#[test]
fn branch_switch_updates_project_branch() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "branch",
        "switch",
        "--json",
        "--path",
        &project_dir,
        "feature-y",
    ]);
    assert_eq!(output.status.code(), Some(0));

    let output2 = run_poly(&["branch", "current", "--json", "--path", &project_dir]);
    assert_eq!(output2.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output2.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("branch").and_then(|v| v.as_str()),
        Some("feature-y")
    );
}

#[test]
fn review_create_list_delete_roundtrip() {
    let project_dir = make_temp_project_dir();

    let create = run_poly(&[
        "review",
        "--json",
        "--path",
        &project_dir,
        "create",
        "--before",
        "main",
        "--after",
        "feature",
    ]);
    assert_eq!(create.status.code(), Some(0));
    let create_payload: serde_json::Value =
        serde_json::from_slice(&create.stdout).expect("valid JSON output");
    let review_id = create_payload
        .get("review")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("review id")
        .to_string();

    let list = run_poly(&["review", "--json", "--path", &project_dir, "list"]);
    assert_eq!(list.status.code(), Some(0));
    let list_payload: serde_json::Value =
        serde_json::from_slice(&list.stdout).expect("valid JSON output");
    let gists = list_payload
        .get("gists")
        .and_then(|v| v.as_array())
        .expect("gists array");
    assert_eq!(gists.len(), 1);

    let delete = run_poly(&[
        "review",
        "--json",
        "--path",
        &project_dir,
        "delete",
        "--id",
        &review_id,
    ]);
    assert_eq!(delete.status.code(), Some(0));

    let list_after = run_poly(&["review", "--json", "--path", &project_dir, "list"]);
    assert_eq!(list_after.status.code(), Some(0));
    let list_after_payload: serde_json::Value =
        serde_json::from_slice(&list_after.stdout).expect("valid JSON output");
    let gists_after = list_after_payload
        .get("gists")
        .and_then(|v| v.as_array())
        .expect("gists array");
    assert!(gists_after.is_empty());
}
