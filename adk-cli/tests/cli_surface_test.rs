use std::process::Command;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn run_poly(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_poly"))
        .env_remove("POLY_ADK_KEY")
        .env("POLY_ADK_ALLOW_INMEMORY_FALLBACK", "1")
        .args(args)
        .output()
        .expect("failed to execute poly")
}

fn run_poly_without_fallback(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_poly"))
        .env_remove("POLY_ADK_KEY")
        .env_remove("POLY_ADK_ALLOW_INMEMORY_FALLBACK")
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
fn status_json_includes_conflict_detection_availability() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["status", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload
            .get("conflict_detection_available")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
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
fn review_json_is_explicitly_unsupported() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["review", "--json", "--path", &project_dir, "list"]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    let error = payload.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(error.contains("not yet supported"));
}

#[test]
fn review_text_is_explicitly_unsupported() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["review", "--path", &project_dir, "list"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not yet supported"));
}

#[test]
fn revert_json_returns_files_reverted_payload() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["revert", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert!(
        payload
            .get("files_reverted")
            .and_then(|v| v.as_array())
            .is_some()
    );
}

#[test]
fn revert_text_prints_no_changes_when_nothing_reverted() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["revert", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No changes to revert."));
}

#[test]
fn pull_requires_remote_or_explicit_fallback_opt_in() {
    let project_dir = make_temp_project_dir();
    let output = run_poly_without_fallback(&["pull", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let error = payload.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(error.contains("remote platform client unavailable"));
    assert!(error.contains("POLY_ADK_ALLOW_INMEMORY_FALLBACK"));
}

#[test]
fn docs_without_arguments_prints_root_docs() {
    let output = run_poly(&["docs"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# Poly ADK"));
}

#[test]
fn docs_output_writes_file_and_reports_path() {
    let project_dir = make_temp_project_dir();
    let output_path = std::path::PathBuf::from(&project_dir)
        .join("nested")
        .join("docs.md");
    let output = run_poly(&[
        "docs",
        "functions",
        "--output",
        output_path.to_string_lossy().as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Documentation written to"));
    let written = fs::read_to_string(output_path).expect("read written docs");
    assert!(written.contains("Function"));
}

#[test]
fn docs_rejects_unknown_document_topic_with_parser_error() {
    let output = run_poly(&["docs", "not-a-real-doc"]);
    assert_eq!(output.status.code(), Some(2));
}
