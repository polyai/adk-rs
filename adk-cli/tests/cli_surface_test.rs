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

#[test]
fn system_tool_dependencies_are_available() {
    for (tool, args) in [("ruff", &["--version"][..]), ("ty", &["version"][..])] {
        let output = std::process::Command::new(tool)
            .args(args)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "{tool} must be installed on PATH for the full ADK parity test suite: {error}"
                )
            });
        assert!(
            output.status.success(),
            "{tool} version check failed with status {:?}",
            output.status.code()
        );
    }
}

fn sample_projection_json() -> &'static str {
    r#"{"knowledgeBase":{"topics":{"entities":{"topic-1":{"name":"Welcome","isActive":true,"actions":"","content":"Hello there","exampleQueries":[{"query":"hi"}]}}}}}"#
}

fn unformatted_function_projection_json() -> String {
    serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "name": "format_local",
                        "description": "Format a pulled function.",
                        "code": "def format_local(conv: Conversation):\n    payload={\"b\":2,\"a\":1}\n    return payload\n"
                    }
                }
            }
        }
    })
    .to_string()
}

fn assert_formatted_function_and_clean_status(project_dir: &str) {
    let function_path = std::path::PathBuf::from(project_dir).join("functions/format_local.py");
    let content = fs::read_to_string(function_path).expect("formatted function");
    assert!(content.contains("payload = {\"b\": 2, \"a\": 1}"));

    let status = run_poly(&["status", "--json", "--path", project_dir]);
    assert_eq!(status.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("valid status JSON output");
    let modified = payload
        .get("modified_files")
        .and_then(|v| v.as_array())
        .expect("modified_files array");
    assert!(modified.is_empty());
}

#[test]
fn invalid_subcommand_returns_parser_error() {
    let output = run_poly(&["not-a-command"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn top_level_help_matches_python_command_surface() {
    let output = run_poly(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);

    for expected in ["poly", "-h", "--help", "-v", "--version"] {
        assert!(
            stdout.contains(expected),
            "expected help to contain {expected:?}\nstdout={stdout}"
        );
    }

    for (command, description) in [
        ("docs", "Outputs documentation for a given topic."),
        ("init", "Initialize a new Agent Studio project."),
        ("project", "Manage Agent Studio projects."),
        (
            "pull",
            "Pull the latest project configuration from Agent Studio.",
        ),
        ("push", "Push the project configuration to Agent Studio."),
        ("status", "Check the changed files of the project."),
        ("revert", "Revert changes in the project."),
        ("diff", "Show the changes made to the project."),
        (
            "review",
            "Create a GitHub Gist of Agent Studio project changes to share changes.",
        ),
        ("branch", "Manage branches in the Agent Studio project."),
        (
            "format",
            "Run ruff and YAML/JSON formatting on the project (optional ty with --ty).",
        ),
        ("validate", "Validate the project configuration locally."),
        ("chat", "Start an interactive chat session with the agent."),
        ("completion", "Generate shell completion scripts"),
        ("deployments", "Manage deployments for the project."),
    ] {
        assert!(
            stdout.contains(command),
            "expected help to contain command {command:?}\nstdout={stdout}"
        );
        assert!(
            stdout.contains(description),
            "expected help to contain description {description:?}\nstdout={stdout}"
        );
    }

    assert!(!stdout.contains("Agent Development Kit (Rust)"));
}

#[test]
fn project_create_json_requires_python_noninteractive_arguments() {
    let output = run_poly(&["project", "create", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        payload.get("error").and_then(|v| v.as_str()),
        Some("create project with --json requires --region, --account_id, and --name.")
    );
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
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert!(payload.get("conflict_detection_available").is_none());
    assert!(payload.get("files_with_conflicts").is_some());
    assert!(payload.get("modified_files").is_some());
    assert!(payload.get("new_files").is_some());
    assert!(payload.get("deleted_files").is_some());
}

#[test]
fn status_json_does_not_require_remote_fallback() {
    let project_dir = make_temp_project_dir();
    let output = run_poly_without_fallback(&["status", "--json", "--path", &project_dir]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert!(
        payload
            .get("modified_files")
            .and_then(|v| v.as_array())
            .is_some_and(Vec::is_empty)
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
fn pull_from_projection_preserves_current_branch_config() {
    let project_dir = make_temp_project_dir();
    let project_root = std::path::PathBuf::from(&project_dir);
    fs::write(
        project_root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: BRANCH-feature\n",
    )
    .expect("rewrite config");

    let output = run_poly(&[
        "pull",
        "--json",
        "--path",
        &project_dir,
        "--from-projection",
        sample_projection_json(),
    ]);

    assert_eq!(output.status.code(), Some(0));
    let project_yaml = fs::read_to_string(project_root.join("project.yaml")).expect("config");
    assert!(project_yaml.contains("branch_id: BRANCH-feature"));
}

#[test]
fn pull_from_projection_format_formats_python_and_baselines_snapshot() {
    let project_dir = make_temp_project_dir();
    let projection_arg = unformatted_function_projection_json();

    let output = run_poly(&[
        "pull",
        "--json",
        "--format",
        "--path",
        &project_dir,
        "--from-projection",
        &projection_arg,
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert_formatted_function_and_clean_status(&project_dir);
}

#[test]
fn init_from_projection_format_formats_python_and_baselines_snapshot() {
    let base_dir = support::cli::temp_dir("adk-rs-cli-init-format-test");
    fs::create_dir_all(&base_dir).expect("mkdir base dir");
    let base_dir_arg = base_dir.to_string_lossy().to_string();
    let projection_arg = unformatted_function_projection_json();

    let output = run_poly(&[
        "init",
        "--json",
        "--format",
        "--base-path",
        &base_dir_arg,
        "--region",
        "us-1",
        "--account_id",
        "test-account",
        "--project_id",
        "test-project",
        "--from-projection",
        &projection_arg,
    ]);

    assert_eq!(output.status.code(), Some(0));
    let project_dir = base_dir.join("test-account/test-project");
    assert_formatted_function_and_clean_status(&project_dir.to_string_lossy());
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
fn pull_output_json_projection_does_not_force_json_error_mode() {
    let missing_project_dir = support::cli::temp_dir("adk-rs-missing-projection-project");
    fs::create_dir_all(&missing_project_dir).expect("mkdir missing project dir");
    let missing_project_arg = missing_project_dir.to_string_lossy().to_string();

    let output = run_poly(&[
        "pull",
        "--output-json-projection",
        "--path",
        &missing_project_arg,
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No project configuration found"));
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
fn branch_switch_from_projection_format_formats_python_and_baselines_snapshot() {
    let project_dir = make_temp_project_dir();
    let projection_arg = unformatted_function_projection_json();

    let output = run_poly(&[
        "branch",
        "switch",
        "--json",
        "--format",
        "--path",
        &project_dir,
        "--from-projection",
        &projection_arg,
        "feature-format",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert_formatted_function_and_clean_status(&project_dir);
}

#[test]
fn branch_switch_output_json_projection_returns_remote_projection_without_from_projection() {
    let project_dir = make_temp_project_dir();

    let output = run_poly(&[
        "branch",
        "switch",
        "--output-json-projection",
        "--path",
        &project_dir,
        "feature-y",
    ]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("branch_name").and_then(|v| v.as_str()),
        Some("feature-y")
    );
    assert!(
        payload
            .get("projection")
            .is_some_and(|value| !value.is_null())
    );
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
fn validate_json_reports_duplicate_names_and_invalid_entity_types() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    fs::write(
        root.join("config/entities.yaml"),
        "entities:\n  - name: customer\n    entity_type: unsupported\n  - name: customer\n    entity_type: enum\n",
    )
    .expect("write invalid entities");

    let output = run_poly(&["validate", "--json", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("valid").and_then(|v| v.as_bool()), Some(false));
    let errors = payload
        .get("errors")
        .and_then(|v| v.as_array())
        .expect("errors array")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate entity name 'customer'"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("unsupported entity_type 'unsupported'"))
    );
}

#[test]
fn validate_json_reports_python_function_syntax_errors() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(
        root.join("functions/bad_global.py"),
        "from _gen import *  # <AUTO GENERATED>\n\n\ndef bad_global(conv: Conversation):\n    if True\n        return None\n",
    )
    .expect("write invalid global function");

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
    assert!(error.contains("Error reading resource bad_global"));
    assert!(error.contains("functions/bad_global.py"));
}

#[test]
fn validate_json_reports_transition_function_signature_errors() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("flows/bad_flow/functions")).expect("mkdir transition functions");
    fs::write(
        root.join("flows/bad_flow/functions/route_account.py"),
        "from _gen import *  # <AUTO GENERATED>\n\n\ndef route_account(conv: Conversation):\n    return None\n",
    )
    .expect("write invalid transition function");

    let output = run_poly(&["validate", "--json", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("valid").and_then(|v| v.as_bool()), Some(false));
    let errors = payload
        .get("errors")
        .and_then(|v| v.as_array())
        .expect("errors array")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert!(errors.iter().any(|error| {
        error.contains("flows/bad_flow/functions/route_account.py")
            && error.contains(
                "Function definition 'def route_account(conv: Conversation, flow: Flow)' not found",
            )
    }));
}

#[test]
fn validate_json_reports_function_decorator_parameter_errors() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(
        root.join("functions/book_table.py"),
        "from _gen import *  # <AUTO GENERATED>\n\n\n@func_description('Book a table')\n@func_parameter('booking_ref', 'The booking reference')\ndef book_table(conv: Conversation, booking_ref):\n    return booking_ref\n",
    )
    .expect("write invalid decorator function");

    let output = run_poly(&["validate", "--json", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let error = payload
        .get("error")
        .and_then(|v| v.as_str())
        .expect("error string");
    assert!(error.contains("Error reading resource book_table"));
    assert!(
        error.contains("Parameter 'booking_ref' has no type annotation"),
        "error was: {error}"
    );
}

#[test]
fn format_check_json_reports_unformatted_json_files() {
    let project_dir = make_temp_unformatted_json_project_dir();
    let output = run_poly(&[
        "format",
        "--json",
        "--check",
        "--path",
        &project_dir,
        "--files",
        "sample.json",
    ]);
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
    assert_eq!(changed, &[serde_json::json!("sample.json")]);
    assert_eq!(
        payload.get("check_only").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(payload.get("format_errors").is_some());
}

#[test]
fn format_files_accepts_absolute_paths_and_formats_json() {
    let project_dir = make_temp_unformatted_json_project_dir();
    let json_path = std::path::PathBuf::from(&project_dir).join("sample.json");
    let json_path_arg = json_path.to_string_lossy().to_string();

    let output = run_poly(&[
        "format",
        "--json",
        "--path",
        &project_dir,
        "--files",
        &json_path_arg,
    ]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("affected").and_then(|v| v.as_array()),
        Some(&vec![serde_json::json!("sample.json")])
    );
    assert_eq!(
        fs::read_to_string(json_path).expect("formatted json"),
        "{\n  \"a\": 1,\n  \"b\": 2\n}\n"
    );
}

#[test]
fn format_ty_json_runs_type_checker() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("functions")).expect("create functions dir");
    fs::write(
        root.join("functions/typecheck_ok.py"),
        "def add(x: int, y: int) -> int:\n    return x + y\n",
    )
    .expect("write typed python file");

    let output = run_poly(&[
        "format",
        "--json",
        "--check",
        "--ty",
        "--path",
        &project_dir,
    ]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(payload.get("ty_ran").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("ty_returncode").and_then(|v| v.as_i64()),
        Some(0)
    );
    assert_eq!(
        payload.get("ty_timed_out").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn format_ty_json_fails_when_ty_is_not_on_path() {
    let project_dir = make_temp_project_dir();
    let empty_path_dir = support::cli::temp_dir("adk-rs-empty-path");
    fs::create_dir_all(&empty_path_dir).expect("create empty PATH dir");

    let output = support::cli::poly_offline_command()
        .env("PATH", empty_path_dir)
        .args([
            "format",
            "--json",
            "--check",
            "--ty",
            "--path",
            &project_dir,
        ])
        .output()
        .expect("run poly format --ty");

    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(payload.get("ty_ran").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("ty_returncode").and_then(|v| v.as_i64()),
        Some(1)
    );
    assert_eq!(
        payload.get("ty_timed_out").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn branch_current_reports_null_when_configured_branch_is_missing_remotely() {
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
    assert!(payload.get("current_branch").is_some_and(|v| v.is_null()));
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
    assert!(payload.get("current_branch").is_some_and(|v| v.is_null()));
}

#[test]
fn pull_json_reconciles_deleted_current_branch_to_main() {
    let project_dir = make_temp_project_dir();
    let p = std::path::PathBuf::from(&project_dir);
    fs::write(
        p.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: deleted-branch\n",
    )
    .expect("rewrite config");

    let output = run_poly(&["pull", "--json", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("new_branch_name").and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        payload.get("new_branch_id").and_then(|v| v.as_str()),
        Some("main")
    );
    let project_yaml = fs::read_to_string(p.join("project.yaml")).expect("project config");
    assert!(!project_yaml.contains("branch_id:"));
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
