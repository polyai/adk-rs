mod support;

use httpmock::Method::{DELETE, GET, POST};
use httpmock::MockServer;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use support::cli::{
    make_temp_project_dir, poly_without_fallback_command, run_poly_offline, temp_dir,
};

fn run_poly(args: &[&str]) -> Output {
    run_poly_offline(args)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn write_us_project(prefix: &str) -> PathBuf {
    let project_dir = temp_dir(prefix);
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    project_dir
}

fn deployment_json(id: &str, hash: &str, env: &str, message: &str) -> Value {
    json!({
        "id": id,
        "version_hash": hash,
        "created_at": "2026-05-01T12:00:00Z",
        "created_by": "tester@example.com",
        "artifact_version": format!("artifact-{id}"),
        "function_deployment_version": format!("lambda-{id}"),
        "client_env": env,
        "deployment_metadata": {
            "deployment_type": "manual",
            "deployment_message": message
        }
    })
}

fn mock_active_deployments<'a>(server: &'a MockServer, body: Value) -> httpmock::Mock<'a> {
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments/active");
        then.status(200).json_body(body);
    })
}

fn assert_not_json_object(text: &str) {
    assert!(
        !text.trim_start().starts_with('{'),
        "human-readable output should not look like JSON: {text:?}"
    );
}

#[test]
fn init_text_auto_selects_single_account_and_prompts_for_project() {
    let server = MockServer::start();
    let accounts = server.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts");
        then.status(200).json_body(json!([
            {"id": "acc-1", "name": "Test Account", "active": true}
        ]));
    });
    let projects = server.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts/acc-1/projects");
        then.status(200).json_body(json!({
            "projects": [
                {"id": "proj-1", "name": "First Project"},
                {"id": "proj-2", "name": "Second Project"}
            ]
        }));
    });

    let base = temp_dir("adk-rs-init-interactive");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args([
            "init",
            "--base-path",
            base.to_string_lossy().as_ref(),
            "--region",
            "us-1",
            "--from-projection",
            "{}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly init");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1\n")
        .expect("write selection");
    let output = child.wait_with_output().expect("wait poly init");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Initialising project"));
    assert!(stdout.contains("Auto-selected account Test Account."));
    assert!(stdout.contains("Select Project"));
    assert!(stdout.contains("Project initialized at"));
    assert_not_json_object(&stdout);
    let project_yaml = fs::read_to_string(base.join("acc-1").join("proj-1").join("project.yaml"))
        .expect("project config");
    assert!(project_yaml.contains("project_name: First Project"));
    accounts.assert();
    projects.assert();
}

#[test]
fn init_text_auto_selects_single_accessible_region() {
    let accessible = MockServer::start();
    let inaccessible = MockServer::start();
    accessible.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts");
        then.status(200).json_body(json!([
            {"id": "acc-1", "name": "Test Account", "active": true}
        ]));
    });
    accessible.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts/acc-1/projects");
        then.status(200).json_body(json!({
            "projects": [{"id": "proj-1", "name": "First Project"}]
        }));
    });
    inaccessible.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts");
        then.status(200).json_body(json!([]));
    });

    let base = temp_dir("adk-rs-init-region-selection");
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env(
            "POLY_ADK_BASE_URL_US_1",
            format!("{}/adk/v1", accessible.base_url()),
        )
        .env(
            "POLY_ADK_BASE_URL_US",
            format!("{}/adk/v1", accessible.base_url()),
        );
    set_non_us_region_base_urls(&mut command, &format!("{}/adk/v1", inaccessible.base_url()));
    command
        .args([
            "init",
            "--base-path",
            base.to_string_lossy().as_ref(),
            "--from-projection",
            "{}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly init");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1\n")
        .expect("write selection");
    let output = child.wait_with_output().expect("wait poly init");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Fetching available regions"));
    assert!(stdout.contains("Auto-selected region us-1."));
    assert!(stdout.contains("Auto-selected account Test Account."));
    assert!(stdout.contains("Project initialized at"));
}

#[test]
fn init_text_project_selection_cancellation_exits_without_error() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/adk/v1/accounts/acc-1/projects");
        then.status(200).json_body(json!({
            "projects": [{"id": "proj-1", "name": "First Project"}]
        }));
    });

    let base = temp_dir("adk-rs-init-cancel");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args([
            "init",
            "--base-path",
            base.to_string_lossy().as_ref(),
            "--region",
            "us-1",
            "--account_id",
            "acc-1",
            "--from-projection",
            "{}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly init");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"\n")
        .expect("write empty selection");
    let output = child.wait_with_output().expect("wait poly init");

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("No project selected. Exiting."));
    assert!(stderr(&output).trim().is_empty());
    assert!(
        !base
            .join("acc-1")
            .join("proj-1")
            .join("project.yaml")
            .exists()
    );
}

#[test]
fn branch_switch_text_prompts_for_branch_when_name_is_omitted() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches");
        then.status(200).json_body(json!({
            "branches": [{"branchId": "BRANCH-1", "name": "feature"}]
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-1/projection");
        then.status(200).json_body(json!({"projection": {}}));
    });

    let project_dir = temp_dir("adk-rs-branch-switch-prompt");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["branch", "switch", "--force", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly branch switch");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"2\n")
        .expect("write selection");
    let output = child.wait_with_output().expect("wait poly branch switch");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Select Branch"));
    assert!(stdout.contains("main (current)"));
    assert!(stdout.contains("feature"));
    assert!(stdout.contains("Command completed."));
    let project_yaml = fs::read_to_string(project_dir.join("project.yaml")).expect("config");
    assert!(project_yaml.contains("branch_id: BRANCH-1"));
}

#[test]
fn branch_delete_text_prompts_for_multiple_branches_and_switches_current_to_main() {
    let server = MockServer::start();
    let branches = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches");
        then.status(200).json_body(json!({
            "branches": [
                {"branchId": "BRANCH-A", "name": "feature-a"},
                {"branchId": "BRANCH-B", "name": "feature-b"}
            ]
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-A/sequence");
        then.status(200)
            .json_body(json!({"lastKnownSequence": "1"}));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-B/sequence");
        then.status(200)
            .json_body(json!({"lastKnownSequence": "2"}));
    });
    let delete_a = server.mock(|when, then| {
        when.method(DELETE)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-A");
        then.status(200).json_body(json!({}));
    });
    let delete_b = server.mock(|when, then| {
        when.method(DELETE)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-B");
        then.status(200).json_body(json!({}));
    });

    let project_dir = temp_dir("adk-rs-branch-delete-prompt");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: BRANCH-A\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["branch", "delete", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly branch delete");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1,2\ny\n")
        .expect("write selections");
    let output = child.wait_with_output().expect("wait poly branch delete");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Select branches to delete"));
    assert!(stdout.contains("feature-a (current)"));
    assert!(stdout.contains("feature-b"));
    assert!(stdout.contains("Deleted branch: feature-a"));
    assert!(stdout.contains("Deleted branch: feature-b"));
    assert!(stdout.contains("Switched to branch 'main'."));
    assert!(stdout.contains("Deleted 2 branch(es)."));
    assert!(stderr(&output).trim().is_empty());
    let project_yaml = fs::read_to_string(project_dir.join("project.yaml")).expect("config");
    assert!(project_yaml.contains("branch_id: main"));
    branches.assert_calls(4);
    delete_a.assert();
    delete_b.assert();
}

#[test]
fn branch_delete_text_reports_no_deletable_branches() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches");
        then.status(200).json_body(json!({"branches": []}));
    });

    let project_dir = temp_dir("adk-rs-branch-delete-none");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["branch", "delete", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().expect("run poly branch delete");

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("No deletable branches found."));
}

#[test]
fn branch_merge_interactive_accepts_auto_merge_and_retries() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches");
        then.status(200).json_body(json!({
            "branches": [{"branchId": "BRANCH-A", "name": "feature-a"}]
        }));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-A/sequence");
        then.status(200)
            .json_body(json!({"lastKnownSequence": "7"}));
    });
    let resolved_merge = server.mock(|when, then| {
        when.method(POST)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-A/merge")
            .body_includes("conflictResolutions");
        then.status(200).json_body(json!({"sequence": "8"}));
    });
    let blocked_merge = server.mock(|when, then| {
        when.method(POST)
            .path("/adk/v1/accounts/test/projects/proj/branches/BRANCH-A/merge")
            .body_excludes("conflictResolutions");
        then.status(400).json_body(json!({
            "hasConflicts": true,
            "conflicts": [
                {
                    "path": ["knowledgeBase", "topics", "entities", "topic-1", "actions"],
                    "baseValue": "base actions",
                    "oursValue": "base actions",
                    "theirsValue": "branch actions",
                    "type": "modify"
                },
                {
                    "path": ["knowledgeBase", "topics", "entities", "topic-1", "updatedAt"],
                    "baseValue": "2026-01-01T00:00:00Z",
                    "oursValue": "2026-01-02T00:00:00Z",
                    "theirsValue": "2026-01-03T00:00:00Z",
                    "type": "modify"
                }
            ],
            "errors": []
        }));
    });

    let project_dir = temp_dir("adk-rs-branch-merge-interactive");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: BRANCH-A\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["branch", "merge", "--interactive", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .arg("Merge feature branch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn poly branch merge");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"1\n")
        .expect("write resolution selection");
    let output = child.wait_with_output().expect("wait poly branch merge");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    let stderr = stderr(&output);
    assert!(stderr.contains("Failed to merge branch 'feature-a'."));
    assert!(stdout.contains("Merge conflicts"));
    assert!(stdout.contains("Resolve conflict"));
    assert!(stdout.contains("Accept auto-merge"));
    assert!(stdout.contains("Branch 'feature-a' merged successfully."));
    assert!(stdout.contains("Switched to \"main\" branch after merge."));
    let project_yaml = fs::read_to_string(project_dir.join("project.yaml")).expect("config");
    assert!(project_yaml.contains("branch_id: main"));
    blocked_merge.assert();
    resolved_merge.assert();
}

#[test]
fn chat_text_echoes_scripted_turns_and_ends_session() {
    let server = MockServer::start();
    let create = server.mock(|when, then| {
        when.method(POST)
            .path("/adk/v1/accounts/test/projects/proj/chat");
        then.status(200).json_body(json!({
            "conversation_id": "conv-1",
            "response": "Hello from Agent Studio",
            "conversation_ended": false
        }));
    });
    let send = server.mock(|when, then| {
        when.method(POST)
            .path("/adk/v1/accounts/test/projects/proj/chat/conv-1")
            .body_includes("Hello");
        then.status(200).json_body(json!({
            "response": "Welcome back",
            "conversation_ended": false
        }));
    });
    let end = server.mock(|when, then| {
        when.method(POST)
            .path("/adk/v1/accounts/test/projects/proj/chat/conv-1/end");
        then.status(200).json_body(json!({"success": true}));
    });

    let project_dir = temp_dir("adk-rs-chat-human-scripted");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());
    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["chat", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .args(["-m", "Hello"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().expect("run poly chat");

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Using sandbox environment for the main branch."));
    assert!(stdout.contains("Chat session started."));
    assert!(stdout.contains("Call Link:"));
    assert!(stdout.contains("Agent: Hello from Agent Studio"));
    assert!(stdout.contains("You: Hello"));
    assert!(stdout.contains("Agent: Welcome back"));
    assert!(stdout.contains("Chat session ended (conversation: conv-1)."));
    assert!(stderr(&output).trim().is_empty());
    create.assert();
    send.assert();
    end.assert();
}

#[test]
fn chat_json_restart_collects_multiple_conversations() {
    let project_dir = make_temp_project_dir("adk-rs-chat-json-restart");

    let output = run_poly(&[
        "chat",
        "--json",
        "--path",
        &project_dir,
        "-m",
        "Hello",
        "-m",
        "/restart",
        "-m",
        "Again",
    ]);

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let conversations = payload
        .get("conversations")
        .and_then(serde_json::Value::as_array)
        .expect("conversations array");
    assert_eq!(conversations.len(), 2);
    assert_eq!(
        conversations[0]
            .get("turns")
            .and_then(serde_json::Value::as_array)
            .and_then(|turns| turns.get(1))
            .and_then(|turn| turn.get("input"))
            .and_then(serde_json::Value::as_str),
        Some("Hello")
    );
    assert_eq!(
        conversations[1]
            .get("turns")
            .and_then(serde_json::Value::as_array)
            .and_then(|turns| turns.get(1))
            .and_then(|turn| turn.get("input"))
            .and_then(serde_json::Value::as_str),
        Some("Again")
    );
}

#[test]
fn deployments_list_text_honors_details_flag() {
    let server = MockServer::start();
    let deployments = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments");
        then.status(200).json_body(json!({
            "deployments": [
                {
                    "id": "dep-1",
                    "version_hash": "abcdef123456",
                    "created_at": "2026-05-01T12:00:00Z",
                    "created_by": "tester@example.com",
                    "artifact_version": "artifact-1",
                    "function_deployment_version": "lambda-1",
                    "client_env": "sandbox",
                    "deployment_metadata": {
                        "deployment_type": "manual",
                        "deployment_message": "Initial deploy"
                    }
                }
            ]
        }));
    });
    let active = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments/active");
        then.status(200).json_body(json!({
            "sandbox": {"version_hash": "abcdef123456"}
        }));
    });

    let project_dir = temp_dir("adk-rs-deployments-details");
    fs::create_dir_all(&project_dir).expect("create project dir");
    fs::write(
        project_dir.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write config");
    let base_url = format!("{}/adk/v1", server.base_url());

    let mut compact = poly_without_fallback_command();
    compact
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["deployments", "list", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let compact_output = compact.output().expect("run compact deployments");

    assert_eq!(compact_output.status.code(), Some(0));
    let compact_stdout = stdout(&compact_output);
    assert!(compact_stdout.contains("abcdef123"));
    assert!(compact_stdout.contains("sandbox"));
    assert!(!compact_stdout.contains("Deployment ID:"));

    let mut detailed = poly_without_fallback_command();
    detailed
        .env("POLY_ADK_KEY", "test-key")
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(["deployments", "list", "--details", "--path"])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let detailed_output = detailed.output().expect("run detailed deployments");

    assert_eq!(detailed_output.status.code(), Some(0));
    let detailed_stdout = stdout(&detailed_output);
    assert!(detailed_stdout.contains("Deployment ID: dep-1"));
    assert!(detailed_stdout.contains("Artifact Version: artifact-1"));
    assert!(detailed_stdout.contains("Lambda Deployment Version: lambda-1"));
    assert!(detailed_stdout.contains("Message: Initial deploy"));
    deployments.assert_calls(2);
    active.assert_calls(2);
}

#[test]
fn deployments_show_json_resolves_included_deployments_from_sandbox_history() {
    let server = MockServer::start();
    let live = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments")
            .query_param("client_env", "live");
        then.status(200).json_body(json!({
            "deployments": [
                deployment_json("dep-live-0", "hash00000xxxx", "live", "live promotion"),
                deployment_json("dep-live-3", "hash00003xxxx", "live", "older live")
            ]
        }));
    });
    let sandbox = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments")
            .query_param("client_env", "sandbox");
        then.status(200).json_body(json!({
            "deployments": [
                deployment_json("dep-0", "hash00000xxxx", "sandbox", "newest"),
                deployment_json("dep-1", "hash00001xxxx", "sandbox", "middle"),
                deployment_json("dep-2", "hash00002xxxx", "sandbox", "middle older"),
                deployment_json("dep-3", "hash00003xxxx", "sandbox", "oldest")
            ]
        }));
    });
    let active = mock_active_deployments(
        &server,
        json!({
            "sandbox": {"version_hash": "hash00000xxxx"},
            "live": {"version_hash": "hash00000xxxx"}
        }),
    );
    let project_dir = write_us_project("adk-rs-deployments-show-json");

    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env(
            "POLY_ADK_BASE_URL_US",
            format!("{}/adk/v1", server.base_url()),
        )
        .env(
            "POLY_ADK_BASE_URL_US_1",
            format!("{}/adk/v1", server.base_url()),
        )
        .args([
            "deployments",
            "show",
            "hash00000xxxx",
            "--env",
            "live",
            "--json",
            "--path",
        ])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().expect("run deployments show");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("json deployments show output");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["deployment"]["id"], "dep-live-0");
    assert_eq!(payload["is_rollback"], false);
    let included = payload["included_deployments"]
        .as_array()
        .expect("included deployments");
    assert_eq!(included.len(), 3);
    assert_eq!(included[0]["id"], "dep-0");
    assert_eq!(included[2]["id"], "dep-2");
    live.assert();
    sandbox.assert();
    active.assert_calls(2);
}

#[test]
fn deployments_promote_json_posts_to_platform_root_endpoint() {
    let server = MockServer::start();
    let deployments = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments")
            .query_param("client_env", "sandbox");
        then.status(200).json_body(json!({
            "deployments": [
                deployment_json("dep-1", "abc123456xyz", "sandbox", "original message"),
                deployment_json("dep-2", "def789012xyz", "sandbox", "previous message")
            ]
        }));
    });
    let active = mock_active_deployments(
        &server,
        json!({
            "sandbox": {"version_hash": "abc123456xyz"},
            "pre-release": {"version_hash": "def789012xyz"}
        }),
    );
    let promote = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/agents/proj/deployments/dep-1/promote")
            .json_body(json!({
                "targetEnvironment": "pre-release",
                "deploymentMessage": "Release notes"
            }));
        then.status(200).json_body(json!({"ok": true}));
    });
    let project_dir = write_us_project("adk-rs-deployments-promote-json");

    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env(
            "POLY_ADK_BASE_URL_US",
            format!("{}/adk/v1", server.base_url()),
        )
        .env(
            "POLY_ADK_BASE_URL_US_1",
            format!("{}/adk/v1", server.base_url()),
        )
        .args([
            "deployments",
            "promote",
            "--from",
            "sandbox",
            "--to",
            "pre-release",
            "--message",
            "Release notes",
            "--json",
            "--force",
            "--path",
        ])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().expect("run deployments promote");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("json deployments promote output");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["to_env"], "pre-release");
    assert_eq!(payload["from_hash"], "abc123456xyz");
    assert_eq!(payload["message"], "Release notes");
    assert_eq!(payload["included_deployments"][0]["id"], "dep-1");
    deployments.assert();
    active.assert();
    promote.assert();
}

#[test]
fn deployments_rollback_json_dry_run_matches_python_contract() {
    let server = MockServer::start();
    let deployments = server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test/projects/proj/deployments")
            .query_param("client_env", "sandbox");
        then.status(200).json_body(json!({
            "deployments": [
                deployment_json("dep-current", "abc123456xyz", "sandbox", "current"),
                deployment_json("dep-target", "def789012xyz", "sandbox", "target"),
                deployment_json("dep-old", "ghi345678xyz", "sandbox", "old")
            ]
        }));
    });
    let active = mock_active_deployments(
        &server,
        json!({
            "sandbox": {"version_hash": "abc123456xyz"}
        }),
    );
    let project_dir = write_us_project("adk-rs-deployments-rollback-dry-run");

    let mut command = poly_without_fallback_command();
    command
        .env("POLY_ADK_KEY", "test-key")
        .env(
            "POLY_ADK_BASE_URL_US",
            format!("{}/adk/v1", server.base_url()),
        )
        .env(
            "POLY_ADK_BASE_URL_US_1",
            format!("{}/adk/v1", server.base_url()),
        )
        .args([
            "deployments",
            "rollback",
            "--to",
            "def789012",
            "--dry-run",
            "--json",
            "--path",
        ])
        .arg(project_dir.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().expect("run deployments rollback");

    assert_eq!(output.status.code(), Some(0));
    let payload: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("json deployments rollback output");
    assert_eq!(payload["success"], false);
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["target_hash"], "def789012xyz");
    assert_eq!(payload["message"], "target");
    assert_eq!(payload["reverted_deployments"][0]["id"], "dep-current");
    deployments.assert();
    active.assert();
}

#[test]
fn verbose_flag_controls_human_error_tracebacks() {
    let missing_path = temp_dir("adk-rs-missing-project");
    let missing = missing_path.to_string_lossy().to_string();

    let concise = run_poly(&["status", "--path", &missing]);
    assert_eq!(concise.status.code(), Some(1));
    let concise_stderr = stderr(&concise);
    assert!(concise_stderr.contains("Run with --verbose for the full traceback."));
    assert!(!concise_stderr.contains("Traceback (most recent call last)"));

    let verbose = run_poly(&["status", "--verbose", "--path", &missing]);
    assert_eq!(verbose.status.code(), Some(1));
    let verbose_stderr = stderr(&verbose);
    assert!(verbose_stderr.contains("Traceback (most recent call last)"));
    assert!(verbose_stderr.contains("No project configuration found."));
    assert!(!verbose_stderr.contains("Run with --verbose for the full traceback."));
}

#[test]
fn debug_flag_enables_debug_logging() {
    let project_dir = make_temp_project_dir("adk-rs-debug-logging");

    let normal = run_poly(&["branch", "current", "--path", &project_dir]);
    assert_eq!(normal.status.code(), Some(0));
    assert!(!stderr(&normal).contains("debug logging enabled"));

    let debug = run_poly(&["branch", "current", "--debug", "--path", &project_dir]);
    assert_eq!(debug.status.code(), Some(0));
    assert!(stderr(&debug).contains("debug logging enabled"));
}

fn set_non_us_region_base_urls(command: &mut Command, base_url: &str) {
    for name in [
        "POLY_ADK_BASE_URL_EUW_1",
        "POLY_ADK_BASE_URL_EU",
        "POLY_ADK_BASE_URL_UK_1",
        "POLY_ADK_BASE_URL_STUDIO",
        "POLY_ADK_BASE_URL_STAGING",
        "POLY_ADK_BASE_URL_DEV",
        "POLY_ADK_BASE_URL",
    ] {
        command.env(name, base_url);
    }
}

#[test]
fn status_text_prints_no_changes_summary() {
    let project_dir = make_temp_project_dir("adk-rs-human-output");

    let output = run_poly(&["status", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("No changes detected."));
    assert_not_json_object(&stdout);
    assert!(!stdout.contains("StatusSummary"));
    assert!(!stdout.contains("files_with_conflicts"));
}

#[test]
fn status_text_summarizes_changed_files_without_rust_debug_dump() {
    let project_dir = make_temp_project_dir("adk-rs-human-output");
    let root = PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("topics")).expect("create topics dir");
    fs::write(
        root.join("topics/new_topic.yaml"),
        "name: New Topic\ncontent: Hello from the human output test\n",
    )
    .expect("write topic");

    let output = run_poly(&["status", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("New files:"));
    assert!(stdout.contains("topics/new_topic.yaml"));
    assert_not_json_object(&stdout);
    assert!(!stdout.contains("StatusSummary"));
    assert!(!stdout.contains("files_with_conflicts"));
}

#[test]
fn branch_list_text_prints_human_table() {
    let project_dir = make_temp_project_dir("adk-rs-human-output");

    let output = run_poly(&["branch", "list", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout(&output);
    assert!(stdout.contains("Branches:"));
    assert!(stdout.contains("* main (main)"));
    assert_not_json_object(&stdout);
    assert!(!stdout.contains("IndexMap"));
    assert!(!stdout.contains("branch_id"));
}

#[test]
fn validate_text_reports_semantic_errors_on_stderr() {
    let project_dir = make_temp_project_dir("adk-rs-human-output");
    let root = PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("config")).expect("create config dir");
    fs::write(
        root.join("config/api_integrations.yaml"),
        "api_integrations:\n  - name: Bad-Name\n",
    )
    .expect("write invalid API integration");

    let output = run_poly(&["validate", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(0));
    let stderr = stderr(&output);
    assert!(stderr.contains("Validation error"));
    assert!(stderr.contains("config/api_integrations.yaml"));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
fn cli_output_paths_do_not_use_debug_formatting() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for file_name in ["main.rs", "console.rs", "docs.rs", "review.rs"] {
        let path = src_dir.join(file_name);
        let source = fs::read_to_string(&path).expect("read CLI source");
        assert!(
            !source.contains("dbg!("),
            "{} should not use dbg! output",
            path.display()
        );
        for (line_number, line) in source.lines().enumerate() {
            let output_or_message_macro =
                ["println!", "eprintln!", "format!", "write!", "writeln!"]
                    .iter()
                    .any(|macro_name| line.contains(macro_name));
            if output_or_message_macro {
                assert!(
                    !line.contains(":?") && !line.contains("#?"),
                    "{}:{} uses Debug formatting in an output path: {}",
                    path.display(),
                    line_number + 1,
                    line.trim()
                );
            }
        }
    }
}

#[test]
fn format_text_check_failure_reports_message_on_stderr() {
    let project_dir = make_temp_project_dir("adk-rs-human-output");
    let root = PathBuf::from(&project_dir);
    fs::create_dir_all(root.join("config")).expect("create config dir");
    fs::write(
        root.join("config/variant_attributes.yaml"),
        "variants: [{name: default, is_default: true}]\n",
    )
    .expect("write unformatted YAML");

    let output = run_poly(&["format", "--check", "--path", &project_dir]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr(&output);
    assert!(stderr.contains("Formatting check failed."));
    assert!(stdout(&output).trim().is_empty());
}
