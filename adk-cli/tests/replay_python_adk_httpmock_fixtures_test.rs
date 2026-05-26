//! Replay saved ADK recordings against Python ADK.
//!
//! The ignored `record_python_adk_from_manifest_test` target refreshes the
//! cassettes from the real Agent Studio API. The default test replays those
//! cassettes against Rust. The ignored Python replay is an opt-in sanity check
//! that runs Python ADK against the saved cassettes without contacting Agent
//! Studio or rewriting fixtures.

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
    lookup_substitution, maybe_lookup_substitution, python_adk_bin, replace_all as substitute,
    substitute_json, temp_replay_dir,
};
use support::recording_manifest::{
    CommandRecord, FileAssertionRecord, FileEditRecord, Manifest, TaggedWorkflowStep, WorkflowStep,
};

#[test]
fn rust_cli_replays_saved_python_adk_httpmock_recordings() {
    for scenario in SCENARIOS {
        replay_scenario(scenario, ReplaySubject::Rust);
    }
}

#[test]
#[ignore = "runs Python ADK against saved httpmock cassettes without rewriting fixtures"]
fn python_adk_replays_saved_python_adk_httpmock_recordings() {
    for scenario in selected_python_replay_scenarios() {
        replay_scenario(&scenario, ReplaySubject::Python);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaySubject {
    Rust,
    Python,
}

#[derive(Debug)]
struct ReplayFixturePaths {
    manifest: PathBuf,
    cassette: PathBuf,
}

impl ReplayFixturePaths {
    fn diagnostic_lines(&self) -> String {
        format!(
            "manifest={}\nhttpmock={}",
            self.manifest.display(),
            self.cassette.display()
        )
    }
}

fn selected_python_replay_scenarios() -> Vec<String> {
    match std::env::var("PYTHON_ADK_REPLAY_SCENARIO") {
        Ok(name) if !name.trim().is_empty() => vec![name],
        _ => SCENARIOS
            .iter()
            .map(|scenario| scenario.to_string())
            .collect(),
    }
}

fn replay_scenario(scenario: &str, subject: ReplaySubject) {
    let fixture_dir = fixture_dir();
    let manifest_path = fixture_dir.join(format!("{scenario}.commands.yaml"));
    let manifest_text = fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "{scenario}: read manifest {}: {error}",
            manifest_path.display()
        )
    });
    let manifest: Manifest = serde_yaml::from_str(&manifest_text).unwrap_or_else(|error| {
        panic!(
            "{scenario}: parse manifest {}: {error}",
            manifest_path.display()
        )
    });

    let cassette_path = fixture_dir.join(&manifest.httpmock_recording);
    let fixture_paths = ReplayFixturePaths {
        manifest: manifest_path,
        cassette: cassette_path,
    };
    let cassette_text = fs::read_to_string(&fixture_paths.cassette).unwrap_or_else(|error| {
        panic!(
            "{scenario}: read cassette {}: {error}",
            fixture_paths.cassette.display()
        )
    });

    let tmp = temp_replay_dir(scenario);
    fs::create_dir_all(&tmp).unwrap_or_else(|error| panic!("{scenario}: create tmp dir: {error}"));
    let playback_server = MockServer::start();
    if subject == ReplaySubject::Python {
        install_python_playback_server(scenario, &playback_server, &cassette_text);
    } else if cassette_text.trim().is_empty() {
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
    let substitutions =
        substitutions_for(scenario, &tmp, &playback_server, &cassette_text, &manifest);

    let mut file_seeds = HashMap::new();
    for workflow in &manifest.workflows {
        for step in &workflow.steps {
            match step {
                WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
                | WorkflowStep::LegacyCommand(record) => {
                    run_and_check_command(
                        scenario,
                        &workflow.name,
                        record,
                        &substitutions,
                        subject,
                        &fixture_paths,
                    );
                }
                WorkflowStep::Tagged(TaggedWorkflowStep::FileEdit(record)) => {
                    apply_file_edit(
                        scenario,
                        &workflow.name,
                        record,
                        &tmp,
                        &substitutions,
                        &mut file_seeds,
                        &fixture_paths,
                    );
                }
                WorkflowStep::Tagged(TaggedWorkflowStep::FileAssertion(record)) => {
                    apply_file_assertion(
                        scenario,
                        &workflow.name,
                        record,
                        &tmp,
                        &substitutions,
                        &fixture_paths,
                    );
                }
            }
        }
    }

    let _ = fs::remove_dir_all(tmp);
}

fn install_python_playback_server(scenario: &str, server: &MockServer, cassette_text: &str) {
    if cassette_text.trim().is_empty() {
        return;
    }
    install_stateful_playback_server_with_interactions(
        scenario,
        server,
        parse_cassette_interactions(scenario, cassette_text),
    );
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
    install_stateful_playback_server_with_interactions(scenario, server, interactions);
}

fn install_stateful_playback_server_with_interactions(
    scenario: &str,
    server: &MockServer,
    interactions: Vec<CassetteInteraction>,
) {
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
    if interaction.method != request.method_str()
        || !request_path_matches(&interaction.path, request.uri().path())
    {
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
        (Ok(expected), Ok(actual)) => normalize_generated_resource_ids(expected, &actual) == actual,
        _ => expected_body.trim() == actual_body.trim(),
    }
}

fn normalize_generated_resource_ids(expected: Value, actual: &Value) -> Value {
    let mut replacements = HashMap::new();
    collect_generated_resource_id_replacements(&expected, actual, None, &mut replacements);
    let mut normalized = expected;
    apply_generated_resource_id_replacements(&mut normalized, &replacements);
    normalized
}

fn collect_generated_resource_id_replacements(
    expected: &Value,
    actual: &Value,
    field_name: Option<&str>,
    replacements: &mut HashMap<String, String>,
) {
    match (expected, actual) {
        (Value::String(expected), Value::String(actual))
            if expected != actual
                && field_name.is_some_and(is_generated_resource_id_field)
                && is_generated_resource_id(expected)
                && is_generated_resource_id(actual) =>
        {
            match replacements.get(expected) {
                Some(existing) if existing != actual => {}
                _ => {
                    replacements.insert(expected.clone(), actual.clone());
                }
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            for (expected, actual) in expected.iter().zip(actual.iter()) {
                collect_generated_resource_id_replacements(
                    expected,
                    actual,
                    field_name,
                    replacements,
                );
            }
        }
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, expected_value) in expected {
                if let Some(actual_value) = actual.get(key) {
                    collect_generated_resource_id_replacements(
                        expected_value,
                        actual_value,
                        Some(key),
                        replacements,
                    );
                }
            }
        }
        _ => {}
    }
}

fn apply_generated_resource_id_replacements(
    value: &mut Value,
    replacements: &HashMap<String, String>,
) {
    match value {
        Value::String(text) => {
            *text = replace_generated_resource_ids(text, replacements);
        }
        Value::Array(items) => {
            for item in items {
                apply_generated_resource_id_replacements(item, replacements);
            }
        }
        Value::Object(object) => {
            let original = std::mem::take(object);
            *object = original
                .into_iter()
                .map(|(key, mut value)| {
                    apply_generated_resource_id_replacements(&mut value, replacements);
                    (replace_generated_resource_ids(&key, replacements), value)
                })
                .collect();
        }
        _ => {}
    }
}

fn replace_generated_resource_ids(text: &str, replacements: &HashMap<String, String>) -> String {
    replacements
        .iter()
        .fold(text.to_string(), |text, (from, to)| text.replace(from, to))
}

fn is_generated_resource_id_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "id" | "ids"
            | "resource_id"
            | "resourceId"
            | "step_id"
            | "stepId"
            | "condition_id"
            | "conditionId"
            | "operation_id"
            | "operationId"
    )
}

fn is_generated_resource_id(value: &str) -> bool {
    is_uuid_like(value)
        || GENERATED_RESOURCE_ID_PREFIXES.iter().any(|prefix| {
            value
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('-'))
                .is_some_and(|suffix| {
                    suffix.len() >= 8 && suffix.chars().all(|ch| ch.is_ascii_hexdigit())
                })
        })
}

const GENERATED_RESOURCE_ID_PREFIXES: &[&str] = &[
    "API-INTEGRATION",
    "API_INTEGRATION",
    "API_INTEGRATIONS",
    "CONDITION",
    "DELAY",
    "ENTITIES",
    "ENTITY",
    "FLOW",
    "FLOW_CONFIG",
    "FLOW_STEPS",
    "FLOW_TRANSITION_FUNCTIONS",
    "FUNCTION",
    "FUNCTIONS",
    "FUNCTION_STEPS",
    "HANDOFF",
    "HANDOFFS",
    "KEYPHRASE_BOOSTING",
    "PARAMETER",
    "PHRASE_FILTERING",
    "PRONUNCIATIONS",
    "SMS_TEMPLATE",
    "SMS_TEMPLATES",
    "STOP_KEYWORDS",
    "TOPIC",
    "TOPICS",
    "TRANSCRIPT_CORRECTIONS",
    "TRANSITION",
    "VARIABLE",
    "VARIABLES",
    "VARIANT",
    "VARIANTS",
    "VARIANT_ATTRIBUTE",
    "VARIANT_ATTRIBUTES",
];

fn is_uuid_like(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.chars().enumerate().all(|(index, ch)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            ch == '-'
        } else {
            ch.is_ascii_hexdigit()
        }
    })
}

#[test]
fn normalizes_generated_resource_ids_across_references() {
    let expected = serde_json::json!({
        "commands": [{
            "type": "create_function",
            "create_function": {
                "id": "FUNCTIONS-aaaaaaaa",
                "name": "lookup",
                "actions": "Call {{fn:FUNCTIONS-aaaaaaaa}}"
            }
        }, {
            "type": "variable_update",
            "variable_update": {
                "id": "VARIABLES-bbbbbbbb",
                "name": "lookup_count",
                "references": {
                    "functions": {
                        "FUNCTIONS-aaaaaaaa": true
                    }
                }
            }
        }]
    });
    let actual = serde_json::json!({
        "commands": [{
            "type": "create_function",
            "create_function": {
                "id": "FUNCTIONS-11111111",
                "name": "lookup",
                "actions": "Call {{fn:FUNCTIONS-11111111}}"
            }
        }, {
            "type": "variable_update",
            "variable_update": {
                "id": "VARIABLES-22222222",
                "name": "lookup_count",
                "references": {
                    "functions": {
                        "FUNCTIONS-11111111": true
                    }
                }
            }
        }]
    });

    assert_eq!(normalize_generated_resource_ids(expected, &actual), actual);
}

fn request_path_matches(recorded_path: &str, actual_path: &str) -> bool {
    if recorded_path == actual_path {
        return true;
    }
    python_replay_path_aliases(actual_path)
        .iter()
        .any(|alias| alias == recorded_path)
}

fn python_replay_path_aliases(path: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    if path.starts_with("/accounts/") {
        aliases.push(format!("/adk/v1{path}"));
    }
    if let Some(rest) = path.strip_prefix("/adk/v1/adk/v1/") {
        aliases.push(format!("/adk/v1/{rest}"));
    }
    if let Some(rest) = path.strip_prefix("/adk/v1/v1/") {
        aliases.push(format!("/v1/{rest}"));
    }
    aliases
}

fn run_and_check_command(
    scenario: &str,
    workflow: &str,
    expected: &CommandRecord,
    substitutions: &[(String, String)],
    subject: ReplaySubject,
    fixture_paths: &ReplayFixturePaths,
) {
    let argv = expected
        .argv
        .iter()
        .skip(1)
        .map(|arg| substitute(arg, substitutions))
        .collect::<Vec<_>>();
    let output = match subject {
        ReplaySubject::Rust => run_rust_poly(&argv, expected.stdin.as_deref(), substitutions),
        ReplaySubject::Python => {
            run_python_adk(scenario, &argv, expected.stdin.as_deref(), substitutions)
        }
    };
    let actual_stdout = String::from_utf8_lossy(&output.stdout);
    let actual_stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(expected.exit_code),
        "{scenario}/{workflow}/{}: exit code mismatch\n{}\nargv={argv:?}\nstdout={actual_stdout}\nstderr={actual_stderr}",
        expected.name,
        fixture_paths.diagnostic_lines()
    );
    let actual_json = serde_json::from_str::<Value>(actual_stdout.trim()).ok();

    if expected.stdout_json.is_some() {
        assert!(
            actual_json.is_some(),
            "{scenario}/{workflow}/{}: expected JSON stdout\n{}\nargv={argv:?}\nstdout={actual_stdout}\nstderr={actual_stderr}",
            expected.name,
            fixture_paths.diagnostic_lines()
        );
    }

    if let (Some(expected_json), Some(actual_json)) = (&expected.stdout_json, &actual_json) {
        assert_json_contract(JsonContractAssertion {
            scenario,
            workflow,
            command_name: &expected.name,
            argv: &argv,
            expected: expected_json,
            actual: actual_json,
            subject,
            substitutions,
            actual_stdout: &actual_stdout,
            actual_stderr: &actual_stderr,
            fixture_paths,
        });
    }
}

fn run_python_adk(
    _scenario: &str,
    args: &[String],
    stdin: Option<&str>,
    substitutions: &[(String, String)],
) -> Output {
    let base_url = lookup_substitution("${HTTPMOCK_ROOT_URL}", substitutions);
    let mut command = Command::new(python_adk_bin());
    command.current_dir(lookup_substitution("${TMP}", substitutions));
    command
        .env("POLY_ADK_KEY", "<redacted>")
        .env("POLY_ADK_KEY_US", "<redacted>")
        .env("POLY_ADK_BASE_URL", &base_url)
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .env_remove("GITHUB_ACCESS_TOKEN")
        .args(args);
    let Some(stdin) = stdin else {
        return command.output().expect("run Python ADK");
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn Python ADK with stdin");
    child
        .stdin
        .as_mut()
        .expect("Python ADK stdin")
        .write_all(substitute(stdin, substitutions).as_bytes())
        .expect("write Python ADK stdin");
    child.wait_with_output().expect("run Python ADK")
}

fn run_rust_poly(
    args: &[String],
    stdin: Option<&str>,
    substitutions: &[(String, String)],
) -> Output {
    let base_url = lookup_substitution("${HTTPMOCK_BASE_URL}", substitutions);
    let mut command = Command::new(env!("CARGO_BIN_EXE_poly"));
    command.current_dir(lookup_substitution("${TMP}", substitutions));
    command
        .env("POLY_ADK_KEY", "httpmock-replay-key")
        .env("POLY_ADK_BASE_URL", &base_url)
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .env_remove("GITHUB_ACCESS_TOKEN");
    if let Some(name) = maybe_lookup_substitution("${GENERATED_ADK_BRANCH_NAME}", substitutions) {
        command.env("POLY_ADK_GENERATED_BRANCH_NAME", name);
    }
    if let Some(state_dir) = maybe_lookup_substitution("${REPLAY_STATE_DIR}", substitutions) {
        command.env("POLY_ADK_REPLAY_STATE_DIR", state_dir);
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

struct JsonContractAssertion<'a> {
    scenario: &'a str,
    workflow: &'a str,
    command_name: &'a str,
    argv: &'a [String],
    expected: &'a Value,
    actual: &'a Value,
    subject: ReplaySubject,
    substitutions: &'a [(String, String)],
    actual_stdout: &'a str,
    actual_stderr: &'a str,
    fixture_paths: &'a ReplayFixturePaths,
}

fn assert_json_contract(assertion: JsonContractAssertion<'_>) {
    let JsonContractAssertion {
        scenario,
        workflow,
        command_name,
        argv,
        expected,
        actual,
        subject,
        substitutions,
        actual_stdout,
        actual_stderr,
        fixture_paths,
    } = assertion;
    let mut expected = substitute_json(expected, substitutions, Some(actual));
    if matches!(subject, ReplaySubject::Rust) {
        expected = normalize_generated_resource_ids(expected, actual);
        if let Some(object) = expected.as_object_mut() {
            object.remove("traceback");
        }
    }
    let actual = actual.clone();
    assert_eq!(
        expected,
        actual,
        "{scenario}/{workflow}/{command_name}: JSON stdout mismatch\n{}\nargv={argv:?}\nexpected={expected}\nactual={actual}\nstdout={actual_stdout}\nstderr={actual_stderr}",
        fixture_paths.diagnostic_lines()
    );
}

fn apply_file_edit(
    scenario: &str,
    workflow: &str,
    record: &FileEditRecord,
    tmp: &Path,
    substitutions: &[(String, String)],
    file_seeds: &mut HashMap<String, String>,
    fixture_paths: &ReplayFixturePaths,
) {
    assert!(
        record.success,
        "{scenario}/{workflow}/{}: cannot replay a file edit that failed during recording\n{}",
        record.name,
        fixture_paths.diagnostic_lines()
    );
    let project_root = project_root_for_file_edit(&record.name, tmp, substitutions);
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
                        "{scenario}/{workflow}/{}: create {}: {error}\n{}",
                        record.name,
                        parent.display(),
                        fixture_paths.diagnostic_lines()
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
                "{scenario}/{workflow}/{}: target text not found in {}\n{}",
                record.name,
                path.display(),
                fixture_paths.diagnostic_lines()
            );
            let updated = existing.replace(&target, &replacement);
            fs::write(&path, updated)
        }
        "delete_file" => fs::remove_file(&path),
        other => panic!(
            "{scenario}/{workflow}/{}: unsupported file edit operation {other}\n{}",
            record.name,
            fixture_paths.diagnostic_lines()
        ),
    };
    result.unwrap_or_else(|error| {
        panic!(
            "{scenario}/{workflow}/{}: apply file edit to {}: {error}\n{}",
            record.name,
            path.display(),
            fixture_paths.diagnostic_lines()
        )
    });
}

fn apply_file_assertion(
    scenario: &str,
    workflow: &str,
    record: &FileAssertionRecord,
    tmp: &Path,
    substitutions: &[(String, String)],
    fixture_paths: &ReplayFixturePaths,
) {
    let project_root = project_root_for_file_edit(&record.name, tmp, substitutions);
    let relative_path = substitute(&record.path, substitutions);
    let path = project_root.join(&relative_path);
    let content = fs::read_to_string(&path);
    assert_eq!(
        content.is_ok(),
        record.exists,
        "{scenario}/{workflow}/{}: file existence mismatch for {}\n{}",
        record.name,
        path.display(),
        fixture_paths.diagnostic_lines()
    );
    if !record.exists {
        return;
    }
    let content = content.unwrap_or_else(|error| {
        panic!(
            "{scenario}/{workflow}/{}: read {}: {error}\n{}",
            record.name,
            path.display(),
            fixture_paths.diagnostic_lines()
        )
    });
    for needle in &record.contains {
        let needle = substitute(needle, substitutions);
        assert!(
            content.contains(&needle),
            "{scenario}/{workflow}/{}: {} did not contain expected text {needle:?}\n{}\ncontent:\n{content}",
            record.name,
            path.display(),
            fixture_paths.diagnostic_lines()
        );
    }
    for needle in &record.not_contains {
        let needle = substitute(needle, substitutions);
        assert!(
            !content.contains(&needle),
            "{scenario}/{workflow}/{}: {} contained unexpected text {needle:?}\n{}\ncontent:\n{content}",
            record.name,
            path.display(),
            fixture_paths.diagnostic_lines()
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

fn project_root_for_file_edit(
    name: &str,
    tmp: &Path,
    substitutions: &[(String, String)],
) -> PathBuf {
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
    let account_id = maybe_lookup_substitution("${ACCOUNT_ID}", substitutions)
        .unwrap_or_else(|| TARGET_ACCOUNT_ID.to_string());
    let project_id = maybe_lookup_substitution("${PROJECT_ID}", substitutions)
        .unwrap_or_else(|| TARGET_PROJECT_ID.to_string());
    base.join(account_id).join(project_id)
}

fn substitutions_for(
    scenario: &str,
    tmp: &Path,
    playback_server: &MockServer,
    cassette_text: &str,
    manifest: &Manifest,
) -> Vec<(String, String)> {
    let account_id = manifest_target_account_id(manifest);
    let project_id = manifest_target_project_id(manifest);
    let branch_name = extract_recording_branch_name(cassette_text)
        .unwrap_or_else(|| "adk-rs-recording-replay".to_string());
    let generic_prefix = format!("adk-rs-recording-{scenario}-");
    let run_id = branch_name
        .strip_prefix(&generic_prefix)
        .or_else(|| branch_name.strip_prefix("adk-rs-recording-conflict-"))
        .or_else(|| branch_name.strip_prefix("adk-rs-recording-merge-"))
        .unwrap_or("replay")
        .to_string();
    let mut substitutions = vec![
        ("${TMP}".to_string(), tmp.to_string_lossy().to_string()),
        ("${ACCOUNT_ID}".to_string(), account_id.to_string()),
        ("${PROJECT_ID}".to_string(), project_id.to_string()),
        (
            "${HTTPMOCK_BASE_URL}".to_string(),
            httpmock_adk_base_url(playback_server),
        ),
        (
            "${HTTPMOCK_ROOT_URL}".to_string(),
            playback_server.base_url(),
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
    if let Some(conversation_id) = first_recorded_conversation_id(manifest) {
        substitutions.push((
            "${CHAT_RESUME_CONVERSATION_ID}".to_string(),
            conversation_id,
        ));
    }
    if let Some(hash) = deployment_hash_at(manifest, 0) {
        substitutions.push((
            "${DEPLOYMENT_NEWEST_HASH}".to_string(),
            deployment_hash_prefix(&hash),
        ));
    }
    if let Some(hash) = deployment_hash_at(manifest, 1) {
        substitutions.push((
            "${DEPLOYMENT_PREVIOUS_HASH}".to_string(),
            deployment_hash_prefix(&hash),
        ));
    }
    if let Some(resolutions) = merge_resolutions_for_conflicts(manifest) {
        substitutions.push(("${MERGE_RESOLUTIONS}".to_string(), resolutions));
    }
    substitutions
}

fn manifest_target_account_id(manifest: &Manifest) -> &str {
    manifest
        .source
        .as_ref()
        .map(|source| source.account_id.as_str())
        .filter(|account_id| !account_id.is_empty())
        .unwrap_or(TARGET_ACCOUNT_ID)
}

fn manifest_target_project_id(manifest: &Manifest) -> &str {
    manifest
        .source
        .as_ref()
        .map(|source| source.project_id.as_str())
        .filter(|project_id| !project_id.is_empty())
        .unwrap_or(TARGET_PROJECT_ID)
}

fn first_recorded_conversation_id(manifest: &Manifest) -> Option<String> {
    find_command(manifest, "chat json leaves conversation open for resume")
        .and_then(|record| record.stdout_json.as_ref())
        .and_then(|json| json.pointer("/conversations/0/conversation_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn deployment_hash_at(manifest: &Manifest, index: usize) -> Option<String> {
    let deployment = find_command(manifest, "deployments list sandbox for mutation hashes")?
        .stdout_json
        .as_ref()?
        .get("versions")?
        .as_array()?
        .get(index)?;
    deployment
        .get("version_hash")
        .or_else(|| deployment.get("versionHash"))
        .or_else(|| deployment.get("hash"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn deployment_hash_prefix(hash: &str) -> String {
    hash.chars().take(9).collect()
}

fn merge_resolutions_for_conflicts(manifest: &Manifest) -> Option<String> {
    let conflicts = find_command(manifest, "merge conflict without resolutions")?
        .stdout_json
        .as_ref()?
        .get("conflicts")?
        .as_array()?;
    let resolutions = conflicts
        .iter()
        .filter_map(|conflict| {
            let path = conflict.get("path")?.as_array()?.clone();
            Some(serde_json::json!({
                "path": path,
                "strategy": "theirs"
            }))
        })
        .collect::<Vec<_>>();
    (!resolutions.is_empty())
        .then(|| serde_json::to_string(&resolutions).ok())
        .flatten()
}

fn find_command<'a>(manifest: &'a Manifest, name: &str) -> Option<&'a CommandRecord> {
    manifest.workflows.iter().find_map(|workflow| {
        workflow.steps.iter().find_map(|step| match step {
            WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
            | WorkflowStep::LegacyCommand(record)
                if record.name == name =>
            {
                Some(record)
            }
            _ => None,
        })
    })
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
