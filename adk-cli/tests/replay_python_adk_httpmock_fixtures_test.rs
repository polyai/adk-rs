//! Replay saved Python ADK httpmock recordings against the Rust `poly` binary.
//!
//! The ignored `record_python_adk_httpmock_fixtures_test` target refreshes the
//! cassettes from the real Agent Studio API. This test is the cheap offline
//! counterpart: it starts httpmock playback, runs Rust commands from the saved
//! manifests, applies recorded file edits, and checks that Rust emits the same
//! JSON contract as Python for JSON-mode commands.

mod support;

use httpmock::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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
}

#[derive(Debug, Deserialize)]
struct CommandRecord {
    name: String,
    argv: Vec<String>,
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
    let playback_cassette_path = write_playback_cassette_without_request_bodies(
        scenario,
        &tmp,
        &manifest.httpmock_recording,
        &cassette_text,
    );

    let playback_server = MockServer::start();
    playback_server.playback(playback_cassette_path);
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
            }
        }
    }

    let _ = fs::remove_dir_all(tmp);
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
    let output = run_rust_poly(&argv, substitutions, expected.stdout_json.as_ref());
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
    substitutions: &[(String, String)],
    expected_json: Option<&Value>,
) -> Output {
    let base_url = lookup_substitution("${HTTPMOCK_BASE_URL}", substitutions);
    let mut command = Command::new(env!("CARGO_BIN_EXE_poly"));
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
    command.args(args).output().expect("run rust poly")
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
    substitutions
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
