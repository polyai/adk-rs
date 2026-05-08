use httpmock::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const PYTHON_ADK_BIN_ENV: &str = "PYTHON_ADK_BIN";
const PYTHON_ADK_BIN_DISPLAY: &str = "${PYTHON_ADK_BIN:-poly}";
const AGENT_STUDIO_HOST_URL: &str = "https://api.us.poly.ai";
const TARGET_REGION: &str = "us-1";
const TARGET_ACCOUNT_ID: &str = "ben-ws";
const TARGET_PROJECT_ID: &str = "PROJECT-JTQKOKLM";
const TARGET_PROJECT_NAME: &str = "Test";
const RECORDING_FIXTURE_DIR: &str = "tests/fixtures/python-adk-recordings";
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
const MUTATING_BRANCH_NAME: &str = "adk-rs-recording-mutating";
const CREATE_DELETE_BRANCH_NAME: &str = "adk-rs-recording-create-delete";
const DIRTY_SWITCH_BRANCH_NAME: &str = "adk-rs-recording-dirty-switch";
const PULL_CONFLICT_BRANCH_NAME: &str = "adk-rs-recording-pull-conflict";
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
const RECORDER_EMAIL: &str = "adk-recorder@example.com";

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
            "This command manifest was produced by the ignored python_adk_recording_test.",
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

fn run_python_poly(
    name: &'static str,
    args: &[&str],
    server: &MockServer,
    replacements: &[(String, String)],
) -> CommandRecord {
    let replacements = command_replacements(replacements);
    let output = Command::new(python_adk_bin())
        .env("POLY_ADK_BASE_URL_US", httpmock_adk_base_url(server))
        .env("POLY_ADK_BASE_URL_US_1", httpmock_adk_base_url(server))
        .env("POLY_ADK_BASE_URL", httpmock_adk_base_url(server))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run Python poly for {name}: {error}"));
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
    relative_path: &'static str,
    content: &'static str,
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
    relative_path: &'static str,
    content: &'static str,
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
    relative_path: &'static str,
    target: &'static str,
    replacement: &'static str,
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
    relative_path: &'static str,
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

fn normalize_text(input: &str, replacements: &[(String, String)]) -> String {
    replacements
        .iter()
        .fold(input.to_string(), |value, (from, to)| {
            value.replace(from, to)
        })
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

fn httpmock_adk_base_url(server: &MockServer) -> String {
    format!("{}/adk/v1", server.base_url())
}

fn temp_recording_dir() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("adk-rs-python-adk-recording-{ts}"))
}

fn recording_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RECORDING_FIXTURE_DIR)
}

fn api_key_from_env() -> String {
    ["POLY_ADK_KEY", "POLY_ADK_KEY_US", "POLY_ADK_KEY_US_1"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .expect("POLY_ADK_KEY, POLY_ADK_KEY_US, or POLY_ADK_KEY_US_1 must be set")
}

fn python_adk_bin() -> String {
    std::env::var(PYTHON_ADK_BIN_ENV).unwrap_or_else(|_| "poly".to_string())
}
