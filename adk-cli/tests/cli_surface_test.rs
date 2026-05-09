mod support;

use std::fs;
use support::cli::{
    make_temp_invalid_yaml_project_dir as support_temp_invalid_yaml_project_dir,
    make_temp_project_dir as support_temp_project_dir,
    make_temp_unformatted_json_project_dir as support_temp_unformatted_json_project_dir,
    run_poly_offline, run_poly_without_fallback,
};

fn run_poly(args: &[&str]) -> std::process::Output {
    run_poly_offline(args)
}

fn make_temp_project_dir() -> String {
    support_temp_project_dir("adk-rs-cli-test")
}

fn make_temp_invalid_yaml_project_dir() -> String {
    support_temp_invalid_yaml_project_dir("adk-rs-cli-test")
}

fn make_temp_unformatted_json_project_dir() -> String {
    support_temp_unformatted_json_project_dir("adk-rs-cli-test")
}

fn sample_projection_json() -> &'static str {
    r#"{"knowledgeBase":{"topics":{"entities":{"topic-1":{"name":"Welcome","isActive":true,"actions":"","content":"Hello there","exampleQueries":[{"query":"hi"}]}}}}}"#
}

#[test]
fn invalid_subcommand_returns_parser_error() {
    let output = run_poly(&["not-a-command"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn version_accepts_python_short_flag_and_output_shape() {
    for flag in ["-v", "--version"] {
        let output = run_poly(&[flag]);
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), env!("CARGO_PKG_VERSION"));
    }

    let output = run_poly(&["-V"]);
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
fn status_json_uses_python_payload_shape() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["status", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert!(payload.get("conflict_detection_available").is_none());
    assert!(payload.get("files_with_conflicts").is_some());
    assert!(payload.get("modified_files").is_some());
    assert!(payload.get("new_files").is_some());
    assert!(payload.get("deleted_files").is_some());
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
fn diff_files_accepts_python_nargs_style() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "diff",
        "--json",
        "--path",
        &project_dir,
        "--files",
        "sample-a.yaml",
        "sample-b.yaml",
    ]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn pull_from_projection_writes_resources_and_echoes_projection() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "pull",
        "--json",
        "--output-json-projection",
        "--path",
        &project_dir,
        "--from-projection",
        sample_projection_json(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert!(payload.get("projection").is_some());
    let topic = std::path::PathBuf::from(&project_dir).join("topics/welcome.yaml");
    let content = fs::read_to_string(topic).expect("topic written from projection");
    assert!(content.contains("Hello there"));
}

#[test]
fn push_from_projection_rejects_non_object_json() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "push",
        "--json",
        "--path",
        &project_dir,
        "--from-projection",
        "[]",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(
        payload
            .get("error")
            .and_then(|v| v.as_str())
            .is_some_and(|message| message.contains("--from-projection must be a JSON object"))
    );
}

#[test]
fn branch_switch_from_projection_updates_branch_and_writes_resources() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "branch",
        "switch",
        "--json",
        "--output-json-projection",
        "--path",
        &project_dir,
        "--from-projection",
        sample_projection_json(),
        "feature-projection",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("branch_name").and_then(|v| v.as_str()),
        Some("feature-projection")
    );
    assert!(payload.get("projection").is_some());
    let topic = std::path::PathBuf::from(&project_dir).join("topics/welcome.yaml");
    assert!(topic.exists());
}

#[test]
fn review_subcommands_accept_json_after_subcommand() {
    let project_dir = make_temp_project_dir();
    for args in [
        vec!["review", "--path", &project_dir, "create", "--json"],
        vec!["review", "list", "--json"],
        vec!["review", "delete", "--json"],
    ] {
        let output = run_poly(&args);
        assert_eq!(output.status.code(), Some(0));
        let payload: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid JSON output");
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false)
        );
    }
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
    let error = payload
        .get("error")
        .and_then(|v| v.as_str())
        .expect("error string");
    assert!(error.contains("Error reading resource bad at"));
    assert!(error.contains("Error loading YAML file:"));
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
        .get("affected")
        .and_then(|v| v.as_array())
        .expect("affected array");
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].as_str(), Some("sample.json"));
    assert_eq!(
        payload.get("check_only").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(payload.get("format_errors").is_some());
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
        payload.get("current_branch").and_then(|v| v.as_str()),
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
        payload.get("current_branch").and_then(|v| v.as_str()),
        Some("feature-y")
    );
}

#[test]
fn branch_create_env_force_uses_hotfix_path() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&[
        "branch",
        "create",
        "--json",
        "--path",
        &project_dir,
        "--env",
        "live",
        "--force",
        "hotfix-branch",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("branch_name").and_then(|v| v.as_str()),
        Some("hotfix-branch")
    );
}

#[test]
fn review_json_reports_missing_github_token() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["review", "--json", "--path", &project_dir, "list"]);
    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    let message = payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(message.contains("GITHUB_ACCESS_TOKEN"));
}

#[test]
fn review_text_reports_missing_github_token() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["review", "--path", &project_dir, "list"]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("GITHUB_ACCESS_TOKEN"));
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
