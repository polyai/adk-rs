mod support;

use httpmock::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use support::python_recordings::{
    TARGET_ACCOUNT_ID, TARGET_PROJECT_ID, TARGET_PROJECT_NAME, TARGET_REGION,
    fixture_dir as recording_fixture_dir, httpmock_adk_base_url, python_adk_bin, recording_run_id,
    replace_all as normalize_text, temp_recording_dir,
};

const PYTHON_ADK_BIN_DISPLAY: &str = "${PYTHON_ADK_BIN:-poly}";
const AGENT_STUDIO_HOST_URL: &str = "https://api.us.poly.ai";
const BASIC_COMMAND_MANIFEST_FILE: &str = "basic-readonly.commands.yaml";
const BASIC_HTTPMOCK_RECORDING_FILE: &str = "basic-readonly.httpmock.yaml";
const BRANCH_UPDATE_COMMAND_MANIFEST_FILE: &str = "branch-update-push.commands.yaml";
const BRANCH_UPDATE_HTTPMOCK_RECORDING_FILE: &str = "branch-update-push.httpmock.yaml";
const CREATE_DELETE_COMMAND_MANIFEST_FILE: &str = "create-delete-dryrun.commands.yaml";
const CREATE_DELETE_HTTPMOCK_RECORDING_FILE: &str = "create-delete-dryrun.httpmock.yaml";
const DIRTY_SWITCH_COMMAND_MANIFEST_FILE: &str = "dirty-switch.commands.yaml";
const DIRTY_SWITCH_HTTPMOCK_RECORDING_FILE: &str = "dirty-switch.httpmock.yaml";
const VALIDATION_ERRORS_COMMAND_MANIFEST_FILE: &str = "validation-errors.commands.yaml";
const VALIDATION_ERRORS_HTTPMOCK_RECORDING_FILE: &str = "validation-errors.httpmock.yaml";
const REVERT_LOCAL_COMMAND_MANIFEST_FILE: &str = "revert-local.commands.yaml";
const REVERT_LOCAL_HTTPMOCK_RECORDING_FILE: &str = "revert-local.httpmock.yaml";
const PULL_CONFLICT_COMMAND_MANIFEST_FILE: &str = "pull-conflict.commands.yaml";
const PULL_CONFLICT_HTTPMOCK_RECORDING_FILE: &str = "pull-conflict.httpmock.yaml";
const BRANCH_MERGE_COMMAND_MANIFEST_FILE: &str = "branch-merge-main.commands.yaml";
const BRANCH_MERGE_HTTPMOCK_RECORDING_FILE: &str = "branch-merge-main.httpmock.yaml";
const MAIN_PUSH_COMMAND_MANIFEST_FILE: &str = "main-push.commands.yaml";
const MAIN_PUSH_HTTPMOCK_RECORDING_FILE: &str = "main-push.httpmock.yaml";
const MERGE_CONFLICT_COMMAND_MANIFEST_FILE: &str = "merge-conflict-resolution.commands.yaml";
const MERGE_CONFLICT_HTTPMOCK_RECORDING_FILE: &str = "merge-conflict-resolution.httpmock.yaml";
const PULL_RESOURCE_COVERAGE_COMMAND_MANIFEST_FILE: &str = "pull-resource-coverage.commands.yaml";
const PULL_RESOURCE_COVERAGE_HTTPMOCK_RECORDING_FILE: &str = "pull-resource-coverage.httpmock.yaml";
const PUSH_RESOURCE_COVERAGE_COMMAND_MANIFEST_FILE: &str = "push-resource-coverage.commands.yaml";
const PUSH_RESOURCE_COVERAGE_HTTPMOCK_RECORDING_FILE: &str = "push-resource-coverage.httpmock.yaml";
const SEMANTIC_VALIDATION_COMMAND_MANIFEST_FILE: &str = "semantic-validation.commands.yaml";
const SEMANTIC_VALIDATION_HTTPMOCK_RECORDING_FILE: &str = "semantic-validation.httpmock.yaml";
const FORMAT_LOCAL_COMMAND_MANIFEST_FILE: &str = "format-local.commands.yaml";
const FORMAT_LOCAL_HTTPMOCK_RECORDING_FILE: &str = "format-local.httpmock.yaml";
const INTERACTIVE_CONTRACTS_COMMAND_MANIFEST_FILE: &str = "interactive-contracts.commands.yaml";
const INTERACTIVE_CONTRACTS_HTTPMOCK_RECORDING_FILE: &str = "interactive-contracts.httpmock.yaml";
const CHAT_JSON_COMMAND_MANIFEST_FILE: &str = "chat-json.commands.yaml";
const CHAT_JSON_HTTPMOCK_RECORDING_FILE: &str = "chat-json.httpmock.yaml";
const CHAT_SESSION_CONTROLS_COMMAND_MANIFEST_FILE: &str = "chat-session-controls.commands.yaml";
const CHAT_SESSION_CONTROLS_HTTPMOCK_RECORDING_FILE: &str = "chat-session-controls.httpmock.yaml";
const PULL_FORCE_CLEANUP_COMMAND_MANIFEST_FILE: &str = "pull-force-cleanup.commands.yaml";
const PULL_FORCE_CLEANUP_HTTPMOCK_RECORDING_FILE: &str = "pull-force-cleanup.httpmock.yaml";
const CHAT_ERROR_METADATA_COMMAND_MANIFEST_FILE: &str = "chat-error-metadata.commands.yaml";
const CHAT_ERROR_METADATA_HTTPMOCK_RECORDING_FILE: &str = "chat-error-metadata.httpmock.yaml";
const CLI_DIFF_EDGES_COMMAND_MANIFEST_FILE: &str = "cli-diff-edges.commands.yaml";
const CLI_DIFF_EDGES_HTTPMOCK_RECORDING_FILE: &str = "cli-diff-edges.httpmock.yaml";
const MUTATING_BRANCH_NAME: &str = "adk-rs-recording-mutating";
const CREATE_DELETE_BRANCH_NAME: &str = "adk-rs-recording-create-delete";
const DIRTY_SWITCH_BRANCH_NAME: &str = "adk-rs-recording-dirty-switch";
const PULL_CONFLICT_BRANCH_NAME: &str = "adk-rs-recording-pull-conflict";
const BRANCH_MERGE_BRANCH_PREFIX: &str = "adk-rs-recording-merge";
const MERGE_CONFLICT_BRANCH_PREFIX: &str = "adk-rs-recording-conflict";
const PUSH_RESOURCE_COVERAGE_BRANCH_NAME: &str = "adk-rs-recording-resource-coverage";
const INTERACTIVE_BRANCH_NAME: &str = "adk-rs-recording-interactive";
const MUTATING_EDIT_FILE: &str = "agent_settings/rules.txt";
const MUTATING_EDIT_TEXT: &str = "\n\n# ADK recording branch edit\nThis line was added by the Python ADK httpmock mutating workflow.\n";
const CREATE_TOPIC_FILE: &str = "topics/adk_recording_topic.yaml";
const CREATE_TOPIC_TEXT: &str = "name: ADK Recording Topic\nenabled: true\nactions: Keep answers concise and helpful.\ncontent: This topic exists only to exercise Python ADK create command generation.\nexample_queries:\n- What can this recorder test?\n";
const DELETE_FUNCTION_FILE: &str = "functions/goodbye_and_hang_up.py";
const INVALID_PERSONALITY_FILE: &str = "agent_settings/personality.yaml";
const INVALID_PERSONALITY_TEXT: &str = "adjectives:\n  Polite: true\n  - malformed\n";
const PULL_CONFLICT_REMOTE_TEXT: &str =
    "\n\n# ADK recording remote branch edit\nThis line is pushed from the second checkout.\n";
const PULL_CONFLICT_LOCAL_TEXT: &str =
    "\n\n# ADK recording local branch edit\nThis line stays local before pull.\n";
const PULL_CONFLICT_RULE_TARGET: &str =
    "Your task is to assist users with their queries about [Organization/Service].";
const PULL_CONFLICT_REMOTE_RULE: &str =
    "Your task is to assist remote recording users with their queries.";
const PULL_CONFLICT_LOCAL_RULE: &str =
    "Your task is to assist local recording users with their queries.";
const MAIN_PUSH_EDIT_FILE: &str = "agent_settings/rules.txt";
const RECORDER_EMAIL: &str = "adk-recorder@example.com";
const RESOURCE_COVERAGE_PERSONALITY: &str =
    "adjectives:\n  Polite: true\n  Curious: true\ncustom: Recording parity custom personality.\n";
const RESOURCE_COVERAGE_ROLE: &str = "value: CustomerServiceRepresentative\nadditional_info: Recording parity role detail.\ncustom: \"\"\n";
const RESOURCE_COVERAGE_SAFETY_FILTERS: &str = "enabled: true\ncategories:\n  violence:\n    enabled: true\n    level: medium\n  hate:\n    enabled: false\n    level: medium\n  sexual:\n    enabled: false\n    level: medium\n  self_harm:\n    enabled: false\n    level: medium\n";
const RESOURCE_COVERAGE_ASR_SETTINGS: &str = "barge_in: true\ninteraction_style: balanced\n";
const RESOURCE_COVERAGE_VOICE_CONFIG: &str = "greeting:\n  welcome_message: Hello from the ADK recording coverage workflow.\n  language_code: en-US\nstyle_prompt:\n  prompt: Keep the voice reply compact.\ndisclaimer_messages:\n  message: This call may be recorded.\n  enabled: true\n  language_code: en-US\n";
const RESOURCE_COVERAGE_KEYPHRASES: &str =
    "keyphrases:\n  - keyphrase: ADK parity\n    level: boosted\n";
const RESOURCE_COVERAGE_PRONUNCIATIONS: &str = "pronunciations:\n  - regex: \"\\\\bADK\\\\b\"\n    replacement: Agent Development Kit\n    case_sensitive: true\n    language_code: en-US\n";
const RESOURCE_COVERAGE_TRANSCRIPT_CORRECTIONS: &str = "corrections:\n  - name: ADK spelling\n    description: Correct ADK spelling.\n    regular_expressions:\n      - regular_expression: agent development kid\n        replacement: agent development kit\n        replacement_type: full\n";
const RESOURCE_COVERAGE_VARIANTS: &str = "variants:\n  - name: default\n    is_default: true\n  - name: treatment\n    is_default: false\nattributes:\n  - name: adk-recording-cohort\n    values:\n      default: control\n      treatment: treatment\n";
const RESOURCE_COVERAGE_API_INTEGRATIONS: &str = "api_integrations:\n  - name: adk_recording_api\n    description: Recording-only API integration.\n    environments:\n      sandbox:\n        base_url: https://example.invalid/sandbox\n        auth_type: none\n      pre-release:\n        base_url: https://example.invalid/pre\n        auth_type: none\n      live:\n        base_url: https://example.invalid/live\n        auth_type: none\n    operations:\n      - name: get_recording_status\n        method: GET\n        resource: /status\n";
const SEMANTIC_VALIDATION_VARIANTS: &str = "variants:\n  - name: first-default\n    is_default: true\n  - name: second-default\n    is_default: true\nattributes: []\n";
const SEMANTIC_VALIDATION_API_INTEGRATIONS: &str = "api_integrations:\n  - name: Bad Name\n    description: Invalid because names must follow Python identifier conventions.\n";
const SEMANTIC_VALIDATION_TRANSCRIPT_CORRECTIONS: &str = "corrections:\n  - name: Missing regex rules\n    description: Invalid because regular_expressions is required.\n    regular_expressions: []\n";
const FORMAT_UNFORMATTED_TOPIC: &str = "name: ADK Format Recording\nenabled: true\nactions: hi\ncontent: hello\nexample_queries: [hello, hi]\n";
const FORMAT_UNFORMATTED_FUNCTION: &str =
    "def adk_format_recording( conv ):\n return {'utterance':'hi'}\n";
const CHAT_INPUT_FILE_TEXT: &str = "Hello from input file\n/exit\n";
const PULL_FORCE_LOCAL_TOPIC_FILE: &str = "topics/adk_pull_force_local_only.yaml";
const PULL_FORCE_LOCAL_TOPIC_TEXT: &str = "name: ADK Pull Force Local Only\nenabled: true\nactions: This resource should be deleted by pull --force.\ncontent: Local-only topic used by Python recording tests.\nexample_queries:\n- Should this local topic remain after pull force?\n";
const PULL_FORCE_LOCAL_FUNCTION_FILE: &str = "functions/adk_pull_force_local_only.py";
const PULL_FORCE_LOCAL_FUNCTION_TEXT: &str =
    "def adk_pull_force_local_only(conversation):\n    return {\"utterance\": \"local only\"}\n";

#[derive(Debug, Serialize)]
struct CommandManifest {
    schema_version: u8,
    source: ManifestSource,
    replay_notes: Vec<&'static str>,
    httpmock_recording: String,
    workflows: Vec<Workflow>,
}

#[derive(Debug, Serialize)]
struct ManifestSource {
    implementation: &'static str,
    recorder: &'static str,
    server: &'static str,
    poly_binary: &'static str,
    region: &'static str,
    account_id: &'static str,
    project_id: &'static str,
    project_name: &'static str,
}

#[derive(Debug, Serialize)]
struct Workflow {
    name: &'static str,
    description: &'static str,
    steps: Vec<CommandRecord>,
}

#[derive(Debug, Serialize)]
struct StepManifest {
    schema_version: u8,
    source: ManifestSource,
    replay_notes: Vec<&'static str>,
    httpmock_recording: String,
    workflows: Vec<StepWorkflow>,
}

#[derive(Debug, Serialize)]
struct StepWorkflow {
    name: &'static str,
    description: &'static str,
    mutates_real_server: bool,
    cleanup: Vec<&'static str>,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkflowStep {
    Command(CommandRecord),
    FileEdit(FileEditRecord),
}

#[derive(Debug, Serialize)]
struct CommandRecord {
    name: &'static str,
    argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdin: Option<String>,
    exit_code: i32,
    stdout_json: Option<Value>,
    stdout: Option<String>,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct FileEditRecord {
    name: &'static str,
    operation: &'static str,
    path: String,
    content: Option<String>,
    target: Option<String>,
    replacement: Option<String>,
    success: bool,
    error: Option<String>,
}

#[test]
#[ignore = "records real Agent Studio traffic; basic read-only workflow"]
fn record_basic_readonly_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();

    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let replacements = vec![
        (tmp.to_string_lossy().to_string(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut workflows = Vec::new();
    let mut init_steps = Vec::new();
    init_steps.push(run_python_poly(
        "init real project with projection output",
        &[
            "init",
            "--json",
            "--base-path",
            tmp.to_string_lossy().as_ref(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
            "--output-json-projection",
        ],
        &server,
        &replacements,
    ));
    init_steps.push(run_python_poly(
        "status after real init",
        &[
            "status",
            "--json",
            "--path",
            project_root.to_string_lossy().as_ref(),
        ],
        &server,
        &replacements,
    ));
    init_steps.push(run_python_poly(
        "validate after real init",
        &[
            "validate",
            "--json",
            "--path",
            project_root.to_string_lossy().as_ref(),
        ],
        &server,
        &replacements,
    ));
    workflows.push(Workflow {
        name: "real_init_and_local_checks",
        description: "Initialize the real Agent Studio project through httpmock forwarding, then run local checks.",
        steps: init_steps,
    });

    let mut read_steps = Vec::new();
    for (name, args) in [
        (
            "branch current",
            vec![
                "branch",
                "current",
                "--json",
                "--path",
                project_root.to_string_lossy().as_ref(),
            ],
        ),
        (
            "branch list",
            vec![
                "branch",
                "list",
                "--json",
                "--path",
                project_root.to_string_lossy().as_ref(),
            ],
        ),
        (
            "deployments list sandbox",
            vec![
                "deployments",
                "list",
                "--json",
                "--path",
                project_root.to_string_lossy().as_ref(),
                "--env",
                "sandbox",
                "--limit",
                "3",
                "--details",
            ],
        ),
        (
            "force pull real projection",
            vec![
                "pull",
                "--json",
                "--force",
                "--path",
                project_root.to_string_lossy().as_ref(),
                "--output-json-projection",
            ],
        ),
        (
            "diff before main against local",
            vec![
                "diff",
                "--json",
                "--path",
                project_root.to_string_lossy().as_ref(),
                "--before",
                "main",
            ],
        ),
        (
            "status after real pull",
            vec![
                "status",
                "--json",
                "--path",
                project_root.to_string_lossy().as_ref(),
            ],
        ),
    ] {
        read_steps.push(run_python_poly(name, &args, &server, &replacements));
    }
    workflows.push(Workflow {
        name: "real_readonly_remote_queries",
        description: "Read-only branch, deployment, pull, and diff commands recorded through httpmock forwarding.",
        steps: read_steps,
    });

    let recording_path = recording
        .save("real-agent-studio-python-adk")
        .expect("save httpmock recording");
    let fixture_dir = recording_fixture_dir();
    fs::create_dir_all(&fixture_dir).expect("create recording fixture dir");
    let httpmock_fixture = fixture_dir.join(BASIC_HTTPMOCK_RECORDING_FILE);
    let mut recording_yaml = fs::read_to_string(&recording_path).expect("read httpmock recording");
    recording_yaml = recording_yaml.replace(&api_key, "<redacted>");
    fs::write(&httpmock_fixture, recording_yaml).expect("write redacted httpmock fixture");

    let manifest = CommandManifest {
        schema_version: 2,
        source: ManifestSource {
            implementation: "python-adk",
            recorder: "rust-httpmock-forwarding",
            server: "real-agent-studio",
            poly_binary: PYTHON_ADK_BIN_DISPLAY,
            region: TARGET_REGION,
            account_id: TARGET_ACCOUNT_ID,
            project_id: TARGET_PROJECT_ID,
            project_name: TARGET_PROJECT_NAME,
        },
        replay_notes: vec![
            "This command manifest was produced by the ignored record_python_adk_httpmock_fixtures_test.",
            "The HTTP traffic lives in basic-readonly.httpmock.yaml, saved by httpmock record/playback.",
            "Authentication headers are redacted after recording.",
            "Substitute ${TMP} with a temporary working directory when replaying command expectations.",
        ],
        httpmock_recording: BASIC_HTTPMOCK_RECORDING_FILE.to_string(),
        workflows,
    };
    let manifest_yaml = serde_yaml::to_string(&manifest).expect("serialize manifest");
    fs::write(fixture_dir.join(BASIC_COMMAND_MANIFEST_FILE), manifest_yaml)
        .expect("write command manifest");

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "records mutating real Agent Studio traffic; branch update/push workflow"]
fn record_branch_update_push_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();

    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            MUTATING_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];

    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before mutating branch workflow",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before mutating branch workflow",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_branch = run_python_poly(
        "create throwaway branch",
        &[
            "branch",
            "create",
            MUTATING_BRANCH_NAME,
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create throwaway branch", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        let current_branch = run_python_poly(
            "current branch after create",
            &[
                "branch",
                "current",
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "current branch after create",
            command_succeeded(&current_branch),
        ));
        steps.push(WorkflowStep::Command(current_branch));

        let edit = append_text_file(
            "append branch edit to rules.txt",
            &project_root,
            MUTATING_EDIT_FILE,
            MUTATING_EDIT_TEXT,
            &replacements,
        );
        let edit_success = edit.success;
        required_results.push(("append branch edit to rules.txt", edit_success));
        steps.push(WorkflowStep::FileEdit(edit));

        if edit_success {
            for (required, name, args) in [
                (
                    true,
                    "status after branch file edit",
                    vec!["status", "--json", "--path", project_path.as_str()],
                ),
                (
                    true,
                    "diff all local changes after branch file edit",
                    vec!["diff", "--json", "--path", project_path.as_str()],
                ),
                (
                    false,
                    "diff filtered rules file before push",
                    vec![
                        "diff",
                        "--json",
                        "--path",
                        project_path.as_str(),
                        "--files",
                        MUTATING_EDIT_FILE,
                    ],
                ),
                (
                    true,
                    "push dry-run with command payload",
                    vec![
                        "push",
                        "--output-json-commands",
                        "--dry-run",
                        "--skip-validation",
                        "--email",
                        RECORDER_EMAIL,
                        "--path",
                        project_path.as_str(),
                    ],
                ),
                (
                    true,
                    "push branch edit",
                    vec![
                        "push",
                        "--json",
                        "--force",
                        "--skip-validation",
                        "--email",
                        RECORDER_EMAIL,
                        "--path",
                        project_path.as_str(),
                    ],
                ),
                (
                    true,
                    "status after branch push",
                    vec!["status", "--json", "--path", project_path.as_str()],
                ),
                (
                    true,
                    "diff pushed branch against main",
                    vec![
                        "diff",
                        "--json",
                        "--path",
                        project_path.as_str(),
                        "--before",
                        "main",
                    ],
                ),
            ] {
                let record = run_python_poly(name, &args, &server, &replacements);
                if required {
                    required_results.push((name, command_succeeded(&record)));
                }
                steps.push(WorkflowStep::Command(record));
            }
        }

        let delete_branch = run_python_poly(
            "delete throwaway branch",
            &[
                "branch",
                "delete",
                MUTATING_BRANCH_NAME,
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push(("delete throwaway branch", command_succeeded(&delete_branch)));
        steps.push(WorkflowStep::Command(delete_branch));

        let current_after_delete = run_python_poly(
            "current branch after deleting throwaway branch",
            &[
                "branch",
                "current",
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "current branch after deleting throwaway branch",
            command_succeeded(&current_after_delete),
        ));
        steps.push(WorkflowStep::Command(current_after_delete));
    }

    let recording_path = recording
        .save("branch-update-push-python-adk")
        .expect("save mutating httpmock recording");
    let fixture_dir = recording_fixture_dir();
    fs::create_dir_all(&fixture_dir).expect("create recording fixture dir");
    let httpmock_fixture = fixture_dir.join(BRANCH_UPDATE_HTTPMOCK_RECORDING_FILE);
    let mut recording_yaml = fs::read_to_string(&recording_path).expect("read httpmock recording");
    recording_yaml = recording_yaml.replace(&api_key, "<redacted>");
    fs::write(&httpmock_fixture, recording_yaml).expect("write redacted httpmock fixture");

    let manifest = StepManifest {
        schema_version: 3,
        source: ManifestSource {
            implementation: "python-adk",
            recorder: "rust-httpmock-forwarding",
            server: "real-agent-studio",
            poly_binary: PYTHON_ADK_BIN_DISPLAY,
            region: TARGET_REGION,
            account_id: TARGET_ACCOUNT_ID,
            project_id: TARGET_PROJECT_ID,
            project_name: TARGET_PROJECT_NAME,
        },
        replay_notes: vec![
            "This manifest records a mutating Python ADK workflow against a throwaway branch.",
            "The HTTP traffic lives in branch-update-push.httpmock.yaml, saved by httpmock record/playback.",
            "Authentication headers are redacted after recording.",
            "Apply file_edit steps before replaying the following command steps.",
            "The branch is deleted at the end of the recorded workflow.",
        ],
        httpmock_recording: BRANCH_UPDATE_HTTPMOCK_RECORDING_FILE.to_string(),
        workflows: vec![StepWorkflow {
            name: "real_branch_edit_push_and_cleanup",
            description: "Create a real Agent Studio branch, edit a local rules file, inspect changes, dry-run push commands, push to the branch, diff against main, then delete the branch.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        }],
    };
    let manifest_yaml = serde_yaml::to_string(&manifest).expect("serialize mutating manifest");
    fs::write(
        fixture_dir.join(BRANCH_UPDATE_COMMAND_MANIFEST_FILE),
        manifest_yaml,
    )
    .expect("write mutating command manifest");

    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "mutating Python ADK recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; creates and deletes a branch after dry-run resource changes"]
fn record_create_delete_dryrun_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            CREATE_DELETE_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before create/delete dry-run",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before create/delete dry-run",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_branch = run_python_poly(
        "create throwaway branch",
        &[
            "branch",
            "create",
            CREATE_DELETE_BRANCH_NAME,
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create throwaway branch", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        let write_topic = write_text_file(
            "write new topic resource",
            &project_root,
            CREATE_TOPIC_FILE,
            CREATE_TOPIC_TEXT,
            &replacements,
        );
        required_results.push(("write new topic resource", write_topic.success));
        steps.push(WorkflowStep::FileEdit(write_topic));

        let delete_function = delete_file(
            "delete existing goodbye function",
            &project_root,
            DELETE_FUNCTION_FILE,
            &replacements,
        );
        required_results.push(("delete existing goodbye function", delete_function.success));
        steps.push(WorkflowStep::FileEdit(delete_function));

        for (name, args) in [
            (
                "status after create/delete edits",
                vec!["status", "--json", "--path", project_path.as_str()],
            ),
            (
                "diff after create/delete edits",
                vec!["diff", "--json", "--path", project_path.as_str()],
            ),
            (
                "push dry-run create/delete command payload",
                vec![
                    "push",
                    "--output-json-commands",
                    "--dry-run",
                    "--skip-validation",
                    "--email",
                    RECORDER_EMAIL,
                    "--path",
                    project_path.as_str(),
                ],
            ),
        ] {
            let record = run_python_poly(name, &args, &server, &replacements);
            required_results.push((name, command_succeeded(&record)));
            steps.push(WorkflowStep::Command(record));
        }

        let delete_branch = run_python_poly(
            "delete throwaway branch",
            &[
                "branch",
                "delete",
                CREATE_DELETE_BRANCH_NAME,
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push(("delete throwaway branch", command_succeeded(&delete_branch)));
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("create-delete-dryrun-python-adk")
        .expect("save create/delete dry-run recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        CREATE_DELETE_HTTPMOCK_RECORDING_FILE,
        CREATE_DELETE_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records create/delete command generation on a throwaway branch.",
            "Apply file_edit steps before replaying the following command steps.",
            "The push step is a dry run and does not send command batches to Agent Studio.",
            "The branch is deleted at the end of the recorded workflow.",
        ],
        StepWorkflow {
            name: "create_delete_dryrun",
            description: "Create a branch, write a new topic, delete an existing function locally, inspect status/diff, dry-run push command generation, then delete the branch.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "create/delete dry-run recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; creates/deletes branch to exercise dirty switch behavior"]
fn record_dirty_switch_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            DIRTY_SWITCH_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before dirty switch",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before dirty switch",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_branch = run_python_poly(
        "create throwaway branch",
        &[
            "branch",
            "create",
            DIRTY_SWITCH_BRANCH_NAME,
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create throwaway branch", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        let edit = append_text_file(
            "append dirty switch edit",
            &project_root,
            MUTATING_EDIT_FILE,
            "\n\n# ADK recording dirty switch edit\nThis line makes branch switching require --force.\n",
            &replacements,
        );
        required_results.push(("append dirty switch edit", edit.success));
        steps.push(WorkflowStep::FileEdit(edit));

        let rejected_switch = run_python_poly(
            "switch to main without force fails with dirty tree",
            &[
                "branch",
                "switch",
                "main",
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "switch to main without force fails with dirty tree",
            command_reported_failure(&rejected_switch),
        ));
        steps.push(WorkflowStep::Command(rejected_switch));

        let force_switch = run_python_poly(
            "force switch to main discards dirty tree",
            &[
                "branch",
                "switch",
                "main",
                "--json",
                "--force",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "force switch to main discards dirty tree",
            command_succeeded(&force_switch),
        ));
        steps.push(WorkflowStep::Command(force_switch));

        let status = run_python_poly(
            "status after force switch",
            &["status", "--json", "--path", project_path.as_str()],
            &server,
            &replacements,
        );
        required_results.push(("status after force switch", command_succeeded(&status)));
        steps.push(WorkflowStep::Command(status));

        let delete_branch = run_python_poly(
            "delete throwaway branch after switch",
            &[
                "branch",
                "delete",
                DIRTY_SWITCH_BRANCH_NAME,
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "delete throwaway branch after switch",
            command_succeeded(&delete_branch),
        ));
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("dirty-switch-python-adk")
        .expect("save dirty switch recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        DIRTY_SWITCH_HTTPMOCK_RECORDING_FILE,
        DIRTY_SWITCH_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records Python ADK branch switch behavior with local dirty files.",
            "The first branch switch is expected to fail because --force is omitted.",
            "The second branch switch uses --force and discards the local edit.",
            "The branch is deleted at the end of the recorded workflow.",
        ],
        StepWorkflow {
            name: "dirty_switch",
            description: "Create a branch, dirty the local checkout, capture switch-without-force failure, force switch to main, then delete the branch.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "dirty switch recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; local validation and push error outputs"]
fn record_validation_errors_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before validation errors",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before validation errors",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let invalid_yaml = write_text_file(
        "write invalid personality yaml",
        &project_root,
        INVALID_PERSONALITY_FILE,
        INVALID_PERSONALITY_TEXT,
        &replacements,
    );
    required_results.push(("write invalid personality yaml", invalid_yaml.success));
    steps.push(WorkflowStep::FileEdit(invalid_yaml));

    for (name, args) in [
        (
            "validate invalid yaml",
            vec!["validate", "--json", "--path", project_path.as_str()],
        ),
        (
            "push dry-run invalid yaml",
            vec![
                "push",
                "--json",
                "--dry-run",
                "--path",
                project_path.as_str(),
            ],
        ),
    ] {
        let record = run_python_poly(name, &args, &server, &replacements);
        required_results.push((name, command_reported_failure(&record)));
        steps.push(WorkflowStep::Command(record));
    }

    let recording_path = recording
        .save("validation-errors-python-adk")
        .expect("save validation errors recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        VALIDATION_ERRORS_HTTPMOCK_RECORDING_FILE,
        VALIDATION_ERRORS_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records Python ADK error output for invalid local project files.",
            "The validate and push steps are expected to report failure.",
            "This workflow is local-only after init and does not mutate Agent Studio.",
        ],
        StepWorkflow {
            name: "validation_errors",
            description: "Initialize a real project, overwrite a YAML file with invalid syntax, then record validate and push dry-run error contracts.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "validation errors recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; local revert behavior after edit"]
fn record_revert_local_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before revert",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push(("init real project before revert", command_succeeded(&init)));
    steps.push(WorkflowStep::Command(init));

    let edit = append_text_file(
        "append local revert edit",
        &project_root,
        MUTATING_EDIT_FILE,
        "\n\n# ADK recording revert edit\nThis line should disappear after poly revert.\n",
        &replacements,
    );
    required_results.push(("append local revert edit", edit.success));
    steps.push(WorkflowStep::FileEdit(edit));

    for (name, args) in [
        (
            "status before revert",
            vec!["status", "--json", "--path", project_path.as_str()],
        ),
        (
            "revert edited rules file",
            vec![
                "revert",
                "--json",
                "--path",
                project_path.as_str(),
                MUTATING_EDIT_FILE,
            ],
        ),
        (
            "status after revert",
            vec!["status", "--json", "--path", project_path.as_str()],
        ),
    ] {
        let record = run_python_poly(name, &args, &server, &replacements);
        required_results.push((name, command_succeeded(&record)));
        steps.push(WorkflowStep::Command(record));
    }

    let recording_path = recording
        .save("revert-local-python-adk")
        .expect("save revert local recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        REVERT_LOCAL_HTTPMOCK_RECORDING_FILE,
        REVERT_LOCAL_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records local Python ADK revert behavior after a file edit.",
            "Apply file_edit steps before replaying the following command steps.",
            "This workflow is local-only after init and does not mutate Agent Studio.",
        ],
        StepWorkflow {
            name: "revert_local",
            description: "Initialize a real project, append to rules.txt, verify status sees it, revert that file, then verify the tree is clean.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "revert local recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; creates/deletes branch to exercise pull conflicts"]
fn record_pull_conflict_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    let local_base = tmp.join("local");
    let remote_base = tmp.join("remote");
    fs::create_dir_all(&local_base).expect("create local recording dir");
    fs::create_dir_all(&remote_base).expect("create remote recording dir");
    let local_project = local_base.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let remote_project = remote_base.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let local_project_path = local_project.to_string_lossy().to_string();
    let remote_project_path = remote_project.to_string_lossy().to_string();
    let local_base_path = local_base.to_string_lossy().to_string();
    let remote_base_path = remote_base.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            PULL_CONFLICT_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    for (name, base_path) in [
        ("init local checkout", local_base_path.as_str()),
        ("init remote checkout", remote_base_path.as_str()),
    ] {
        let init = run_python_poly(
            name,
            &[
                "init",
                "--json",
                "--base-path",
                base_path,
                "--region",
                TARGET_REGION,
                "--account_id",
                TARGET_ACCOUNT_ID,
                "--project_id",
                TARGET_PROJECT_ID,
            ],
            &server,
            &replacements,
        );
        required_results.push((name, command_succeeded(&init)));
        steps.push(WorkflowStep::Command(init));
    }

    let create_branch = run_python_poly(
        "create conflict branch from local checkout",
        &[
            "branch",
            "create",
            PULL_CONFLICT_BRANCH_NAME,
            "--json",
            "--path",
            local_project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create conflict branch from local checkout", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        let switch_remote = run_python_poly(
            "switch remote checkout to conflict branch",
            &[
                "branch",
                "switch",
                PULL_CONFLICT_BRANCH_NAME,
                "--json",
                "--force",
                "--path",
                remote_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "switch remote checkout to conflict branch",
            command_succeeded(&switch_remote),
        ));
        steps.push(WorkflowStep::Command(switch_remote));

        let remote_edit = replace_text_file(
            "replace rules text in remote checkout",
            &remote_project,
            MUTATING_EDIT_FILE,
            PULL_CONFLICT_RULE_TARGET,
            PULL_CONFLICT_REMOTE_RULE,
            &replacements,
        );
        required_results.push(("replace rules text in remote checkout", remote_edit.success));
        steps.push(WorkflowStep::FileEdit(remote_edit));

        let remote_append = append_text_file(
            "append remote-only rules line",
            &remote_project,
            MUTATING_EDIT_FILE,
            PULL_CONFLICT_REMOTE_TEXT,
            &replacements,
        );
        required_results.push(("append remote-only rules line", remote_append.success));
        steps.push(WorkflowStep::FileEdit(remote_append));

        let push_remote = run_python_poly(
            "push remote checkout change",
            &[
                "push",
                "--json",
                "--force",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                remote_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "push remote checkout change",
            command_succeeded(&push_remote),
        ));
        steps.push(WorkflowStep::Command(push_remote));

        let local_edit = replace_text_file(
            "replace same rules text in local checkout",
            &local_project,
            MUTATING_EDIT_FILE,
            PULL_CONFLICT_RULE_TARGET,
            PULL_CONFLICT_LOCAL_RULE,
            &replacements,
        );
        required_results.push((
            "replace same rules text in local checkout",
            local_edit.success,
        ));
        steps.push(WorkflowStep::FileEdit(local_edit));

        let local_append = append_text_file(
            "append local-only rules line",
            &local_project,
            MUTATING_EDIT_FILE,
            PULL_CONFLICT_LOCAL_TEXT,
            &replacements,
        );
        required_results.push(("append local-only rules line", local_append.success));
        steps.push(WorkflowStep::FileEdit(local_append));

        let pull_conflict = run_python_poly(
            "pull without force reports conflict",
            &["pull", "--json", "--path", local_project_path.as_str()],
            &server,
            &replacements,
        );
        required_results.push((
            "pull without force reports conflict",
            command_reported_failure(&pull_conflict),
        ));
        steps.push(WorkflowStep::Command(pull_conflict));

        let force_pull = run_python_poly(
            "force pull resolves by overwriting local checkout",
            &[
                "pull",
                "--json",
                "--force",
                "--path",
                local_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "force pull resolves by overwriting local checkout",
            command_succeeded(&force_pull),
        ));
        steps.push(WorkflowStep::Command(force_pull));

        let delete_branch = run_python_poly(
            "delete conflict branch",
            &[
                "branch",
                "delete",
                PULL_CONFLICT_BRANCH_NAME,
                "--json",
                "--path",
                local_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push(("delete conflict branch", command_succeeded(&delete_branch)));
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("pull-conflict-python-adk")
        .expect("save pull conflict recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        PULL_CONFLICT_HTTPMOCK_RECORDING_FILE,
        PULL_CONFLICT_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records Python ADK pull conflict behavior with two checkouts.",
            "The remote checkout pushes one edit to a throwaway branch while the local checkout makes a conflicting edit.",
            "The pull without --force is expected to report failure/conflicts.",
            "The force pull overwrites the local checkout, and the branch is deleted at the end.",
        ],
        StepWorkflow {
            name: "pull_conflict",
            description: "Create two checkouts of a throwaway branch, push a remote edit, make a conflicting local edit, record pull conflict output, force pull, then delete the branch.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/local/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "pull conflict recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records permanent real Agent Studio traffic; merges a branch into main"]
fn record_branch_merge_main_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let run_id = recording_run_id();
    let branch_name = format!("{BRANCH_MERGE_BRANCH_PREFIX}-{run_id}");
    let merge_text = format!(
        "\n\n# ADK recording branch merge {run_id}\nThis line was merged into main by the Python ADK recorder.\n"
    );
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (branch_name.clone(), "${BRANCH_NAME}".to_string()),
        (merge_text.clone(), "${MERGE_TEXT}".to_string()),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before branch merge",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before branch merge",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_branch = run_python_poly(
        "create branch to merge",
        &[
            "branch",
            "create",
            branch_name.as_str(),
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create branch to merge", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        let edit = append_text_file(
            "append branch merge edit",
            &project_root,
            MUTATING_EDIT_FILE,
            merge_text.as_str(),
            &replacements,
        );
        required_results.push(("append branch merge edit", edit.success));
        steps.push(WorkflowStep::FileEdit(edit));

        for (name, args) in [
            (
                "push branch before merge",
                vec![
                    "push",
                    "--json",
                    "--force",
                    "--skip-validation",
                    "--email",
                    RECORDER_EMAIL,
                    "--path",
                    project_path.as_str(),
                ],
            ),
            (
                "merge branch into main",
                vec![
                    "branch",
                    "merge",
                    "ADK recording branch merge",
                    "--json",
                    "--path",
                    project_path.as_str(),
                ],
            ),
            (
                "current branch after merge",
                vec![
                    "branch",
                    "current",
                    "--json",
                    "--path",
                    project_path.as_str(),
                ],
            ),
            (
                "status after merge switch to main",
                vec!["status", "--json", "--path", project_path.as_str()],
            ),
        ] {
            let record = run_python_poly(name, &args, &server, &replacements);
            required_results.push((name, command_succeeded(&record)));
            steps.push(WorkflowStep::Command(record));
        }

        let delete_branch = run_python_poly(
            "delete merged branch if still present",
            &[
                "branch",
                "delete",
                branch_name.as_str(),
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("branch-merge-main-python-adk")
        .expect("save branch merge recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        BRANCH_MERGE_HTTPMOCK_RECORDING_FILE,
        BRANCH_MERGE_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records a permanent branch merge into main.",
            "The throwaway branch is pushed, merged into main, and then deleted if still present.",
            "The main branch retains the merged rules.txt change.",
        ],
        StepWorkflow {
            name: "branch_merge_main",
            description: "Create a branch, edit rules.txt, push the branch, merge it into main, verify main is current, then attempt branch cleanup.",
            mutates_real_server: true,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "branch merge recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; pushes from a main checkout"]
fn record_main_push_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let run_id = recording_run_id();
    let push_text = format!(
        "\n\n# ADK recording direct main push {run_id}\nThis line was pushed directly to main by the Python ADK recorder.\n"
    );
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (push_text.clone(), "${MAIN_PUSH_TEXT}".to_string()),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init = run_python_poly(
        "init real project before direct main push",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before direct main push",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let edit = append_text_file(
        "append push-from-main edit",
        &project_root,
        MAIN_PUSH_EDIT_FILE,
        push_text.as_str(),
        &replacements,
    );
    required_results.push(("append push-from-main edit", edit.success));
    steps.push(WorkflowStep::FileEdit(edit));

    for (name, args) in [
        (
            "status before push from main",
            vec!["status", "--json", "--path", project_path.as_str()],
        ),
        (
            "push from main checkout",
            vec![
                "push",
                "--json",
                "--force",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                project_path.as_str(),
            ],
        ),
        (
            "status after push from main",
            vec!["status", "--json", "--path", project_path.as_str()],
        ),
    ] {
        let record = run_python_poly(name, &args, &server, &replacements);
        required_results.push((name, command_succeeded(&record)));
        steps.push(WorkflowStep::Command(record));
    }

    let recording_path = recording
        .save("main-push-python-adk")
        .expect("save push-from-main recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        MAIN_PUSH_HTTPMOCK_RECORDING_FILE,
        MAIN_PUSH_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records Python ADK push behavior from a main checkout.",
            "Python ADK persists the edit by creating and switching to an ADK branch; it does not merge that edit into main.",
        ],
        StepWorkflow {
            name: "push_from_main",
            description: "Initialize main, edit rules.txt, push from the main checkout, then verify local status is clean.",
            mutates_real_server: true,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "main push recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records permanent real Agent Studio traffic; resolves a merge conflict into main"]
fn record_merge_conflict_resolution_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let run_id = recording_run_id();
    let branch_name = format!("{MERGE_CONFLICT_BRANCH_PREFIX}-{run_id}");
    let base_branch_name = format!("{branch_name}-base");
    let main_branch_name = format!("{branch_name}-main");
    let base_line = format!("ADK recording conflict base {run_id}");
    let main_line = format!("ADK recording conflict main {run_id}");
    let branch_line = format!("ADK recording conflict branch {run_id}");
    let topic_file = format!("topics/adk_recording_conflict_{run_id}.yaml");
    let topic_name = format!("ADK Recording Conflict {run_id}");
    let base_topic_text = format!(
        "name: {topic_name}\nenabled: true\nactions: {base_line}\ncontent: This topic exists only to exercise Python ADK merge conflict recording.\nexample_queries:\n- How do conflict recordings work?\n"
    );
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    let base_dir = tmp.join("base");
    let branch_dir = tmp.join("branch");
    let main_dir = tmp.join("main");
    fs::create_dir_all(&base_dir).expect("create base recording dir");
    fs::create_dir_all(&branch_dir).expect("create branch recording dir");
    fs::create_dir_all(&main_dir).expect("create main recording dir");
    let base_project = base_dir.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let branch_project = branch_dir.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let main_project = main_dir.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let base_project_path = base_project.to_string_lossy().to_string();
    let branch_project_path = branch_project.to_string_lossy().to_string();
    let main_project_path = main_project.to_string_lossy().to_string();
    let base_dir_path = base_dir.to_string_lossy().to_string();
    let branch_dir_path = branch_dir.to_string_lossy().to_string();
    let main_dir_path = main_dir.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (branch_name.clone(), "${BRANCH_NAME}".to_string()),
        (base_branch_name.clone(), "${BASE_BRANCH_NAME}".to_string()),
        (main_branch_name.clone(), "${MAIN_BRANCH_NAME}".to_string()),
        (topic_file.clone(), "${CONFLICT_TOPIC_FILE}".to_string()),
        (topic_name.clone(), "${CONFLICT_TOPIC_NAME}".to_string()),
        (base_line.clone(), "${CONFLICT_BASE_LINE}".to_string()),
        (main_line.clone(), "${CONFLICT_MAIN_LINE}".to_string()),
        (branch_line.clone(), "${CONFLICT_BRANCH_LINE}".to_string()),
    ];
    let mut required_results = Vec::new();
    let mut steps = Vec::new();

    let init_base = run_python_poly(
        "init base checkout before conflict setup",
        &[
            "init",
            "--json",
            "--base-path",
            base_dir_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init base checkout before conflict setup",
        command_succeeded(&init_base),
    ));
    steps.push(WorkflowStep::Command(init_base));

    let create_base_branch = run_python_poly(
        "create base marker branch",
        &[
            "branch",
            "create",
            base_branch_name.as_str(),
            "--json",
            "--path",
            base_project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let base_branch_created = command_succeeded(&create_base_branch);
    required_results.push(("create base marker branch", base_branch_created));
    steps.push(WorkflowStep::Command(create_base_branch));

    let mut base_ready = false;
    if base_branch_created {
        let base_edit = write_text_file(
            "write conflict base topic",
            &base_project,
            topic_file.as_str(),
            base_topic_text.as_str(),
            &replacements,
        );
        required_results.push(("write conflict base topic", base_edit.success));
        steps.push(WorkflowStep::FileEdit(base_edit));

        let push_base = run_python_poly(
            "push conflict base marker branch",
            &[
                "push",
                "--json",
                "--force",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                base_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "push conflict base marker branch",
            command_succeeded(&push_base),
        ));
        steps.push(WorkflowStep::Command(push_base));

        let merge_base = run_python_poly(
            "merge base marker branch into main",
            &[
                "branch",
                "merge",
                "ADK recording conflict base marker",
                "--json",
                "--path",
                base_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        base_ready = command_succeeded(&merge_base);
        required_results.push(("merge base marker branch into main", base_ready));
        steps.push(WorkflowStep::Command(merge_base));
    }

    if base_ready {
        for (name, base_path) in [
            (
                "init branch checkout after conflict base merge",
                branch_dir_path.as_str(),
            ),
            (
                "init main-side checkout after conflict base merge",
                main_dir_path.as_str(),
            ),
        ] {
            let init = run_python_poly(
                name,
                &[
                    "init",
                    "--json",
                    "--base-path",
                    base_path,
                    "--region",
                    TARGET_REGION,
                    "--account_id",
                    TARGET_ACCOUNT_ID,
                    "--project_id",
                    TARGET_PROJECT_ID,
                ],
                &server,
                &replacements,
            );
            required_results.push((name, command_succeeded(&init)));
            steps.push(WorkflowStep::Command(init));
        }

        let create_branch = run_python_poly(
            "create branch for conflict resolution",
            &[
                "branch",
                "create",
                branch_name.as_str(),
                "--json",
                "--path",
                branch_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        let branch_created = command_succeeded(&create_branch);
        required_results.push(("create branch for conflict resolution", branch_created));
        steps.push(WorkflowStep::Command(create_branch));

        let create_main_branch = run_python_poly(
            "create main-side branch",
            &[
                "branch",
                "create",
                main_branch_name.as_str(),
                "--json",
                "--path",
                main_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        let main_branch_created = command_succeeded(&create_main_branch);
        required_results.push(("create main-side branch", main_branch_created));
        steps.push(WorkflowStep::Command(create_main_branch));

        if !branch_created || !main_branch_created {
            let recording_path = recording
                .save("merge-conflict-resolution-python-adk")
                .expect("save merge conflict resolution recording");
            write_step_recording_fixture(
                &api_key,
                recording_path,
                MERGE_CONFLICT_HTTPMOCK_RECORDING_FILE,
                MERGE_CONFLICT_COMMAND_MANIFEST_FILE,
                vec![
                    "This manifest records a permanent merge conflict and resolution into main.",
                    "A small base topic is first merged to main so the conflict target is unique to this recording.",
                    "The unresolved merge is expected to fail, then the recorded resolution accepts the branch side.",
                    "The main branch retains the resolved branch-side topic edit.",
                ],
                StepWorkflow {
                    name: "merge_conflict_resolution",
                    description: "Merge a small base topic to main, diverge two branches on that topic, merge one branch to advance main, record the other branch's conflict, resolve with theirs, then attempt branch cleanup.",
                    mutates_real_server: true,
                    cleanup: vec![],
                    steps,
                },
            );
            let _ = fs::remove_dir_all(&tmp);

            let failures: Vec<&str> = required_results
                .iter()
                .filter_map(|(name, success)| (!success).then_some(*name))
                .collect();
            assert!(
                failures.is_empty(),
                "merge conflict resolution recording had failed required steps: {failures:?}"
            );
            return;
        }

        let branch_edit = replace_text_file(
            "replace base topic action in branch checkout",
            &branch_project,
            topic_file.as_str(),
            base_line.as_str(),
            branch_line.as_str(),
            &replacements,
        );
        required_results.push((
            "replace base topic action in branch checkout",
            branch_edit.success,
        ));
        steps.push(WorkflowStep::FileEdit(branch_edit));

        let push_branch = run_python_poly(
            "push branch side of conflict",
            &[
                "push",
                "--json",
                "--force",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                branch_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "push branch side of conflict",
            command_succeeded(&push_branch),
        ));
        steps.push(WorkflowStep::Command(push_branch));

        let main_edit = replace_text_file(
            "replace base topic action in main-side checkout",
            &main_project,
            topic_file.as_str(),
            base_line.as_str(),
            main_line.as_str(),
            &replacements,
        );
        required_results.push((
            "replace base topic action in main-side checkout",
            main_edit.success,
        ));
        steps.push(WorkflowStep::FileEdit(main_edit));

        let push_main = run_python_poly(
            "push main-side conflict branch",
            &[
                "push",
                "--json",
                "--force",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                main_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "push main-side conflict branch",
            command_succeeded(&push_main),
        ));
        steps.push(WorkflowStep::Command(push_main));

        let merge_main_side = run_python_poly(
            "merge main-side branch into main",
            &[
                "branch",
                "merge",
                "ADK recording main-side conflict marker",
                "--json",
                "--path",
                main_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "merge main-side branch into main",
            command_succeeded(&merge_main_side),
        ));
        steps.push(WorkflowStep::Command(merge_main_side));

        let failed_merge = run_python_poly(
            "merge conflict without resolutions",
            &[
                "branch",
                "merge",
                "ADK recording unresolved conflict",
                "--json",
                "--path",
                branch_project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        let resolution_json = merge_resolutions_for_conflicts(&failed_merge);
        required_results.push((
            "merge conflict without resolutions",
            command_reported_failure(&failed_merge) && resolution_json.is_some(),
        ));
        steps.push(WorkflowStep::Command(failed_merge));

        if let Some(resolution_json) = resolution_json {
            let resolved_merge = run_python_poly(
                "merge conflict with theirs resolution",
                &[
                    "branch",
                    "merge",
                    "ADK recording resolved conflict",
                    "--json",
                    "--resolutions",
                    resolution_json.as_str(),
                    "--path",
                    branch_project_path.as_str(),
                ],
                &server,
                &replacements,
            );
            required_results.push((
                "merge conflict with theirs resolution",
                command_succeeded(&resolved_merge),
            ));
            steps.push(WorkflowStep::Command(resolved_merge));

            let current = run_python_poly(
                "current branch after conflict resolution merge",
                &[
                    "branch",
                    "current",
                    "--json",
                    "--path",
                    branch_project_path.as_str(),
                ],
                &server,
                &replacements,
            );
            required_results.push((
                "current branch after conflict resolution merge",
                command_succeeded(&current),
            ));
            steps.push(WorkflowStep::Command(current));

            let delete_branch = run_python_poly(
                "delete conflict resolution branch if still present",
                &[
                    "branch",
                    "delete",
                    branch_name.as_str(),
                    "--json",
                    "--path",
                    branch_project_path.as_str(),
                ],
                &server,
                &replacements,
            );
            steps.push(WorkflowStep::Command(delete_branch));

            for (name, delete_name) in [
                (
                    "delete base marker branch if still present",
                    base_branch_name.as_str(),
                ),
                (
                    "delete main-side branch if still present",
                    main_branch_name.as_str(),
                ),
            ] {
                let delete_branch = run_python_poly(
                    name,
                    &[
                        "branch",
                        "delete",
                        delete_name,
                        "--json",
                        "--path",
                        branch_project_path.as_str(),
                    ],
                    &server,
                    &replacements,
                );
                steps.push(WorkflowStep::Command(delete_branch));
            }
        }
    }

    let recording_path = recording
        .save("merge-conflict-resolution-python-adk")
        .expect("save merge conflict resolution recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        MERGE_CONFLICT_HTTPMOCK_RECORDING_FILE,
        MERGE_CONFLICT_COMMAND_MANIFEST_FILE,
        vec![
            "This manifest records a permanent merge conflict and resolution into main.",
            "A small base topic is first merged to main so the conflict target is unique to this recording.",
            "The unresolved merge is expected to fail, then the recorded resolution accepts the branch side.",
            "The main branch retains the resolved branch-side topic edit.",
        ],
        StepWorkflow {
            name: "merge_conflict_resolution",
            description: "Merge a small base topic to main, diverge two branches on that topic, merge one branch to advance main, record the other branch's conflict, resolve with theirs, then attempt branch cleanup.",
            mutates_real_server: true,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);

    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "merge conflict resolution recording had failed required steps: {failures:?}"
    );
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python pull resource coverage"]
fn record_pull_resource_coverage_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project with broad resource files",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
            "--output-json-projection",
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project with broad resource files",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    for (name, path) in [
        (
            "delete generated personality settings",
            "agent_settings/personality.yaml",
        ),
        ("delete generated role settings", "agent_settings/role.yaml"),
        (
            "delete generated project safety filters",
            "agent_settings/safety_filters.yaml",
        ),
        (
            "delete generated voice configuration",
            "voice/configuration.yaml",
        ),
        (
            "delete generated voice safety filters",
            "voice/safety_filters.yaml",
        ),
        (
            "delete generated voice ASR settings",
            "voice/speech_recognition/asr_settings.yaml",
        ),
    ] {
        let edit = delete_file(name, &project_root, path, &replacements);
        required_results.push((name, edit.success));
        steps.push(WorkflowStep::FileEdit(edit));
    }

    let status = run_python_poly(
        "status after deleting Python-generated resource files",
        &["status", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "status after deleting Python-generated resource files",
        command_succeeded(&status),
    ));
    steps.push(WorkflowStep::Command(status));

    let force_pull = run_python_poly(
        "force pull restores Python-generated resource files",
        &[
            "pull",
            "--json",
            "--force",
            "--path",
            project_path.as_str(),
            "--output-json-projection",
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "force pull restores Python-generated resource files",
        command_succeeded(&force_pull),
    ));
    steps.push(WorkflowStep::Command(force_pull));

    let clean_status = run_python_poly(
        "status after restoring generated resource files",
        &["status", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "status after restoring generated resource files",
        command_succeeded(&clean_status),
    ));
    steps.push(WorkflowStep::Command(clean_status));

    let recording_path = recording
        .save("pull-resource-coverage-python-adk")
        .expect("save pull resource coverage recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        PULL_RESOURCE_COVERAGE_HTTPMOCK_RECORDING_FILE,
        PULL_RESOURCE_COVERAGE_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for broad Python pull/init resource materialization.",
            "The delete_file steps are intentional: replay fails early if Rust init/pull did not write the same resource files Python wrote.",
            "This is not in the default replay scenario list until Rust resource materialization reaches parity.",
        ],
        StepWorkflow {
            name: "pull_resource_coverage",
            description: "Initialize the real project, delete representative Python-generated settings/channel/ASR files, record status, force pull, and verify they are restored.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("pull resource coverage", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python push resource command coverage"]
fn record_push_resource_coverage_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            PUSH_RESOURCE_COVERAGE_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project before broad push coverage",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before broad push coverage",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_branch = run_python_poly(
        "create broad resource coverage branch",
        &[
            "branch",
            "create",
            PUSH_RESOURCE_COVERAGE_BRANCH_NAME,
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    let branch_created = command_succeeded(&create_branch);
    required_results.push(("create broad resource coverage branch", branch_created));
    steps.push(WorkflowStep::Command(create_branch));

    if branch_created {
        for (name, path, content) in [
            (
                "write updated personality settings",
                "agent_settings/personality.yaml",
                RESOURCE_COVERAGE_PERSONALITY,
            ),
            (
                "write updated role settings",
                "agent_settings/role.yaml",
                RESOURCE_COVERAGE_ROLE,
            ),
            (
                "write updated project safety filters",
                "agent_settings/safety_filters.yaml",
                RESOURCE_COVERAGE_SAFETY_FILTERS,
            ),
            (
                "write updated voice ASR settings",
                "voice/speech_recognition/asr_settings.yaml",
                RESOURCE_COVERAGE_ASR_SETTINGS,
            ),
            (
                "write updated voice channel configuration",
                "voice/configuration.yaml",
                RESOURCE_COVERAGE_VOICE_CONFIG,
            ),
            (
                "write keyphrase boosting resources",
                "voice/speech_recognition/keyphrase_boosting.yaml",
                RESOURCE_COVERAGE_KEYPHRASES,
            ),
            (
                "write pronunciation resources",
                "voice/response_control/pronunciations.yaml",
                RESOURCE_COVERAGE_PRONUNCIATIONS,
            ),
            (
                "write transcript correction resources",
                "voice/speech_recognition/transcript_corrections.yaml",
                RESOURCE_COVERAGE_TRANSCRIPT_CORRECTIONS,
            ),
            (
                "write variant resources",
                "config/variant_attributes.yaml",
                RESOURCE_COVERAGE_VARIANTS,
            ),
            (
                "write API integration resources",
                "config/api_integrations.yaml",
                RESOURCE_COVERAGE_API_INTEGRATIONS,
            ),
        ] {
            let edit = write_text_file(name, &project_root, path, content, &replacements);
            required_results.push((name, edit.success));
            steps.push(WorkflowStep::FileEdit(edit));
        }

        let status = run_python_poly(
            "status after broad resource edits",
            &["status", "--json", "--path", project_path.as_str()],
            &server,
            &replacements,
        );
        required_results.push((
            "status after broad resource edits",
            command_succeeded(&status),
        ));
        steps.push(WorkflowStep::Command(status));

        let dry_run = run_python_poly(
            "push dry-run broad resource command payload",
            &[
                "push",
                "--output-json-commands",
                "--dry-run",
                "--skip-validation",
                "--email",
                RECORDER_EMAIL,
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "push dry-run broad resource command payload",
            command_succeeded(&dry_run),
        ));
        steps.push(WorkflowStep::Command(dry_run));

        let delete_branch = run_python_poly(
            "delete broad resource coverage branch",
            &[
                "branch",
                "delete",
                PUSH_RESOURCE_COVERAGE_BRANCH_NAME,
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "delete broad resource coverage branch",
            command_succeeded(&delete_branch),
        ));
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("push-resource-coverage-python-adk")
        .expect("save push resource coverage recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        PUSH_RESOURCE_COVERAGE_HTTPMOCK_RECORDING_FILE,
        PUSH_RESOURCE_COVERAGE_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python push command generation across resource families not yet covered by Rust.",
            "The workflow uses dry-run push only; it should not persist these broad resource edits to Agent Studio.",
            "This is not in the default replay scenario list until Rust command generation reaches parity.",
        ],
        StepWorkflow {
            name: "push_resource_coverage",
            description: "Create a throwaway branch, edit/update representative advanced resource files, record Python dry-run protobuf command JSON, then delete the branch.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("push resource coverage", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python semantic validation"]
fn record_semantic_validation_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project before semantic validation checks",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before semantic validation checks",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    for (name, path, content) in [
        (
            "write invalid duplicate default variants",
            "config/variant_attributes.yaml",
            SEMANTIC_VALIDATION_VARIANTS,
        ),
        (
            "write invalid API integration name",
            "config/api_integrations.yaml",
            SEMANTIC_VALIDATION_API_INTEGRATIONS,
        ),
        (
            "write invalid transcript correction",
            "voice/speech_recognition/transcript_corrections.yaml",
            SEMANTIC_VALIDATION_TRANSCRIPT_CORRECTIONS,
        ),
    ] {
        let edit = write_text_file(name, &project_root, path, content, &replacements);
        required_results.push((name, edit.success));
        steps.push(WorkflowStep::FileEdit(edit));
    }

    let validate = run_python_poly(
        "validate semantic resource errors",
        &["validate", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "validate semantic resource errors",
        command_reported_invalid_validation(&validate),
    ));
    steps.push(WorkflowStep::Command(validate));

    let push = run_python_poly(
        "push dry-run blocks on semantic validation errors",
        &[
            "push",
            "--json",
            "--dry-run",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "push dry-run blocks on semantic validation errors",
        command_reported_failure(&push),
    ));
    steps.push(WorkflowStep::Command(push));

    let recording_path = recording
        .save("semantic-validation-python-adk")
        .expect("save semantic validation recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        SEMANTIC_VALIDATION_HTTPMOCK_RECORDING_FILE,
        SEMANTIC_VALIDATION_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python's semantic resource validation, beyond YAML/JSON parseability.",
            "The invalid resources are syntactically valid YAML but should fail Python resource validators.",
            "This is not in the default replay scenario list until Rust validation reaches parity.",
        ],
        StepWorkflow {
            name: "semantic_validation",
            description: "Record Python validation output for invalid variant defaults, API integration naming, and transcript correction rules.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("semantic validation", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python local formatting"]
fn record_format_local_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project before formatting checks",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before formatting checks",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    for (name, path, content) in [
        (
            "write unformatted topic yaml",
            "topics/adk_format_recording.yaml",
            FORMAT_UNFORMATTED_TOPIC,
        ),
        (
            "write unformatted function python",
            "functions/adk_format_recording.py",
            FORMAT_UNFORMATTED_FUNCTION,
        ),
    ] {
        let edit = write_text_file(name, &project_root, path, content, &replacements);
        required_results.push((name, edit.success));
        steps.push(WorkflowStep::FileEdit(edit));
    }

    let check = run_python_poly(
        "format check finds YAML and Python changes",
        &[
            "format",
            "--json",
            "--check",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "format check finds YAML and Python changes",
        command_reported_failure(&check),
    ));
    steps.push(WorkflowStep::Command(check));

    let format = run_python_poly(
        "format fixes YAML and Python files",
        &["format", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "format fixes YAML and Python files",
        command_succeeded(&format),
    ));
    steps.push(WorkflowStep::Command(format));

    let ty_check = run_python_poly(
        "format check with ty after fixing files",
        &[
            "format",
            "--json",
            "--check",
            "--ty",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    steps.push(WorkflowStep::Command(ty_check));

    let recording_path = recording
        .save("format-local-python-adk")
        .expect("save local format recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        FORMAT_LOCAL_HTTPMOCK_RECORDING_FILE,
        FORMAT_LOCAL_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python local formatting behavior.",
            "The fixture records both YAML formatting and Python function formatting through Python ADK's formatter stack.",
            "The ty step is recorded as observed because local ty availability is part of Python's current command contract.",
            "This is not in the default replay scenario list until Rust formatting reaches parity.",
        ],
        StepWorkflow {
            name: "format_local",
            description: "Record Python format --check, format fix, and format --check --ty output after intentionally unformatted YAML and Python resources.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("format local", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python interactive-adjacent contracts"]
fn record_interactive_contracts_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
        (
            INTERACTIVE_BRANCH_NAME.to_string(),
            "${BRANCH_NAME}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project before interactive contracts",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before interactive contracts",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let create_prompted_branch = run_python_poly_with_options(
        "branch create prompts for missing branch name",
        &["branch", "create", "--path", project_path.as_str()],
        &server,
        &replacements,
        RunPythonOptions {
            stdin: Some(&format!("{INTERACTIVE_BRANCH_NAME}\n")),
            ..RunPythonOptions::default()
        },
    );
    let branch_created = command_succeeded(&create_prompted_branch);
    required_results.push((
        "branch create prompts for missing branch name",
        branch_created,
    ));
    steps.push(WorkflowStep::Command(create_prompted_branch));

    let switch_missing_json = run_python_poly(
        "branch switch json requires branch name",
        &[
            "branch",
            "switch",
            "--json",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "branch switch json requires branch name",
        command_reported_failure(&switch_missing_json),
    ));
    steps.push(WorkflowStep::Command(switch_missing_json));

    if branch_created {
        let delete_branch = run_python_poly(
            "delete branch created through prompt",
            &[
                "branch",
                "delete",
                INTERACTIVE_BRANCH_NAME,
                "--json",
                "--path",
                project_path.as_str(),
            ],
            &server,
            &replacements,
        );
        required_results.push((
            "delete branch created through prompt",
            command_succeeded(&delete_branch),
        ));
        steps.push(WorkflowStep::Command(delete_branch));
    }

    let recording_path = recording
        .save("interactive-contracts-python-adk")
        .expect("save interactive contracts recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        INTERACTIVE_CONTRACTS_HTTPMOCK_RECORDING_FILE,
        INTERACTIVE_CONTRACTS_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python interactive-adjacent CLI behavior.",
            "This records the deterministic stdin-backed branch-create prompt path plus JSON-mode failures for missing interactive arguments.",
            "Questionary menus are intentionally not automated here; they need a separate TTY test harness.",
            "This is not in the default replay scenario list until Rust interactive behavior is implemented.",
        ],
        StepWorkflow {
            name: "interactive_contracts",
            description: "Record Python branch create prompting for a missing branch name and JSON-mode branch switch failure when the branch name is omitted.",
            mutates_real_server: true,
            cleanup: vec![
                "poly branch delete ${BRANCH_NAME} --json --path ${TMP}/ben-ws/PROJECT-JTQKOKLM",
            ],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("interactive contracts", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python chat JSON behavior"]
fn record_chat_json_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();
    let init = run_python_poly(
        "init real project before chat JSON checks",
        &[
            "init",
            "--json",
            "--base-path",
            tmp_path.as_str(),
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init real project before chat JSON checks",
        command_succeeded(&init),
    ));
    steps.push(WorkflowStep::Command(init));

    let chat_message = run_python_poly(
        "chat json message with metadata flags",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--message",
            "Hello from the ADK recording.",
            "--functions",
            "--flows",
            "--state",
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "chat json message with metadata flags",
        command_succeeded(&chat_message),
    ));
    steps.push(WorkflowStep::Command(chat_message));

    let input_file = write_text_file(
        "write chat input file",
        &project_root,
        "adk_chat_input.txt",
        CHAT_INPUT_FILE_TEXT,
        &replacements,
    );
    required_results.push(("write chat input file", input_file.success));
    steps.push(WorkflowStep::FileEdit(input_file));

    let chat_input_file_path = project_root.join("adk_chat_input.txt");
    let chat_input_file_arg = chat_input_file_path.to_string_lossy().to_string();
    let chat_input_file = run_python_poly(
        "chat json input-file contract",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--input-file",
            chat_input_file_arg.as_str(),
        ],
        &server,
        &replacements,
    );
    steps.push(WorkflowStep::Command(chat_input_file));

    let recording_path = recording
        .save("chat-json-python-adk")
        .expect("save chat JSON recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        CHAT_JSON_HTTPMOCK_RECORDING_FILE,
        CHAT_JSON_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python chat JSON output and metadata shaping.",
            "The --message step records Python's JSON conversation/turn contract with metadata flags.",
            "The --input-file step records current Python behavior separately, including any existing Python bug.",
            "This is not in the default replay scenario list until Rust chat JSON behavior reaches parity.",
        ],
        StepWorkflow {
            name: "chat_json",
            description: "Record Python chat --json output for scripted message metadata and the current input-file path behavior.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("chat JSON", &required_results);
}

fn first_recorded_conversation_id(record: &CommandRecord) -> Option<String> {
    record
        .stdout_json
        .as_ref()
        .and_then(|json| json.pointer("/conversations/0/conversation_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn chat_record_has_error_turn(record: &CommandRecord) -> bool {
    record
        .stdout_json
        .as_ref()
        .and_then(|json| json.pointer("/conversations/0/turns/0/error"))
        .is_some()
}

fn target_project_config() -> String {
    format!(
        "region: {TARGET_REGION}\naccount_id: {TARGET_ACCOUNT_ID}\nproject_id: {TARGET_PROJECT_ID}\nbranch_id: main\n"
    )
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes chat restart/resume/exit controls"]
fn record_chat_session_controls_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();

    let project_config = target_project_config();
    let config = write_text_file(
        "write project config before chat session controls",
        &project_root,
        "project.yaml",
        project_config.as_str(),
        &replacements,
    );
    required_results.push((
        "write project config before chat session controls",
        config.success,
    ));
    steps.push(WorkflowStep::FileEdit(config));

    let restart_and_exit = run_python_poly_with_options(
        "chat json restart starts a new conversation before exit",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--message",
            "Hello before restart.",
            "--message",
            "/restart",
            "--message",
            "Hello after restart.",
            "--message",
            "/exit",
        ],
        &server,
        &replacements,
        RunPythonOptions {
            base_url: PythonBaseUrlMode::AgentStudioRoot,
            ..RunPythonOptions::default()
        },
    );
    required_results.push((
        "chat json restart starts a new conversation before exit",
        command_succeeded(&restart_and_exit),
    ));
    steps.push(WorkflowStep::Command(restart_and_exit));

    let open_chat = run_python_poly_with_options(
        "chat json leaves conversation open for resume",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--message",
            "Hello before resume.",
        ],
        &server,
        &replacements,
        RunPythonOptions {
            base_url: PythonBaseUrlMode::AgentStudioRoot,
            ..RunPythonOptions::default()
        },
    );
    let resume_id = first_recorded_conversation_id(&open_chat);
    required_results.push((
        "chat json leaves conversation open for resume",
        command_succeeded(&open_chat),
    ));
    required_results.push((
        "extract conversation id for resumed chat",
        resume_id.is_some(),
    ));
    steps.push(WorkflowStep::Command(open_chat));

    if let Some(resume_id) = resume_id {
        let resumed_exit = run_python_poly_with_options(
            "chat json resumes conversation id and exits",
            &[
                "chat",
                "--json",
                "--path",
                project_path.as_str(),
                "--environment",
                "sandbox",
                "--conversation-id",
                resume_id.as_str(),
                "--message",
                "/exit",
            ],
            &server,
            &replacements,
            RunPythonOptions {
                base_url: PythonBaseUrlMode::AgentStudioRoot,
                ..RunPythonOptions::default()
            },
        );
        required_results.push((
            "chat json resumes conversation id and exits",
            command_succeeded(&resumed_exit),
        ));
        steps.push(WorkflowStep::Command(resumed_exit));
    }

    let goodbye = run_python_poly_with_options(
        "chat json records server-side conversation-ended metadata",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--message",
            "Goodbye, that is all for now.",
        ],
        &server,
        &replacements,
        RunPythonOptions {
            base_url: PythonBaseUrlMode::AgentStudioRoot,
            ..RunPythonOptions::default()
        },
    );
    required_results.push((
        "chat json records server-side conversation-ended metadata",
        command_succeeded(&goodbye),
    ));
    steps.push(WorkflowStep::Command(goodbye));

    let recording_path = recording
        .save("chat-session-controls-python-adk")
        .expect("save chat session controls recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        CHAT_SESSION_CONTROLS_HTTPMOCK_RECORDING_FILE,
        CHAT_SESSION_CONTROLS_COMMAND_MANIFEST_FILE,
        vec![
            "Replay fixture for chat control commands and resumed conversations.",
            "The restart step records Python's JSON behavior for /restart and /exit in one scripted run.",
            "The resume step records that --conversation-id skips session creation and can be ended with /exit.",
            "The goodbye step documents the server-side conversation_ended flag as returned by the real project.",
        ],
        StepWorkflow {
            name: "chat_session_controls",
            description: "Record Python chat --json behavior for /restart, /exit, resumed conversation IDs, and conversation-ended metadata.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("chat session controls", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes pull --force local cleanup"]
fn record_pull_force_cleanup_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();

    let project_config = target_project_config();
    let config = write_text_file(
        "write project config before pull force cleanup",
        &project_root,
        "project.yaml",
        project_config.as_str(),
        &replacements,
    );
    required_results.push((
        "write project config before pull force cleanup",
        config.success,
    ));
    steps.push(WorkflowStep::FileEdit(config));

    let topic = write_text_file(
        "write pull force topic before force pull",
        &project_root,
        PULL_FORCE_LOCAL_TOPIC_FILE,
        PULL_FORCE_LOCAL_TOPIC_TEXT,
        &replacements,
    );
    required_results.push(("write pull force topic before force pull", topic.success));
    steps.push(WorkflowStep::FileEdit(topic));

    let function = write_text_file(
        "write pull force function before force pull",
        &project_root,
        PULL_FORCE_LOCAL_FUNCTION_FILE,
        PULL_FORCE_LOCAL_FUNCTION_TEXT,
        &replacements,
    );
    required_results.push((
        "write pull force function before force pull",
        function.success,
    ));
    steps.push(WorkflowStep::FileEdit(function));

    let dirty_status = run_python_poly(
        "status reports local-only resources before force pull",
        &["status", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "status reports local-only resources before force pull",
        command_succeeded(&dirty_status),
    ));
    steps.push(WorkflowStep::Command(dirty_status));

    let force_pull = run_python_poly(
        "pull force removes local-only resources",
        &["pull", "--json", "--force", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "pull force removes local-only resources",
        command_succeeded(&force_pull),
    ));
    steps.push(WorkflowStep::Command(force_pull));

    required_results.push((
        "pull force removed local-only topic from disk",
        !project_root.join(PULL_FORCE_LOCAL_TOPIC_FILE).exists(),
    ));
    required_results.push((
        "pull force removed local-only function from disk",
        !project_root.join(PULL_FORCE_LOCAL_FUNCTION_FILE).exists(),
    ));

    let clean_status = run_python_poly(
        "status after force pull is clean",
        &["status", "--json", "--path", project_path.as_str()],
        &server,
        &replacements,
    );
    required_results.push((
        "status after force pull is clean",
        command_succeeded(&clean_status),
    ));
    steps.push(WorkflowStep::Command(clean_status));

    let recording_path = recording
        .save("pull-force-cleanup-python-adk")
        .expect("save pull force cleanup recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        PULL_FORCE_CLEANUP_HTTPMOCK_RECORDING_FILE,
        PULL_FORCE_CLEANUP_COMMAND_MANIFEST_FILE,
        vec![
            "Replay fixture for Python pull --force cleanup of local-only resources.",
            "The manifest records explicit file edits before pull, then asserts Python removes those files from disk.",
        ],
        StepWorkflow {
            name: "pull_force_cleanup",
            description: "Record Python pull --force behavior when a checkout contains local-only resources absent from Agent Studio.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("pull force cleanup", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes chat errors and metadata"]
fn record_chat_error_metadata_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();

    let project_config = target_project_config();
    let config = write_text_file(
        "write project config before chat error metadata checks",
        &project_root,
        "project.yaml",
        project_config.as_str(),
        &replacements,
    );
    required_results.push((
        "write project config before chat error metadata checks",
        config.success,
    ));
    steps.push(WorkflowStep::FileEdit(config));

    let metadata = run_python_poly_with_options(
        "chat json all metadata flags",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--message",
            "Hello from the ADK metadata recording.",
            "--functions",
            "--flows",
            "--state",
        ],
        &server,
        &replacements,
        RunPythonOptions {
            base_url: PythonBaseUrlMode::AgentStudioRoot,
            ..RunPythonOptions::default()
        },
    );
    required_results.push(("chat json all metadata flags", command_succeeded(&metadata)));
    steps.push(WorkflowStep::Command(metadata));

    let invalid_resume = run_python_poly_with_options(
        "chat json send-message error is captured in turn",
        &[
            "chat",
            "--json",
            "--path",
            project_path.as_str(),
            "--environment",
            "sandbox",
            "--conversation-id",
            "AS_CHAT_adk-rs-recording-missing-conversation",
            "--message",
            "This message should record the Python error contract.",
        ],
        &server,
        &replacements,
        RunPythonOptions {
            base_url: PythonBaseUrlMode::AgentStudioRoot,
            ..RunPythonOptions::default()
        },
    );
    required_results.push((
        "chat json send-message command completes with JSON error payload",
        invalid_resume.exit_code == 0,
    ));
    required_results.push((
        "chat json send-message error is captured in turn",
        chat_record_has_error_turn(&invalid_resume),
    ));
    steps.push(WorkflowStep::Command(invalid_resume));

    let recording_path = recording
        .save("chat-error-metadata-python-adk")
        .expect("save chat error metadata recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        CHAT_ERROR_METADATA_HTTPMOCK_RECORDING_FILE,
        CHAT_ERROR_METADATA_COMMAND_MANIFEST_FILE,
        vec![
            "Replay fixture for chat metadata flags and Python's JSON error-turn contract.",
            "The metadata step records --functions/--flows/--state projection in chat JSON output.",
            "The invalid --conversation-id step records that Python captures send-message failures as a turn-level error.",
        ],
        StepWorkflow {
            name: "chat_error_metadata",
            description: "Record Python chat --json metadata projection and send-message error handling.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("chat error metadata", &required_results);
}

#[test]
#[ignore = "records real Agent Studio traffic; refreshes Python CLI surface and diff edge behavior"]
fn record_cli_diff_edges_with_python_adk_and_httpmock() {
    let api_key = api_key_from_env();
    let server = MockServer::start();
    server.forward_to(AGENT_STUDIO_HOST_URL, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });

    let tmp = temp_recording_dir();
    fs::create_dir_all(&tmp).expect("create temp recording dir");
    let project_root = tmp.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID);
    let project_path = project_root.to_string_lossy().to_string();
    let tmp_path = tmp.to_string_lossy().to_string();
    let replacements = vec![
        (tmp_path.clone(), "${TMP}".to_string()),
        (
            httpmock_adk_base_url(&server),
            "${HTTPMOCK_BASE_URL}".to_string(),
        ),
    ];

    let mut steps = Vec::new();
    let mut required_results: Vec<(&'static str, bool)> = Vec::new();

    let invalid_region = run_python_poly(
        "init rejects invalid region at parser level",
        &[
            "init",
            "--json",
            "--region",
            "not-a-region",
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "init rejects invalid region at parser level",
        invalid_region.exit_code != 0,
    ));
    steps.push(WorkflowStep::Command(invalid_region));

    let default_base_path_init = run_python_poly_with_options(
        "init default base path uses current working directory",
        &[
            "init",
            "--json",
            "--region",
            TARGET_REGION,
            "--account_id",
            TARGET_ACCOUNT_ID,
            "--project_id",
            TARGET_PROJECT_ID,
        ],
        &server,
        &replacements,
        RunPythonOptions {
            cwd: Some(tmp.as_path()),
            ..RunPythonOptions::default()
        },
    );
    required_results.push((
        "init default base path uses current working directory",
        command_succeeded(&default_base_path_init),
    ));
    steps.push(WorkflowStep::Command(default_base_path_init));

    let deployments_verbose_after_subcommand = run_python_poly(
        "deployments list rejects verbose after subcommand",
        &[
            "deployments",
            "list",
            "--verbose",
            "--path",
            project_path.as_str(),
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "deployments list rejects verbose after subcommand",
        deployments_verbose_after_subcommand.exit_code != 0,
    ));
    steps.push(WorkflowStep::Command(deployments_verbose_after_subcommand));

    let switch_help = run_python_poly(
        "branch switch help exposes output-json-projection",
        &["branch", "switch", "--help"],
        &server,
        &replacements,
    );
    required_results.push((
        "branch switch help exposes output-json-projection",
        switch_help
            .stdout
            .as_deref()
            .is_some_and(|out| out.contains("--output-json-projection")),
    ));
    steps.push(WorkflowStep::Command(switch_help));

    let edit = append_text_file(
        "append local main diff edge edit",
        &project_root,
        MUTATING_EDIT_FILE,
        "\n\n# ADK recording diff edge\nThis line records Python diff edge behavior.\n",
        &replacements,
    );
    required_results.push(("append local main diff edge edit", edit.success));
    steps.push(WorkflowStep::FileEdit(edit));

    let relative_diff = run_python_poly(
        "diff relative file filter",
        &[
            "diff",
            "--json",
            "--path",
            project_path.as_str(),
            "--files",
            MUTATING_EDIT_FILE,
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "diff relative file filter",
        command_reported_failure(&relative_diff),
    ));
    steps.push(WorkflowStep::Command(relative_diff));

    let absolute_file = project_root.join(MUTATING_EDIT_FILE);
    let absolute_file_arg = absolute_file.to_string_lossy().to_string();
    let absolute_diff = run_python_poly(
        "diff absolute file filter",
        &[
            "diff",
            "--json",
            "--path",
            project_path.as_str(),
            "--files",
            absolute_file_arg.as_str(),
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "diff absolute file filter",
        command_succeeded(&absolute_diff),
    ));
    steps.push(WorkflowStep::Command(absolute_diff));

    let before_main_diff = run_python_poly(
        "diff before main against dirty local checkout",
        &[
            "diff",
            "--json",
            "--path",
            project_path.as_str(),
            "--before",
            "main",
        ],
        &server,
        &replacements,
    );
    required_results.push((
        "diff before main against dirty local checkout",
        command_succeeded(&before_main_diff),
    ));
    steps.push(WorkflowStep::Command(before_main_diff));

    let review_without_github = run_python_poly_with_options(
        "review create relative file filter without github token",
        &[
            "review",
            "--path",
            project_path.as_str(),
            "create",
            "--json",
            "--files",
            MUTATING_EDIT_FILE,
        ],
        &server,
        &replacements,
        RunPythonOptions {
            remove_env: &["GITHUB_ACCESS_TOKEN"],
            ..RunPythonOptions::default()
        },
    );
    required_results.push((
        "review create relative file filter without github token",
        command_reported_failure(&review_without_github),
    ));
    steps.push(WorkflowStep::Command(review_without_github));

    let recording_path = recording
        .save("cli-diff-edges-python-adk")
        .expect("save CLI/diff edge recording");
    write_step_recording_fixture(
        &api_key,
        recording_path,
        CLI_DIFF_EDGES_HTTPMOCK_RECORDING_FILE,
        CLI_DIFF_EDGES_COMMAND_MANIFEST_FILE,
        vec![
            "Pending parity fixture for Python CLI surface details and diff/review edge behavior.",
            "This records parser-level invalid region behavior, default init base path, verbose flag placement, hidden/visible help text, file filters, review filtering, and --before main local diff behavior.",
            "This is not in the default replay scenario list until these surface contracts are implemented in Rust.",
        ],
        StepWorkflow {
            name: "cli_diff_edges",
            description: "Record Python CLI parser contracts plus file-filtered diff/review and named diff behavior on a dirty local checkout.",
            mutates_real_server: false,
            cleanup: vec![],
            steps,
        },
    );
    let _ = fs::remove_dir_all(&tmp);
    assert_required_results("CLI/diff edges", &required_results);
}

fn run_python_poly(
    name: &'static str,
    args: &[&str],
    server: &MockServer,
    replacements: &[(String, String)],
) -> CommandRecord {
    run_python_poly_with_options(
        name,
        args,
        server,
        replacements,
        RunPythonOptions::default(),
    )
}

#[derive(Clone, Copy, Default)]
enum PythonBaseUrlMode {
    #[default]
    AdkV1,
    AgentStudioRoot,
}

impl PythonBaseUrlMode {
    fn base_url(self, server: &MockServer) -> String {
        match self {
            PythonBaseUrlMode::AdkV1 => httpmock_adk_base_url(server),
            PythonBaseUrlMode::AgentStudioRoot => server.base_url(),
        }
    }
}

#[derive(Default)]
struct RunPythonOptions<'a> {
    stdin: Option<&'a str>,
    cwd: Option<&'a Path>,
    remove_env: &'a [&'a str],
    base_url: PythonBaseUrlMode,
}

fn run_python_poly_with_options(
    name: &'static str,
    args: &[&str],
    server: &MockServer,
    replacements: &[(String, String)],
    options: RunPythonOptions<'_>,
) -> CommandRecord {
    let replacements = command_replacements(replacements);
    let base_url = options.base_url.base_url(server);
    let mut command = Command::new(python_adk_bin());
    command
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .env("POLY_ADK_BASE_URL", &base_url)
        .args(args);
    for key in options.remove_env {
        command.env_remove(key);
    }
    if let Some(cwd) = options.cwd {
        command.current_dir(cwd);
    }
    let output = if let Some(stdin) = options.stdin {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("failed to spawn Python poly for {name}: {error}"));
        child
            .stdin
            .as_mut()
            .expect("python poly stdin")
            .write_all(stdin.as_bytes())
            .unwrap_or_else(|error| panic!("failed to write stdin for {name}: {error}"));
        child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("failed to wait for Python poly {name}: {error}"))
    } else {
        command
            .output()
            .unwrap_or_else(|error| panic!("failed to run Python poly for {name}: {error}"))
    };
    let exit_code = output.status.code().unwrap_or(1);
    let stdout_raw = normalize_text(&String::from_utf8_lossy(&output.stdout), &replacements);
    let stderr = normalize_text(&String::from_utf8_lossy(&output.stderr), &replacements);
    let stdout_json = serde_json::from_str::<Value>(stdout_raw.trim())
        .ok()
        .map(normalize_json_value);
    CommandRecord {
        name,
        argv: std::iter::once("poly".to_string())
            .chain(args.iter().map(|arg| normalize_text(arg, &replacements)))
            .collect(),
        stdin: options
            .stdin
            .map(|stdin| normalize_text(stdin, &replacements)),
        exit_code,
        stdout_json,
        stdout: if stdout_raw.trim().is_empty()
            || serde_json::from_str::<Value>(stdout_raw.trim()).is_ok()
        {
            None
        } else {
            Some(stdout_raw)
        },
        stderr,
    }
}

fn append_text_file(
    name: &'static str,
    project_root: &std::path::Path,
    relative_path: &str,
    content: &str,
    replacements: &[(String, String)],
) -> FileEditRecord {
    let path = project_root.join(relative_path);
    let result = (|| -> Result<(), String> {
        let mut existing = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        existing.push_str(content);
        fs::write(&path, existing)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Ok(())
    })();
    let success = result.is_ok();
    FileEditRecord {
        name,
        operation: "append_text",
        path: normalize_text(relative_path, replacements),
        content: Some(normalize_text(content, replacements)),
        target: None,
        replacement: None,
        success,
        error: result.err(),
    }
}

fn write_text_file(
    name: &'static str,
    project_root: &std::path::Path,
    relative_path: &str,
    content: &str,
    replacements: &[(String, String)],
) -> FileEditRecord {
    let path = project_root.join(relative_path);
    let result = (|| -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        fs::write(&path, content)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Ok(())
    })();
    let success = result.is_ok();
    FileEditRecord {
        name,
        operation: "write_text",
        path: normalize_text(relative_path, replacements),
        content: Some(normalize_text(content, replacements)),
        target: None,
        replacement: None,
        success,
        error: result.err(),
    }
}

fn replace_text_file(
    name: &'static str,
    project_root: &std::path::Path,
    relative_path: &str,
    target: &str,
    replacement: &str,
    replacements: &[(String, String)],
) -> FileEditRecord {
    let path = project_root.join(relative_path);
    let result = (|| -> Result<(), String> {
        let existing = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if !existing.contains(target) {
            return Err(format!(
                "failed to replace text in {}: target text not found",
                path.display()
            ));
        }
        fs::write(&path, existing.replace(target, replacement))
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Ok(())
    })();
    let success = result.is_ok();
    FileEditRecord {
        name,
        operation: "replace_text",
        path: normalize_text(relative_path, replacements),
        content: None,
        target: Some(normalize_text(target, replacements)),
        replacement: Some(normalize_text(replacement, replacements)),
        success,
        error: result.err(),
    }
}

fn delete_file(
    name: &'static str,
    project_root: &std::path::Path,
    relative_path: &str,
    replacements: &[(String, String)],
) -> FileEditRecord {
    let path = project_root.join(relative_path);
    let result = fs::remove_file(&path)
        .map_err(|error| format!("failed to delete {}: {error}", path.display()));
    let success = result.is_ok();
    FileEditRecord {
        name,
        operation: "delete_file",
        path: normalize_text(relative_path, replacements),
        content: None,
        target: None,
        replacement: None,
        success,
        error: result.err(),
    }
}

fn write_step_recording_fixture(
    api_key: &str,
    recording_path: PathBuf,
    httpmock_file: &'static str,
    manifest_file: &'static str,
    replay_notes: Vec<&'static str>,
    workflow: StepWorkflow,
) {
    let fixture_dir = recording_fixture_dir();
    fs::create_dir_all(&fixture_dir).expect("create recording fixture dir");
    let httpmock_fixture = fixture_dir.join(httpmock_file);
    let mut recording_yaml = fs::read_to_string(&recording_path).expect("read httpmock recording");
    recording_yaml = recording_yaml.replace(api_key, "<redacted>");
    fs::write(&httpmock_fixture, recording_yaml).expect("write redacted httpmock fixture");

    let manifest = StepManifest {
        schema_version: 3,
        source: ManifestSource {
            implementation: "python-adk",
            recorder: "rust-httpmock-forwarding",
            server: "real-agent-studio",
            poly_binary: PYTHON_ADK_BIN_DISPLAY,
            region: TARGET_REGION,
            account_id: TARGET_ACCOUNT_ID,
            project_id: TARGET_PROJECT_ID,
            project_name: TARGET_PROJECT_NAME,
        },
        replay_notes,
        httpmock_recording: httpmock_file.to_string(),
        workflows: vec![workflow],
    };
    let manifest_yaml = serde_yaml::to_string(&manifest).expect("serialize step manifest");
    fs::write(fixture_dir.join(manifest_file), manifest_yaml).expect("write step manifest");
}

fn command_succeeded(record: &CommandRecord) -> bool {
    record.exit_code == 0
        && record
            .stdout_json
            .as_ref()
            .and_then(|json| json.get("success"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
}

fn command_reported_failure(record: &CommandRecord) -> bool {
    record.exit_code != 0
        || record
            .stdout_json
            .as_ref()
            .and_then(|json| json.get("success"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn command_reported_invalid_validation(record: &CommandRecord) -> bool {
    record.exit_code == 0
        && record
            .stdout_json
            .as_ref()
            .and_then(|json| json.get("valid"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn assert_required_results(label: &str, required_results: &[(&'static str, bool)]) {
    let failures: Vec<&str> = required_results
        .iter()
        .filter_map(|(name, success)| (!success).then_some(*name))
        .collect();
    assert!(
        failures.is_empty(),
        "{label} recording had failed required steps: {failures:?}"
    );
}

fn normalize_json_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_json_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = match key.as_str() {
                        "command_id" => Value::String("${COMMAND_ID}".to_string()),
                        "created_at" => Value::String("${TIMESTAMP}".to_string()),
                        _ => normalize_json_value(value),
                    };
                    (key, value)
                })
                .collect(),
        ),
        other => other,
    }
}

fn command_replacements(replacements: &[(String, String)]) -> Vec<(String, String)> {
    let mut all = machine_path_replacements();
    all.extend(replacements.iter().cloned());
    all
}

fn machine_path_replacements() -> Vec<(String, String)> {
    let mut replacements = Vec::new();
    let bin = PathBuf::from(python_adk_bin());
    if bin.is_absolute() {
        if let Some(bin_dir) = bin.parent() {
            if let Some(venv_dir) = bin_dir.parent() {
                replacements.push((
                    venv_dir.to_string_lossy().to_string(),
                    "${PYTHON_ADK_VENV}".to_string(),
                ));
                if let Some(root_dir) = venv_dir.parent() {
                    replacements.push((
                        root_dir.to_string_lossy().to_string(),
                        "${PYTHON_ADK_ROOT}".to_string(),
                    ));
                }
            }
        }
    }
    if let Ok(cwd) = std::env::var("PYTHON_ADK_CWD") {
        replacements.push((cwd, "${PYTHON_ADK_ROOT}".to_string()));
    }
    replacements
}

fn api_key_from_env() -> String {
    ["POLY_ADK_KEY", "POLY_ADK_KEY_US", "POLY_ADK_KEY_US_1"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .expect("POLY_ADK_KEY, POLY_ADK_KEY_US, or POLY_ADK_KEY_US_1 must be set")
}

fn merge_resolutions_for_conflicts(record: &CommandRecord) -> Option<String> {
    let conflicts = record.stdout_json.as_ref()?.get("conflicts")?.as_array()?;
    let resolutions = conflicts
        .iter()
        .filter_map(|conflict| {
            let path = conflict.get("path")?.as_array()?.clone();
            Some(json!({
                "path": path,
                "strategy": "theirs"
            }))
        })
        .collect::<Vec<_>>();
    (!resolutions.is_empty())
        .then(|| serde_json::to_string(&resolutions).ok())
        .flatten()
}
