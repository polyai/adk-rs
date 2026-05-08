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
const COMMAND_MANIFEST_FILE: &str = "real-agent-studio.commands.yaml";
const HTTPMOCK_RECORDING_FILE: &str = "real-agent-studio.httpmock.yaml";
const MUTATING_COMMAND_MANIFEST_FILE: &str = "real-agent-studio-mutating.commands.yaml";
const MUTATING_HTTPMOCK_RECORDING_FILE: &str = "real-agent-studio-mutating.httpmock.yaml";
const MUTATING_BRANCH_NAME: &str = "adk-rs-recording-mutating";
const MUTATING_EDIT_FILE: &str = "agent_settings/rules.txt";
const MUTATING_EDIT_TEXT: &str = "\n\n# ADK recording branch edit\nThis line was added by the Python ADK httpmock mutating workflow.\n";
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
    content: String,
    success: bool,
    error: Option<String>,
}

#[test]
#[ignore = "records real Agent Studio traffic; requires POLY_ADK_KEY"]
fn record_real_agent_studio_with_python_adk_and_httpmock() {
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
    let httpmock_fixture = fixture_dir.join(HTTPMOCK_RECORDING_FILE);
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
            "The HTTP traffic lives in real-agent-studio.httpmock.yaml, saved by httpmock record/playback.",
            "Authentication headers are redacted after recording.",
            "Substitute ${TMP} with a temporary working directory when replaying command expectations.",
        ],
        httpmock_recording: HTTPMOCK_RECORDING_FILE.to_string(),
        workflows,
    };
    let manifest_yaml = serde_yaml::to_string(&manifest).expect("serialize manifest");
    fs::write(fixture_dir.join(COMMAND_MANIFEST_FILE), manifest_yaml)
        .expect("write command manifest");

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "records mutating real Agent Studio traffic; creates, pushes, and deletes a branch"]
fn record_real_agent_studio_mutating_branch_workflow_with_python_adk_and_httpmock() {
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
        .save("real-agent-studio-mutating-python-adk")
        .expect("save mutating httpmock recording");
    let fixture_dir = recording_fixture_dir();
    fs::create_dir_all(&fixture_dir).expect("create recording fixture dir");
    let httpmock_fixture = fixture_dir.join(MUTATING_HTTPMOCK_RECORDING_FILE);
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
            "The HTTP traffic lives in real-agent-studio-mutating.httpmock.yaml, saved by httpmock record/playback.",
            "Authentication headers are redacted after recording.",
            "Apply file_edit steps before replaying the following command steps.",
            "The branch is deleted at the end of the recorded workflow.",
        ],
        httpmock_recording: MUTATING_HTTPMOCK_RECORDING_FILE.to_string(),
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
        fixture_dir.join(MUTATING_COMMAND_MANIFEST_FILE),
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

fn run_python_poly(
    name: &'static str,
    args: &[&str],
    server: &MockServer,
    replacements: &[(String, String)],
) -> CommandRecord {
    let output = Command::new(python_adk_bin())
        .env("POLY_ADK_BASE_URL_US", httpmock_adk_base_url(server))
        .env("POLY_ADK_BASE_URL_US_1", httpmock_adk_base_url(server))
        .env("POLY_ADK_BASE_URL", httpmock_adk_base_url(server))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run Python poly for {name}: {error}"));
    let exit_code = output.status.code().unwrap_or(1);
    let stdout_raw = normalize_text(&String::from_utf8_lossy(&output.stdout), replacements);
    let stderr = normalize_text(&String::from_utf8_lossy(&output.stderr), replacements);
    let stdout_json = serde_json::from_str::<Value>(stdout_raw.trim()).ok();
    CommandRecord {
        name,
        argv: std::iter::once("poly".to_string())
            .chain(args.iter().map(|arg| normalize_text(arg, replacements)))
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
        name: "append branch edit to rules.txt",
        operation: "append_text",
        path: normalize_text(relative_path, replacements),
        content: normalize_text(content, replacements),
        success,
        error: result.err(),
    }
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

fn normalize_text(input: &str, replacements: &[(String, String)]) -> String {
    replacements
        .iter()
        .fold(input.to_string(), |value, (from, to)| {
            value.replace(from, to)
        })
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
