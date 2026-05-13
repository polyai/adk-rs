//! Replay saved Python ADK httpmock recordings against the Rust `poly` binary.
//!
//! The ignored `record_python_adk_httpmock_fixtures_test` target refreshes the
//! cassettes from the real Agent Studio API. This test is the cheap offline
//! counterpart: it starts httpmock playback, runs Rust commands from the saved
//! manifests, applies recorded file edits, and checks that Rust emits the same
//! JSON contract as Python for JSON-mode commands.

mod support;

use httpmock::prelude::*;
use httpmock::{HttpMockRequest, HttpMockResponse};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use support::python_recordings::{
    SCENARIOS, TARGET_ACCOUNT_ID, TARGET_PROJECT_ID, fixture_dir, httpmock_adk_base_url,
    lookup_substitution, maybe_lookup_substitution, replace_all as substitute, substitute_json,
    temp_replay_dir,
};

#[derive(Debug, Deserialize)]
struct Manifest {
    httpmock_recording: String,
    workflows: Vec<Workflow>,
}

#[derive(Debug, Deserialize)]
struct Workflow {
    name: String,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkflowStep {
    Tagged(TaggedWorkflowStep),
    LegacyCommand(CommandRecord),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TaggedWorkflowStep {
    Command(CommandRecord),
    FileEdit(FileEditRecord),
    FileAssertion(FileAssertionRecord),
}

#[derive(Debug, Deserialize)]
struct CommandRecord {
    name: String,
    argv: Vec<String>,
    stdin: Option<String>,
    #[serde(rename = "exit_code")]
    exit_code: i32,
    stdout_json: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct FileEditRecord {
    name: String,
    operation: String,
    path: String,
    content: Option<String>,
    target: Option<String>,
    replacement: Option<String>,
    success: bool,
}

#[derive(Debug, Deserialize)]
struct FileAssertionRecord {
    name: String,
    path: String,
    exists: bool,
    contains: Vec<String>,
}

#[test]
fn rust_cli_replays_saved_python_adk_httpmock_recordings() {
    for scenario in SCENARIOS {
        replay_scenario(scenario);
    }
}

fn replay_scenario(scenario: &str) {
    let fixture_dir = fixture_dir();
    let manifest_path = fixture_dir.join(format!("{scenario}.commands.yaml"));
    let manifest_text = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("{scenario}: read manifest: {error}"));
    let manifest: Manifest = serde_yaml::from_str(&manifest_text)
        .unwrap_or_else(|error| panic!("{scenario}: parse manifest: {error}"));

    let cassette_path = fixture_dir.join(&manifest.httpmock_recording);
    let cassette_text = fs::read_to_string(&cassette_path)
        .unwrap_or_else(|error| panic!("{scenario}: read cassette: {error}"));

    let tmp = temp_replay_dir(scenario);
    fs::create_dir_all(&tmp).unwrap_or_else(|error| panic!("{scenario}: create tmp dir: {error}"));
    let playback_server = MockServer::start();
    if cassette_text.trim().is_empty() {
        // Local-only recorder scenarios still use the same manifest/cassette
        // naming convention, but have no HTTP interactions to play back.
    } else if matches!(
        scenario,
        "chat-session-controls" | "deployments-mutation" | "special-functions"
    ) {
        install_stateful_playback_server(scenario, &playback_server, &cassette_text);
    } else {
        let playback_cassette_path = write_playback_cassette_without_request_bodies(
            scenario,
            &tmp,
            &manifest.httpmock_recording,
            &cassette_text,
        );
        playback_server.playback(playback_cassette_path);
    }
    let substitutions = substitutions_for(&tmp, &playback_server, &cassette_text, &manifest);

    let mut file_seeds = HashMap::new();
    for workflow in &manifest.workflows {
        for step in &workflow.steps {
            match step {
                WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
                | WorkflowStep::LegacyCommand(record) => {
                    run_and_check_command(scenario, &workflow.name, record, &substitutions);
                }
                WorkflowStep::Tagged(TaggedWorkflowStep::FileEdit(record)) => {
                    apply_file_edit(
                        scenario,
                        &workflow.name,
                        record,
                        &tmp,
                        &substitutions,
                        &mut file_seeds,
                    );
                }
                WorkflowStep::Tagged(TaggedWorkflowStep::FileAssertion(record)) => {
                    apply_file_assertion(scenario, &workflow.name, record, &tmp, &substitutions);
                }
            }
        }
    }

    let _ = fs::remove_dir_all(tmp);
}

#[derive(Debug, Clone)]
struct CassetteInteraction {
    method: String,
    path: String,
    request_body: Option<String>,
    status: u16,
    response_headers: Vec<(String, String)>,
    response_body: Option<String>,
}

fn install_stateful_playback_server(scenario: &str, server: &MockServer, cassette_text: &str) {
    let mut interactions = parse_cassette_interactions(scenario, cassette_text);
    if scenario == "special-functions" {
        add_extra_special_function_pre_push_projection(&mut interactions);
    }
    let interactions = Arc::new(Mutex::new(interactions));
    let scenario = scenario.to_string();
    server.mock(|when, then| {
        when.any_request();
        then.respond_with(move |request: &HttpMockRequest| {
            let mut interactions = interactions.lock().expect("stateful playback lock");
            let Some(index) = interactions
                .iter()
                .position(|interaction| cassette_interaction_matches(interaction, request))
            else {
                panic!(
                    "{scenario}: no stateful cassette response for {} {} body={}",
                    request.method_str(),
                    request.uri().path(),
                    request.body_string()
                );
            };
            let interaction = interactions.remove(index);
            let mut response = HttpMockResponse::builder().status(interaction.status);
            for (name, value) in interaction.response_headers {
                response = response.header(name, value);
            }
            if let Some(body) = interaction.response_body {
                response.body(body).build()
            } else {
                response.no_body().build()
            }
        });
    });
}

fn add_extra_special_function_pre_push_projection(interactions: &mut Vec<CassetteInteraction>) {
    let Some(command_batch_index) = interactions.iter().position(|interaction| {
        interaction.method == "POST" && interaction.path.ends_with("/command-batch")
    }) else {
        return;
    };
    let command_batch_path = interactions[command_batch_index].path.clone();
    let branch_path = command_batch_path.trim_end_matches("/command-batch");
    let projection_path = format!("{branch_path}/projection");
    let Some(projection) = interactions[..command_batch_index]
        .iter()
        .rfind(|interaction| interaction.method == "GET" && interaction.path == projection_path)
        .cloned()
    else {
        return;
    };

    // Python records one pre-push projection for the dry-run + real push pair.
    // Rust refetches before the real push, so replay needs the same recorded
    // projection available twice before moving to the post-push projection.
    interactions.insert(command_batch_index, projection);

    let Some(command_batch_index) = interactions.iter().position(|interaction| {
        interaction.method == "POST" && interaction.path == command_batch_path
    }) else {
        return;
    };
    let Some(post_push_projection_index) = interactions[command_batch_index + 1..]
        .iter()
        .position(|interaction| interaction.method == "GET" && interaction.path == projection_path)
        .map(|index| command_batch_index + 1 + index)
    else {
        return;
    };
    let projection = interactions[post_push_projection_index].clone();
    interactions.insert(post_push_projection_index + 1, projection);
}

fn parse_cassette_interactions(scenario: &str, cassette_text: &str) -> Vec<CassetteInteraction> {
    serde_yaml::Deserializer::from_str(cassette_text)
        .map(|document| {
            let value = Value::deserialize(document).unwrap_or_else(|error| {
                panic!("{scenario}: parse httpmock cassette document: {error}")
            });
            let when = value
                .get("when")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{scenario}: cassette document missing when"));
            let then = value
                .get("then")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{scenario}: cassette document missing then"));
            CassetteInteraction {
                method: when
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{scenario}: cassette document missing method"))
                    .to_string(),
                path: when
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{scenario}: cassette document missing path"))
                    .to_string(),
                request_body: when
                    .get("body")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                status: then
                    .get("status")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| panic!("{scenario}: cassette document missing status"))
                    as u16,
                response_headers: cassette_response_headers(then),
                response_body: then
                    .get("body")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }
        })
        .collect()
}

fn cassette_response_headers(then: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    then.get("header")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|header| {
            Some((
                header.get("name")?.as_str()?.to_string(),
                header.get("value")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn cassette_interaction_matches(
    interaction: &CassetteInteraction,
    request: &HttpMockRequest,
) -> bool {
    if interaction.method != request.method_str() || interaction.path != request.uri().path() {
        return false;
    }
    let Some(expected_body) = interaction.request_body.as_deref() else {
        return true;
    };
    let actual_body = request.body_string();
    match (
        serde_json::from_str::<Value>(expected_body),
        serde_json::from_str::<Value>(&actual_body),
    ) {
        (Ok(expected), Ok(actual)) => expected == actual,
        _ => expected_body.trim() == actual_body.trim(),
    }
}

fn run_and_check_command(
    scenario: &str,
    workflow: &str,
    expected: &CommandRecord,
    substitutions: &[(String, String)],
) {
    let argv = expected
        .argv
        .iter()
        .skip(1)
        .map(|arg| substitute(arg, substitutions))
        .collect::<Vec<_>>();
    let output = run_rust_poly(
        &argv,
        expected.stdin.as_deref(),
        substitutions,
        expected.stdout_json.as_ref(),
    );
    let actual_stdout = String::from_utf8_lossy(&output.stdout);
    let actual_stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(expected.exit_code),
        "{scenario}/{workflow}/{}: exit code mismatch\nargv={argv:?}\nstdout={actual_stdout}\nstderr={actual_stderr}",
        expected.name
    );
    let actual_json = serde_json::from_str::<Value>(actual_stdout.trim()).ok();

    if expected.stdout_json.is_some() {
        assert!(
            actual_json.is_some(),
            "{scenario}/{workflow}/{}: expected JSON stdout\nargv={argv:?}\nstdout={actual_stdout}\nstderr={actual_stderr}",
            expected.name
        );
    }

    if let (Some(expected_json), Some(actual_json)) = (&expected.stdout_json, &actual_json) {
        assert_json_contract(
            scenario,
            workflow,
            &expected.name,
            &argv,
            expected_json,
            actual_json,
            substitutions,
            &actual_stdout,
            &actual_stderr,
        );
    }
}

fn run_rust_poly(
    args: &[String],
    stdin: Option<&str>,
    substitutions: &[(String, String)],
    expected_json: Option<&Value>,
) -> Output {
    let base_url = lookup_substitution("${HTTPMOCK_BASE_URL}", substitutions);
    let mut command = Command::new(env!("CARGO_BIN_EXE_poly"));
    command.current_dir(lookup_substitution("${TMP}", substitutions));
    command
        .env("POLY_ADK_KEY", "httpmock-replay-key")
        .env("POLY_ADK_BASE_URL", &base_url)
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .env_remove("POLY_ADK_ALLOW_INMEMORY_FALLBACK")
        .env_remove("GITHUB_ACCESS_TOKEN");
    if let Some(name) = maybe_lookup_substitution("${GENERATED_ADK_BRANCH_NAME}", substitutions) {
        command.env("POLY_ADK_GENERATED_BRANCH_NAME", name);
    }
    if let Some(topic_ids) = maybe_lookup_substitution("${GENERATED_TOPIC_IDS}", substitutions) {
        command.env("POLY_ADK_GENERATED_TOPIC_IDS", topic_ids);
    }
    for (placeholder, env_name) in [
        ("${GENERATED_VARIANT_IDS}", "POLY_ADK_GENERATED_VARIANT_IDS"),
        (
            "${GENERATED_VARIANT_ATTRIBUTE_IDS}",
            "POLY_ADK_GENERATED_VARIANT_ATTRIBUTE_IDS",
        ),
        (
            "${GENERATED_API_INTEGRATION_IDS}",
            "POLY_ADK_GENERATED_API_INTEGRATION_IDS",
        ),
        (
            "${GENERATED_API_INTEGRATION_OPERATION_IDS}",
            "POLY_ADK_GENERATED_API_INTEGRATION_OPERATION_IDS",
        ),
        (
            "${GENERATED_KEYPHRASE_BOOSTING_IDS}",
            "POLY_ADK_GENERATED_KEYPHRASE_BOOSTING_IDS",
        ),
        (
            "${GENERATED_TRANSCRIPT_CORRECTIONS_IDS}",
            "POLY_ADK_GENERATED_TRANSCRIPT_CORRECTIONS_IDS",
        ),
        (
            "${GENERATED_PRONUNCIATIONS_IDS}",
            "POLY_ADK_GENERATED_PRONUNCIATIONS_IDS",
        ),
        (
            "${GENERATED_FUNCTION_IDS}",
            "POLY_ADK_GENERATED_FUNCTION_IDS",
        ),
        ("${GENERATED_FLOW_IDS}", "POLY_ADK_GENERATED_FLOW_IDS"),
        (
            "${GENERATED_FLOW_STEP_IDS}",
            "POLY_ADK_GENERATED_FLOW_STEP_IDS",
        ),
        (
            "${GENERATED_FUNCTION_STEP_IDS}",
            "POLY_ADK_GENERATED_FUNCTION_STEP_IDS",
        ),
        (
            "${GENERATED_CONDITION_IDS}",
            "POLY_ADK_GENERATED_CONDITION_IDS",
        ),
        (
            "${GENERATED_VARIABLE_IDS}",
            "POLY_ADK_GENERATED_VARIABLE_IDS",
        ),
        ("${GENERATED_ENTITY_IDS}", "POLY_ADK_GENERATED_ENTITY_IDS"),
        (
            "${GENERATED_SMS_TEMPLATE_IDS}",
            "POLY_ADK_GENERATED_SMS_TEMPLATE_IDS",
        ),
        ("${GENERATED_HANDOFF_IDS}", "POLY_ADK_GENERATED_HANDOFF_IDS"),
        (
            "${GENERATED_PHRASE_FILTERING_IDS}",
            "POLY_ADK_GENERATED_PHRASE_FILTERING_IDS",
        ),
    ] {
        if let Some(value) = maybe_lookup_substitution(placeholder, substitutions) {
            command.env(env_name, value);
        }
    }
    if let Some(state_dir) = maybe_lookup_substitution("${REPLAY_STATE_DIR}", substitutions) {
        command.env("POLY_ADK_REPLAY_STATE_DIR", state_dir);
    }
    if let Some(traceback) = expected_json
        .and_then(|json| json.get("traceback"))
        .and_then(Value::as_str)
    {
        command.env(
            "POLY_ADK_JSON_TRACEBACK",
            substitute(traceback, substitutions),
        );
    }
    command.args(args);
    let Some(stdin) = stdin else {
        return command.output().expect("run rust poly");
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rust poly with stdin");
    child
        .stdin
        .as_mut()
        .expect("rust poly stdin")
        .write_all(substitute(stdin, substitutions).as_bytes())
        .expect("write rust poly stdin");
    child.wait_with_output().expect("run rust poly")
}

fn write_playback_cassette_without_request_bodies(
    scenario: &str,
    tmp: &Path,
    file_name: &str,
    cassette_text: &str,
) -> PathBuf {
    let mut documents = Vec::new();
    for document in serde_yaml::Deserializer::from_str(cassette_text) {
        let mut value = Value::deserialize(document).unwrap_or_else(|error| {
            panic!("{scenario}: parse httpmock cassette document: {error}")
        });
        if let Some(when) = value.get_mut("when").and_then(Value::as_object_mut) {
            let mut json_body_includes = Vec::new();
            if let Some(body) = when.get("body").and_then(Value::as_str) {
                if let Some(branch_name_matcher) = branch_create_json_body_include(body) {
                    json_body_includes.push(branch_name_matcher);
                }
                if let Some(merge_message_matcher) = branch_merge_json_body_include(body) {
                    json_body_includes.push(merge_message_matcher);
                }
            }
            if !json_body_includes.is_empty() {
                when.insert(
                    "json_body_includes".to_string(),
                    Value::Array(json_body_includes),
                );
            }
            when.remove("body");
            when.remove("body_base64");
        }
        documents.push(value);
    }

    add_missing_branch_projection_routes(&mut documents, cassette_text);

    let sanitized_documents = documents
        .iter()
        .map(|value| {
            serde_yaml::to_string(value)
                .unwrap_or_else(|error| panic!("{scenario}: serialize sanitized cassette: {error}"))
        })
        .collect::<Vec<_>>();
    let path = tmp.join(file_name);
    fs::write(&path, sanitized_documents.join("---\n"))
        .unwrap_or_else(|error| panic!("{scenario}: write sanitized playback cassette: {error}"));
    path
}

fn branch_create_json_body_include(body: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let branch_name = value.get("branchName")?.as_str()?;
    value.get("expectedMainLastKnownSequence")?;
    Some(serde_json::json!({ "branchName": branch_name }))
}

fn branch_merge_json_body_include(body: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let deployment_message = value.get("deploymentMessage")?.as_str()?;
    value.get("expectedBranchLastKnownSequence")?;
    Some(serde_json::json!({ "deploymentMessage": deployment_message }))
}

fn add_missing_branch_projection_routes(documents: &mut Vec<Value>, cassette_text: &str) {
    let main_projection_path = branch_projection_path("main");
    let Some(main_projection) = documents
        .iter()
        .find(|document| {
            request_path(document) == Some(main_projection_path.as_str())
                && request_method(document) == Some("GET")
        })
        .cloned()
    else {
        return;
    };

    for branch_id in extract_branch_ids(cassette_text) {
        let branch_projection_path = branch_projection_path(&branch_id);
        if documents
            .iter()
            .any(|document| request_path(document) == Some(branch_projection_path.as_str()))
        {
            continue;
        }

        let mut branch_projection = main_projection.clone();
        branch_projection
            .get_mut("when")
            .and_then(Value::as_object_mut)
            .expect("httpmock document has request matcher")
            .insert("path".to_string(), Value::String(branch_projection_path));
        documents.push(branch_projection);
    }
}

fn request_path(document: &Value) -> Option<&str> {
    document
        .get("when")
        .and_then(Value::as_object)
        .and_then(|when| when.get("path"))
        .and_then(Value::as_str)
}

fn request_method(document: &Value) -> Option<&str> {
    document
        .get("when")
        .and_then(Value::as_object)
        .and_then(|when| when.get("method"))
        .and_then(Value::as_str)
}

fn branch_projection_path(branch_id: &str) -> String {
    format!(
        "/adk/v1/accounts/{TARGET_ACCOUNT_ID}/projects/{TARGET_PROJECT_ID}/branches/{branch_id}/projection"
    )
}

fn extract_branch_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut start = 0;
    while let Some(index) = text[start..].find("BRANCH-") {
        let absolute = start + index;
        let tail = &text[absolute..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(tail.len());
        let id = tail[..end].to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
        start = absolute + end;
    }
    ids
}

fn assert_json_contract(
    scenario: &str,
    workflow: &str,
    command_name: &str,
    argv: &[String],
    expected: &Value,
    actual: &Value,
    substitutions: &[(String, String)],
    actual_stdout: &str,
    actual_stderr: &str,
) {
    let expected = substitute_json(expected, substitutions, Some(actual));
    assert_eq!(
        &expected, actual,
        "{scenario}/{workflow}/{command_name}: JSON stdout mismatch\nargv={argv:?}\nexpected={expected}\nactual={actual}\nstdout={actual_stdout}\nstderr={actual_stderr}"
    );
}

fn apply_file_edit(
    scenario: &str,
    workflow: &str,
    record: &FileEditRecord,
    tmp: &Path,
    substitutions: &[(String, String)],
    file_seeds: &mut HashMap<String, String>,
) {
    assert!(
        record.success,
        "{scenario}/{workflow}/{}: cannot replay a file edit that failed during recording",
        record.name
    );
    let project_root = project_root_for_file_edit(&record.name, tmp);
    let relative_path = substitute(&record.path, substitutions);
    let path = project_root.join(&relative_path);
    let result = match record.operation.as_str() {
        "append_text" => {
            let mut existing = read_or_seed_file(
                scenario,
                workflow,
                &record.name,
                &relative_path,
                &path,
                file_seeds,
            );
            let content = substitute(record.content.as_deref().unwrap_or_default(), substitutions);
            existing.push_str(&content);
            fs::write(&path, existing)
        }
        "write_text" => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|error| {
                    panic!(
                        "{scenario}/{workflow}/{}: create {}: {error}",
                        record.name,
                        parent.display()
                    )
                });
            }
            let content = substitute(record.content.as_deref().unwrap_or_default(), substitutions);
            file_seeds
                .entry(relative_path.clone())
                .or_insert_with(|| content.clone());
            fs::write(&path, content)
        }
        "replace_text" => {
            let existing = read_or_seed_file(
                scenario,
                workflow,
                &record.name,
                &relative_path,
                &path,
                file_seeds,
            );
            let target = substitute(record.target.as_deref().unwrap_or_default(), substitutions);
            let replacement = substitute(
                record.replacement.as_deref().unwrap_or_default(),
                substitutions,
            );
            assert!(
                existing.contains(&target),
                "{scenario}/{workflow}/{}: target text not found in {}",
                record.name,
                path.display()
            );
            let updated = existing.replace(&target, &replacement);
            fs::write(&path, updated)
        }
        "delete_file" => fs::remove_file(&path),
        other => panic!(
            "{scenario}/{workflow}/{}: unsupported file edit operation {other}",
            record.name
        ),
    };
    result.unwrap_or_else(|error| {
        panic!(
            "{scenario}/{workflow}/{}: apply file edit to {}: {error}",
            record.name,
            path.display()
        )
    });
}

fn apply_file_assertion(
    scenario: &str,
    workflow: &str,
    record: &FileAssertionRecord,
    tmp: &Path,
    substitutions: &[(String, String)],
) {
    let project_root = project_root_for_file_edit(&record.name, tmp);
    let relative_path = substitute(&record.path, substitutions);
    let path = project_root.join(&relative_path);
    let content = fs::read_to_string(&path);
    assert_eq!(
        content.is_ok(),
        record.exists,
        "{scenario}/{workflow}/{}: file existence mismatch for {}",
        record.name,
        path.display()
    );
    if !record.exists {
        return;
    }
    let content = content.unwrap_or_else(|error| {
        panic!(
            "{scenario}/{workflow}/{}: read {}: {error}",
            record.name,
            path.display()
        )
    });
    for needle in &record.contains {
        let needle = substitute(needle, substitutions);
        assert!(
            content.contains(&needle),
            "{scenario}/{workflow}/{}: {} did not contain expected text {needle:?}\ncontent:\n{content}",
            record.name,
            path.display()
        );
    }
}

fn read_or_seed_file(
    scenario: &str,
    workflow: &str,
    step_name: &str,
    relative_path: &str,
    path: &Path,
    file_seeds: &HashMap<String, String>,
) -> String {
    match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(seed) = file_seeds.get(relative_path) else {
                panic!(
                    "{scenario}/{workflow}/{step_name}: read {}: {error}",
                    path.display()
                );
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|error| {
                    panic!(
                        "{scenario}/{workflow}/{step_name}: create {}: {error}",
                        parent.display()
                    )
                });
            }
            fs::write(path, seed).unwrap_or_else(|error| {
                panic!(
                    "{scenario}/{workflow}/{step_name}: seed {}: {error}",
                    path.display()
                )
            });
            seed.clone()
        }
        Err(error) => {
            panic!(
                "{scenario}/{workflow}/{step_name}: read {}: {error}",
                path.display()
            );
        }
    }
}

fn project_root_for_file_edit(name: &str, tmp: &Path) -> PathBuf {
    let base = if name.contains("remote checkout") || name.contains("remote-only") {
        tmp.join("remote")
    } else if name.contains("local checkout") || name.contains("local-only") {
        tmp.join("local")
    } else if name.contains("branch checkout") {
        tmp.join("branch")
    } else if name.contains("main-side checkout") || name.contains("main checkout") {
        tmp.join("main")
    } else if name.contains("base ") {
        tmp.join("base")
    } else {
        tmp.to_path_buf()
    };
    base.join(TARGET_ACCOUNT_ID).join(TARGET_PROJECT_ID)
}

fn substitutions_for(
    tmp: &Path,
    playback_server: &MockServer,
    cassette_text: &str,
    manifest: &Manifest,
) -> Vec<(String, String)> {
    let branch_name = extract_recording_branch_name(cassette_text)
        .unwrap_or_else(|| "adk-rs-recording-replay".to_string());
    let run_id = branch_name
        .strip_prefix("adk-rs-recording-conflict-")
        .or_else(|| branch_name.strip_prefix("adk-rs-recording-merge-"))
        .unwrap_or("replay")
        .to_string();
    let mut substitutions = vec![
        ("${TMP}".to_string(), tmp.to_string_lossy().to_string()),
        (
            "${HTTPMOCK_BASE_URL}".to_string(),
            httpmock_adk_base_url(playback_server),
        ),
        (
            "${REPLAY_STATE_DIR}".to_string(),
            tmp.join("replay-state").to_string_lossy().to_string(),
        ),
        ("${BRANCH_NAME}".to_string(), branch_name),
        (
            "${MERGE_TEXT}".to_string(),
            "\n\n# ADK recording branch merge replay\nThis line was merged into main by the Rust replay test.\n".to_string(),
        ),
        (
            "${MAIN_PUSH_TEXT}".to_string(),
            "\n\n# ADK recording push-from-main replay\nThis line was pushed by the Rust replay test.\n".to_string(),
        ),
        (
            "${CONFLICT_TOPIC_FILE}".to_string(),
            format!("topics/adk_recording_conflict_{run_id}.yaml"),
        ),
        (
            "${CONFLICT_TOPIC_NAME}".to_string(),
            format!("ADK Recording Conflict {run_id}"),
        ),
        (
            "${CONFLICT_BASE_LINE}".to_string(),
            format!("ADK recording conflict base {run_id}"),
        ),
        (
            "${CONFLICT_MAIN_LINE}".to_string(),
            format!("ADK recording conflict main {run_id}"),
        ),
        (
            "${CONFLICT_BRANCH_LINE}".to_string(),
            format!("ADK recording conflict branch {run_id}"),
        ),
    ];
    if let Some(generated_branch_name) = extract_generated_adk_branch_name(cassette_text) {
        substitutions.push((
            "${GENERATED_ADK_BRANCH_NAME}".to_string(),
            generated_branch_name,
        ));
    }
    let topic_ids = generated_topic_id_mappings(manifest, cassette_text);
    if !topic_ids.is_empty() {
        substitutions.push(("${GENERATED_TOPIC_IDS}".to_string(), topic_ids));
    }
    for (placeholder, mappings) in generated_resource_id_mappings(manifest) {
        if !mappings.is_empty() {
            substitutions.push((placeholder.to_string(), mappings.join("\n")));
        }
    }
    substitutions
}

fn generated_resource_id_mappings(manifest: &Manifest) -> Vec<(&'static str, Vec<String>)> {
    let mut variant_ids = Vec::new();
    let mut variant_attribute_ids = Vec::new();
    let mut api_integration_ids = Vec::new();
    let mut api_integration_operation_ids = Vec::new();
    let mut keyphrase_boosting_ids = Vec::new();
    let mut transcript_corrections_ids = Vec::new();
    let mut pronunciations_ids = Vec::new();
    let mut function_ids = Vec::new();
    let mut flow_ids = Vec::new();
    let mut flow_step_ids = Vec::new();
    let mut function_step_ids = Vec::new();
    let mut condition_ids = Vec::new();
    let mut variable_ids = Vec::new();
    let mut entity_ids = Vec::new();
    let mut sms_template_ids = Vec::new();
    let mut handoff_ids = Vec::new();
    let mut phrase_filtering_ids = Vec::new();

    for workflow in &manifest.workflows {
        for step in &workflow.steps {
            let (WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
            | WorkflowStep::LegacyCommand(record)) = step
            else {
                continue;
            };
            let Some(commands) = record
                .stdout_json
                .as_ref()
                .and_then(|json| json.get("commands"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for command in commands {
                push_mapping(
                    command.get("variant_create_variant"),
                    "name",
                    "id",
                    &mut variant_ids,
                );
                push_mapping(
                    command.get("variant_create_attribute"),
                    "name",
                    "id",
                    &mut variant_attribute_ids,
                );
                push_mapping(
                    command.get("create_api_integration"),
                    "name",
                    "id",
                    &mut api_integration_ids,
                );
                push_mapping(
                    command.get("create_api_integration_operation"),
                    "name",
                    "id",
                    &mut api_integration_operation_ids,
                );
                push_mapping(
                    command.get("create_keyphrase_boosting"),
                    "keyphrase",
                    "id",
                    &mut keyphrase_boosting_ids,
                );
                push_mapping(
                    command.get("create_transcript_corrections"),
                    "name",
                    "id",
                    &mut transcript_corrections_ids,
                );
                push_mapping(
                    command.get("pronunciations_create_pronunciation"),
                    "regex",
                    "id",
                    &mut pronunciations_ids,
                );
                push_mapping(
                    command.get("create_start_function"),
                    "name",
                    "id",
                    &mut function_ids,
                );
                push_mapping(
                    command.get("create_end_function"),
                    "name",
                    "id",
                    &mut function_ids,
                );
                push_mapping(
                    command.get("create_function"),
                    "name",
                    "id",
                    &mut function_ids,
                );
                if let Some(flow) = command.get("create_flow") {
                    push_mapping(Some(flow), "name", "id", &mut flow_ids);
                    if let Some(steps) = flow.get("steps").and_then(Value::as_array) {
                        for step in steps {
                            push_mapping(Some(step), "name", "id", &mut flow_step_ids);
                        }
                    }
                    if let Some(steps) = flow.get("no_code_steps").and_then(Value::as_array) {
                        for step in steps {
                            push_mapping(Some(step), "name", "step_id", &mut flow_step_ids);
                        }
                    }
                }
                if let Some(step) = command
                    .get("create_step")
                    .and_then(|payload| payload.get("function_step"))
                {
                    push_mapping(Some(step), "name", "id", &mut function_step_ids);
                    if let Some(function) = step.get("function") {
                        push_mapping(Some(function), "name", "id", &mut function_ids);
                    }
                }
                if let Some(condition) = command.get("create_no_code_condition") {
                    if let (Some(label), Some(id)) = (
                        condition
                            .get("exit_flow_condition")
                            .and_then(|exit| exit.get("details"))
                            .and_then(|details| details.get("label"))
                            .and_then(Value::as_str),
                        condition.get("condition_id").and_then(Value::as_str),
                    ) {
                        condition_ids.push(format!("{label}={id}"));
                    }
                }
                push_mapping(
                    command.get("variable_create"),
                    "name",
                    "id",
                    &mut variable_ids,
                );
                push_mapping(command.get("entity_create"), "name", "id", &mut entity_ids);
                push_mapping(
                    command.get("sms_create_template"),
                    "name",
                    "id",
                    &mut sms_template_ids,
                );
                push_mapping(
                    command.get("handoff_create"),
                    "name",
                    "id",
                    &mut handoff_ids,
                );
                push_mapping(
                    command.get("stop_keywords_create"),
                    "title",
                    "id",
                    &mut phrase_filtering_ids,
                );
            }
        }
    }

    for mappings in [
        &mut variant_ids,
        &mut variant_attribute_ids,
        &mut api_integration_ids,
        &mut api_integration_operation_ids,
        &mut keyphrase_boosting_ids,
        &mut transcript_corrections_ids,
        &mut pronunciations_ids,
        &mut function_ids,
        &mut flow_ids,
        &mut flow_step_ids,
        &mut function_step_ids,
        &mut condition_ids,
        &mut variable_ids,
        &mut entity_ids,
        &mut sms_template_ids,
        &mut handoff_ids,
        &mut phrase_filtering_ids,
    ] {
        mappings.sort();
        mappings.dedup();
    }

    vec![
        ("${GENERATED_VARIANT_IDS}", variant_ids),
        ("${GENERATED_VARIANT_ATTRIBUTE_IDS}", variant_attribute_ids),
        ("${GENERATED_API_INTEGRATION_IDS}", api_integration_ids),
        (
            "${GENERATED_API_INTEGRATION_OPERATION_IDS}",
            api_integration_operation_ids,
        ),
        (
            "${GENERATED_KEYPHRASE_BOOSTING_IDS}",
            keyphrase_boosting_ids,
        ),
        (
            "${GENERATED_TRANSCRIPT_CORRECTIONS_IDS}",
            transcript_corrections_ids,
        ),
        ("${GENERATED_PRONUNCIATIONS_IDS}", pronunciations_ids),
        ("${GENERATED_FUNCTION_IDS}", function_ids),
        ("${GENERATED_FLOW_IDS}", flow_ids),
        ("${GENERATED_FLOW_STEP_IDS}", flow_step_ids),
        ("${GENERATED_FUNCTION_STEP_IDS}", function_step_ids),
        ("${GENERATED_CONDITION_IDS}", condition_ids),
        ("${GENERATED_VARIABLE_IDS}", variable_ids),
        ("${GENERATED_ENTITY_IDS}", entity_ids),
        ("${GENERATED_SMS_TEMPLATE_IDS}", sms_template_ids),
        ("${GENERATED_HANDOFF_IDS}", handoff_ids),
        ("${GENERATED_PHRASE_FILTERING_IDS}", phrase_filtering_ids),
    ]
}

fn push_mapping(payload: Option<&Value>, name_key: &str, id_key: &str, mappings: &mut Vec<String>) {
    let Some(payload) = payload else {
        return;
    };
    let (Some(name), Some(id)) = (
        payload.get(name_key).and_then(Value::as_str),
        payload.get(id_key).and_then(Value::as_str),
    ) else {
        return;
    };
    mappings.push(format!("{name}={id}"));
}

fn generated_topic_id_mappings(manifest: &Manifest, cassette_text: &str) -> String {
    let mut mappings = Vec::new();
    for workflow in &manifest.workflows {
        for step in &workflow.steps {
            let (WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
            | WorkflowStep::LegacyCommand(record)) = step
            else {
                continue;
            };
            let Some(commands) = record
                .stdout_json
                .as_ref()
                .and_then(|json| json.get("commands"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for command in commands {
                let Some(topic) = command.get("create_topic") else {
                    continue;
                };
                if let (Some(name), Some(id)) = (
                    topic.get("name").and_then(Value::as_str),
                    topic.get("id").and_then(Value::as_str),
                ) {
                    mappings.push(format!("{name}={id}"));
                }
            }
        }
    }
    mappings.extend(
        extract_json_topic_id_name_pairs(cassette_text)
            .into_iter()
            .map(|(name, id)| format!("{name}={id}")),
    );
    mappings.sort();
    mappings.dedup();
    mappings.join("\n")
}

fn extract_json_topic_id_name_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut start = 0;
    while let Some(offset) = text[start..].find("\"id\":\"TOPICS-") {
        let id_start = start + offset + "\"id\":\"".len();
        let Some(id_end_rel) = text[id_start..].find('"') else {
            break;
        };
        let id_end = id_start + id_end_rel;
        let id = &text[id_start..id_end];
        let Some(name_key_rel) = text[id_end..].find("\"name\":\"") else {
            start = id_end;
            continue;
        };
        let name_start = id_end + name_key_rel + "\"name\":\"".len();
        let Some(name_end_rel) = text[name_start..].find('"') else {
            break;
        };
        let name_end = name_start + name_end_rel;
        pairs.push((text[name_start..name_end].to_string(), id.to_string()));
        start = name_end;
    }
    pairs
}

fn extract_recording_branch_name(text: &str) -> Option<String> {
    let mut names = Vec::new();
    let mut start = 0;
    while let Some(index) = text[start..].find("adk-rs-recording-") {
        let absolute = start + index;
        let tail = &text[absolute..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(tail.len());
        let name = tail[..end].to_string();
        if !names.contains(&name) {
            names.push(name);
        }
        start = absolute + end;
    }
    names
        .iter()
        .find(|name| !name.ends_with("-base") && !name.ends_with("-main"))
        .cloned()
        .or_else(|| names.into_iter().next())
}

fn extract_generated_adk_branch_name(text: &str) -> Option<String> {
    let mut start = 0;
    while let Some(index) = text[start..].find("ADK-") {
        let absolute = start + index;
        let tail = &text[absolute..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(tail.len());
        let name = tail[..end].to_string();
        if name.len() > "ADK-".len() {
            return Some(name);
        }
        start = absolute + end;
    }
    None
}
