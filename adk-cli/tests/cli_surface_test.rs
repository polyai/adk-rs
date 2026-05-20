mod support;

use std::fs;
use support::cli::{
    make_temp_invalid_yaml_project_dir as support_temp_invalid_yaml_project_dir,
    make_temp_project_dir as support_temp_project_dir,
    make_temp_unformatted_json_project_dir as support_temp_unformatted_json_project_dir,
    poly_offline_command, run_poly_offline, temp_dir,
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

fn synthetic_resource_projection_json() -> String {
    serde_json::json!({
        "entities": {
            "entities": {
                "entities": {
                    "ENTITY-age": {
                        "name": "Age",
                        "description": "Customer age.",
                        "type": "numeric",
                        "config": {
                            "value": {
                                "has_range": true,
                                "min": 1,
                                "max": 120
                            }
                        }
                    }
                }
            }
        },
        "experimentalConfig": {
            "experimentalConfigs": {
                "entities": {
                    "default": {
                        "features": {
                            "recording_flag": true,
                            "nested": { "enabled": true }
                        }
                    }
                }
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "FLOW-recording": {
                        "id": "FLOW-recording",
                        "name": "adk_recording_flow",
                        "description": "Flow recording baseline.",
                        "startStepId": "STEP-start",
                        "steps": {
                            "entities": {
                                "STEP-start": {
                                    "name": "start_step",
                                    "type": "advanced_step",
                                    "prompt": "Welcome to the flow recording.",
                                    "asrBiasing": { "isEnabled": false },
                                    "dtmfConfig": { "isEnabled": false }
                                },
                                "STEP-default": {
                                    "name": "default_step",
                                    "type": "default_step",
                                    "prompt": "What do you need?",
                                    "conditions": [{
                                        "id": "COND-exit",
                                        "config": {
                                            "$case": "exitFlowCondition",
                                            "value": {
                                                "details": {
                                                    "label": "exit",
                                                    "description": "Exit the flow.",
                                                    "ingressPosition": "top",
                                                    "requiredEntities": []
                                                }
                                            }
                                        }
                                    }]
                                }
                            }
                        }
                    }
                }
            }
        },
        "handoff": {
            "handoffs": {
                "entities": {
                    "HANDOFF-sales": {
                        "name": "Sales",
                        "description": "Route to sales.",
                        "isDefault": true,
                        "active": true,
                        "sipConfig": {
                            "config": {
                                "$case": "invite",
                                "value": {
                                    "phoneNumber": "+15551234567",
                                    "outboundEndpoint": "sales-trunk",
                                    "outboundEncryption": "TLS/SRTP"
                                }
                            }
                        },
                        "sipHeaders": {
                            "headers": [{ "key": "X-Recording", "value": "sales" }]
                        }
                    }
                }
            }
        },
        "sms": {
            "templates": {
                "entities": {
                    "SMS-welcome": {
                        "name": "Welcome SMS",
                        "text": "Hello {recording_state}",
                        "active": true,
                        "envPhoneNumbers": {
                            "sandbox": "+15550000001",
                            "preRelease": "+15550000002",
                            "live": "+15550000003"
                        }
                    }
                }
            }
        },
        "stopKeywords": {
            "filters": {
                "entities": {
                    "STOP-hangup": {
                        "title": "Hang Up",
                        "description": "End the conversation.",
                        "regularExpressions": ["(?i)bye"],
                        "sayPhrase": false,
                        "languageCode": "en-US"
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
fn help_subcommand_prints_top_level_help() {
    let output = run_poly(&["help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: poly [-h] [-v]"));
    assert!(stdout.contains("help"));
    assert!(stdout.contains("Show this help message and exit."));
    assert!(stdout.contains("Outputs documentation for a given topic."));
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
        ("help", "Show this help message and exit."),
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
        ("review", "Incomplete: review Agent Studio project changes."),
        ("branch", "Manage branches in the Agent Studio project."),
        (
            "format",
            "Run ruff and YAML/JSON formatting on the project (optional ty with --ty).",
        ),
        ("validate", "Validate the project configuration locally."),
        ("chat", "Start an interactive chat session with the agent."),
        (
            "self-update",
            "Update the ADK CLI installed by the release shell installer.",
        ),
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
fn self_update_without_release_installer_receipt_exits_before_network() {
    let config_dir = temp_dir("adk-rs-update-no-receipt");
    fs::create_dir_all(&config_dir).expect("mkdir update test config dir");

    let output = poly_offline_command()
        .arg("self-update")
        .env("AXOUPDATER_CONFIG_PATH", &config_dir)
        .output()
        .expect("failed to execute poly self-update");
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "Self-update is only supported for ADK installs that were installed via shell; no shell install receipt was found."
        ),
        "expected concise shell-install guidance\nstderr={stderr}"
    );
    assert!(
        !stderr.contains("poly-adk"),
        "expected no receipt details by default\nstderr={stderr}"
    );
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
fn status_json_does_not_require_remote_configuration() {
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
fn pull_from_projection_materializes_synthetic_resources_and_clean_status() {
    let project_dir = make_temp_project_dir();
    let project_root = std::path::PathBuf::from(&project_dir);
    let projection_arg = synthetic_resource_projection_json();

    let output = run_poly(&[
        "pull",
        "--json",
        "--path",
        &project_dir,
        "--from-projection",
        &projection_arg,
    ]);

    assert_eq!(output.status.code(), Some(0));
    let flow_config =
        fs::read_to_string(project_root.join("flows/adk_recording_flow/flow_config.yaml"))
            .expect("flow config");
    assert!(flow_config.contains("name: adk_recording_flow"));
    assert!(flow_config.contains("Flow recording baseline."));

    let flow_step =
        fs::read_to_string(project_root.join("flows/adk_recording_flow/steps/start_step.yaml"))
            .expect("flow step");
    assert!(flow_step.contains("step_type: advanced_step"));
    assert!(flow_step.contains("Welcome to the flow recording."));

    let entities = fs::read_to_string(project_root.join("config/entities.yaml")).expect("entities");
    assert!(entities.contains("Age"));
    assert!(entities.contains("has_range"));

    let handoffs = fs::read_to_string(project_root.join("config/handoffs.yaml")).expect("handoffs");
    assert!(handoffs.contains("Sales"));
    assert!(handoffs.contains("TLS/SRTP"));

    let sms =
        fs::read_to_string(project_root.join("config/sms_templates.yaml")).expect("sms templates");
    assert!(sms.contains("Welcome SMS"));
    assert!(sms.contains("Hello {recording_state}"));

    let phrase_filtering =
        fs::read_to_string(project_root.join("voice/response_control/phrase_filtering.yaml"))
            .expect("phrase filtering");
    assert!(phrase_filtering.contains("Hang Up"));
    assert!(phrase_filtering.contains("(?i)bye"));

    let status = run_poly(&["status", "--json", "--path", &project_dir]);
    assert_eq!(status.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("valid status JSON output");
    for field in [
        "new_files",
        "modified_files",
        "deleted_files",
        "files_with_conflicts",
    ] {
        let values = payload
            .get(field)
            .and_then(|value| value.as_array())
            .unwrap_or_else(|| panic!("{field} array"));
        assert!(values.is_empty(), "{field} should be empty: {values:?}");
    }
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
fn review_subcommands_report_incomplete() {
    let project_dir = make_temp_project_dir();
    for args in [
        vec!["review", "--path", &project_dir, "create", "--json"],
        vec!["review", "list", "--json"],
        vec!["review", "delete", "--json"],
    ] {
        let output = run_poly(&args);
        assert_eq!(output.status.code(), Some(1));
        let payload: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid JSON output");
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            payload.get("message").and_then(|v| v.as_str()),
            Some("The review subcommand is not implemented in adk-rs yet.")
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
fn validate_json_reports_special_and_flow_function_syntax_errors() {
    for (path, name, content) in [
        (
            "functions/start_function.py",
            "start_function",
            "from _gen import *  # <AUTO GENERATED>\n\n\ndef start_function(conv: Conversation):\n    return (\n",
        ),
        (
            "functions/end_function.py",
            "end_function",
            "from _gen import *  # <AUTO GENERATED>\n\n\ndef end_function(conv: Conversation):\n    if True\n        return None\n",
        ),
        (
            "flows/bad_flow/function_steps/bad_func.py",
            "bad_func",
            "from _gen import *  # <AUTO GENERATED>\n\n\ndef bad_func(conv: Conversation, flow: Flow):\n    if True\n        return None\n",
        ),
    ] {
        let project_dir = make_temp_project_dir();
        let root = std::path::PathBuf::from(&project_dir);
        let file_path = root.join(path);
        fs::create_dir_all(file_path.parent().expect("parent")).expect("mkdir resource dir");
        fs::write(&file_path, content).expect("write invalid function");

        let output = run_poly(&["validate", "--json", "--path", &project_dir]);

        assert_eq!(output.status.code(), Some(1), "{path}");
        let payload: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid JSON output");
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false),
            "{path}"
        );
        let error = payload
            .get("error")
            .and_then(|v| v.as_str())
            .expect("error string");
        assert!(
            error.contains(&format!("Error reading resource {name}")),
            "{path}: {error}"
        );
        assert!(error.contains(path), "{path}: {error}");
    }
}

#[test]
fn push_json_dry_run_reports_python_syntax_read_errors_without_http_recording() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(
        root.join("functions/bad_global.py"),
        "from _gen import *  # <AUTO GENERATED>\n\n\ndef bad_global(conv: Conversation):\n    if True\n        return None\n",
    )
    .expect("write invalid global function");

    let output = run_poly(&[
        "push",
        "--json",
        "--dry-run",
        "--from-projection",
        "{}",
        "--path",
        &project_dir,
    ]);

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

fn write_invalid_flow_validation_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    fs::create_dir_all(root.join("flows/bad_flow/steps")).expect("mkdir flow steps");
    fs::create_dir_all(root.join("flows/bad_flow/function_steps")).expect("mkdir function steps");
    fs::write(
        root.join("config/entities.yaml"),
        "entities:\n  - name: Age\n    description: Age.\n    entity_type: numeric\n    config:\n      has_range: false\n",
    )
    .expect("write entities");
    fs::write(
        root.join("flows/bad_flow/flow_config.yaml"),
        "name: bad_flow\ndescription: Valid description.\nstart_step: missing_start\n",
    )
    .expect("write flow config");
    fs::write(
        root.join("flows/bad_flow/steps/bad_condition.yaml"),
        "step_type: default_step\nname: bad_condition\nprompt: Continue.\nconditions:\n  - name: go\n    condition_type: step_condition\n    description: Go.\n    child_step: missing_step\n    required_entities:\n      - ENTITY-missing\n",
    )
    .expect("write bad condition step");
    fs::write(
        root.join("flows/bad_flow/steps/default_func.yaml"),
        "step_type: default_step\nname: default_func\nprompt: \"Use {{fn:recording_handler}}.\"\nconditions: []\n",
    )
    .expect("write default function step");
    fs::write(
        root.join("flows/bad_flow/steps/empty_prompt.yaml"),
        "step_type: advanced_step\nname: empty_prompt\nprompt: \"\"\n",
    )
    .expect("write empty prompt step");
    fs::write(
        root.join("flows/bad_flow/function_steps/bad_func.py"),
        "from _gen import *  # <AUTO GENERATED>\n\n@func_parameter(\"x\", \"bad\")\ndef bad_func(conv: Conversation, flow: Flow, x: str):\n    return x\n",
    )
    .expect("write bad function step");
}

#[test]
fn validate_json_reports_flow_resource_errors_without_http_recording() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    write_invalid_flow_validation_project(root.as_path());

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
    for expected in [
        "Step 'missing_step' not found",
        "Default steps cannot reference functions",
        "Prompt cannot be empty",
        "Function definition 'def bad_func(conv: Conversation, flow: Flow)' not found",
        "Start step 'missing_start' not found",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "missing {expected:?}: {errors:#?}"
        );
    }
}

#[test]
fn push_json_dry_run_reports_flow_validation_errors_without_http_recording() {
    let project_dir = make_temp_project_dir();
    let root = std::path::PathBuf::from(&project_dir);
    write_invalid_flow_validation_project(root.as_path());

    let output = run_poly(&[
        "push",
        "--json",
        "--dry-run",
        "--from-projection",
        "{}",
        "--path",
        &project_dir,
    ]);

    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    let message = payload
        .get("message")
        .and_then(|v| v.as_str())
        .expect("message string");
    assert!(message.contains("Validation errors detected:"));
    assert!(message.contains("Step 'missing_step' not found"));
    assert!(message.contains("Default steps cannot reference functions"));
    assert!(message.contains("Prompt cannot be empty"));
    assert!(message.contains("Start step 'missing_start' not found"));
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
fn review_text_reports_incomplete() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["review", "--path", &project_dir, "list"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("The review subcommand is not implemented in adk-rs yet."));
}

#[test]
fn pull_requires_remote_configuration() {
    let project_dir = make_temp_project_dir();
    let output = run_poly(&["pull", "--json", "--path", &project_dir]);
    assert_eq!(output.status.code(), Some(1));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let error = payload.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(error.contains("remote platform client unavailable"));
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
