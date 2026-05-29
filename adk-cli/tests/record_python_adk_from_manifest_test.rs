//! Refresh Python ADK recordings from the saved YAML manifests.
//!
//! This is the manifest-driven recorder: the `*.commands.yaml` file is the
//! workflow definition, and this test just runs it with Python ADK, refreshes
//! observed command outputs, and writes the accompanying `*.httpmock.yaml`
//! cassette. Dynamic values such as throwaway branch names, chat conversation
//! IDs, deployment hashes, and merge resolutions are captured as placeholders so
//! the manifests stay readable.

mod support;

use httpmock::prelude::*;
use serde_json::Value;
use serde_yaml_ng::{from_str, to_string};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use support::python_recordings::{
    SCENARIOS, TARGET_ACCOUNT_ID, TARGET_PROJECT_ID, fixture_dir, httpmock_adk_base_url,
    python_adk_bin, replace_all as substitute, temp_recording_dir,
};
use support::recording_manifest::{
    CommandRecord, FileEditRecord, Manifest, TaggedWorkflowStep, WorkflowStep,
};

const AGENT_STUDIO_HOST_URL: &str = "https://api.us.poly.ai";

#[test]
#[ignore = "refreshes selected Python ADK recordings from their YAML manifests"]
fn record_python_adk_fixtures_from_manifests() {
    for scenario in selected_scenarios() {
        record_scenario_from_manifest(&scenario);
    }
}

fn selected_scenarios() -> Vec<String> {
    match std::env::var("PYTHON_ADK_RECORD_SCENARIO") {
        Ok(name) if !name.trim().is_empty() => vec![name],
        _ => SCENARIOS
            .iter()
            .map(|scenario| scenario.to_string())
            .collect(),
    }
}

fn record_scenario_from_manifest(scenario: &str) {
    let fixture_dir = fixture_dir();
    let manifest_path = fixture_dir.join(format!("{scenario}.commands.yaml"));
    let manifest_text = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("{scenario}: read manifest: {error}"));
    let mut manifest: Manifest = from_str(&manifest_text)
        .unwrap_or_else(|error| panic!("{scenario}: parse manifest: {error}"));
    let target_account_id = manifest_target_account_id(&manifest).to_string();
    let target_project_id = manifest_target_project_id(&manifest).to_string();

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
    fs::create_dir_all(&tmp).unwrap_or_else(|error| panic!("{scenario}: create tmp: {error}"));
    let mut substitutions = substitutions_for_recording(
        scenario,
        &tmp,
        &server,
        &target_account_id,
        &target_project_id,
    );
    let mut file_seeds = HashMap::new();

    for workflow in &mut manifest.workflows {
        for step in &mut workflow.steps {
            match step {
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
                WorkflowStep::Tagged(TaggedWorkflowStep::Command(record))
                | WorkflowStep::LegacyCommand(record) => {
                    prepare_dynamic_command(record, &substitutions);
                    let actual = run_python_command(scenario, record, &substitutions);
                    assert_eq!(
                        actual.exit_code, record.exit_code,
                        "{scenario}/{}: exit code changed for {}\nargv={:?}\nstderr={}",
                        workflow.name, record.name, record.argv, actual.stderr
                    );
                    capture_dynamic_outputs(&actual, &mut substitutions);
                    *record = actual;
                }
                WorkflowStep::Tagged(TaggedWorkflowStep::FileAssertion(assertion)) => {
                    let project_root =
                        project_root_for_file_edit(&assertion.name, &tmp, &substitutions);
                    let path = project_root.join(substitute(&assertion.path, &substitutions));
                    let content = fs::read_to_string(path).ok();
                    assertion.exists = content.is_some();
                }
            }
        }
    }

    let recording_path = recording
        .save(format!("{scenario}-python-adk"))
        .unwrap_or_else(|error| panic!("{scenario}: save httpmock recording: {error}"));
    let mut recording_yaml = fs::read_to_string(&recording_path)
        .unwrap_or_else(|error| panic!("{scenario}: read temporary httpmock recording: {error}"));
    if let Some(api_key) = api_key_from_env() {
        recording_yaml = recording_yaml.replace(&api_key, "<redacted>");
    }
    fs::write(
        fixture_dir.join(&manifest.httpmock_recording),
        recording_yaml,
    )
    .unwrap_or_else(|error| panic!("{scenario}: write cassette: {error}"));

    let manifest_yaml = to_string(&manifest)
        .unwrap_or_else(|error| panic!("{scenario}: serialize refreshed manifest: {error}"));
    fs::write(&manifest_path, manifest_yaml)
        .unwrap_or_else(|error| panic!("{scenario}: write refreshed manifest: {error}"));
    let _ = fs::remove_dir_all(tmp);
}

fn run_python_command(
    scenario: &str,
    expected: &CommandRecord,
    substitutions: &[(String, String)],
) -> CommandRecord {
    let args = expected
        .argv
        .iter()
        .skip(1)
        .map(|arg| substitute(arg, substitutions))
        .collect::<Vec<_>>();
    let mut command = Command::new(python_adk_bin());
    let base_url = base_url_for_command(scenario, substitutions);
    command
        .env("POLY_ADK_BASE_URL", &base_url)
        .env("POLY_ADK_BASE_URL_US", &base_url)
        .env("POLY_ADK_BASE_URL_US_1", &base_url)
        .args(&args);

    let output = if let Some(stdin) = expected.stdin.as_deref() {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn Python ADK");
        child
            .stdin
            .as_mut()
            .expect("Python ADK stdin")
            .write_all(substitute(stdin, substitutions).as_bytes())
            .expect("write Python ADK stdin");
        child.wait_with_output().expect("wait for Python ADK")
    } else {
        command.output().expect("run Python ADK")
    };

    let replacements = output_replacements(substitutions);
    let stdout_raw = substitute(&String::from_utf8_lossy(&output.stdout), &replacements);
    let stderr = substitute(&String::from_utf8_lossy(&output.stderr), &replacements);
    let stdout_json = serde_json::from_str::<Value>(stdout_raw.trim())
        .ok()
        .map(normalize_json_value);

    CommandRecord {
        name: expected.name.clone(),
        argv: expected.argv.clone(),
        stdin: expected.stdin.clone(),
        exit_code: output.status.code().unwrap_or(1),
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

fn base_url_for_command(scenario: &str, substitutions: &[(String, String)]) -> String {
    if matches!(
        scenario,
        "chat-error-metadata" | "chat-session-controls" | "deployments-mutation"
    ) {
        return lookup("${HTTPMOCK_ROOT_URL}", substitutions);
    }
    lookup("${HTTPMOCK_BASE_URL}", substitutions)
}

fn prepare_dynamic_command(record: &mut CommandRecord, substitutions: &[(String, String)]) {
    match record.name.as_str() {
        "chat json resumes conversation id and exits" => {
            replace_arg_after(
                &mut record.argv,
                "--conversation-id",
                "${CHAT_RESUME_CONVERSATION_ID}",
            );
        }
        "deployments show newest sandbox deployment" => {
            replace_arg_after(&mut record.argv, "show", "${DEPLOYMENT_NEWEST_HASH}");
        }
        "deployments promote newest to pre-release dry run"
        | "deployments promote newest to pre-release" => {
            replace_arg_after(&mut record.argv, "--from", "${DEPLOYMENT_NEWEST_HASH}");
        }
        "deployments rollback sandbox to previous dry run"
        | "deployments rollback sandbox to previous" => {
            replace_arg_after(&mut record.argv, "--to", "${DEPLOYMENT_PREVIOUS_HASH}");
        }
        "deployments restore sandbox to newest" => {
            replace_arg_after(&mut record.argv, "--to", "${DEPLOYMENT_NEWEST_HASH}");
        }
        "merge conflict with theirs resolution" => {
            replace_arg_after(&mut record.argv, "--resolutions", "${MERGE_RESOLUTIONS}");
        }
        _ => {}
    }

    for arg in &record.argv {
        for placeholder in dynamic_placeholders() {
            if arg.contains(placeholder) {
                lookup(placeholder, substitutions);
            }
        }
    }
}

fn replace_arg_after(argv: &mut [String], marker: &str, placeholder: &str) {
    if let Some(index) = argv.iter().position(|arg| arg == marker)
        && let Some(value) = argv.get_mut(index + 1)
    {
        *value = placeholder.to_string();
    }
}

fn capture_dynamic_outputs(record: &CommandRecord, substitutions: &mut Vec<(String, String)>) {
    match record.name.as_str() {
        "chat json leaves conversation open for resume" => {
            let conversation_id = first_recorded_conversation_id(record)
                .expect("chat resume fixture did not return a conversation ID");
            upsert_substitution(
                substitutions,
                "${CHAT_RESUME_CONVERSATION_ID}",
                conversation_id,
            );
        }
        "deployments list sandbox for mutation hashes" => {
            let newest_hash = deployment_hash_at(record, 0)
                .expect("deployment mutation fixture did not return a newest hash");
            let previous_hash = deployment_hash_at(record, 1)
                .expect("deployment mutation fixture did not return a previous hash");
            upsert_substitution(
                substitutions,
                "${DEPLOYMENT_NEWEST_HASH}",
                deployment_hash_prefix(&newest_hash),
            );
            upsert_substitution(
                substitutions,
                "${DEPLOYMENT_PREVIOUS_HASH}",
                deployment_hash_prefix(&previous_hash),
            );
        }
        "merge conflict without resolutions" => {
            let resolutions = merge_resolutions_for_conflicts(record)
                .expect("merge conflict fixture did not return resolvable conflicts");
            upsert_substitution(substitutions, "${MERGE_RESOLUTIONS}", resolutions);
        }
        _ => {}
    }
}

fn first_recorded_conversation_id(record: &CommandRecord) -> Option<String> {
    record
        .stdout_json
        .as_ref()
        .and_then(|json| json.pointer("/conversations/0/conversation_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn deployment_hash_at(record: &CommandRecord, index: usize) -> Option<String> {
    let deployment = record
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

fn merge_resolutions_for_conflicts(record: &CommandRecord) -> Option<String> {
    let conflicts = record.stdout_json.as_ref()?.get("conflicts")?.as_array()?;
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

fn apply_file_edit(
    scenario: &str,
    workflow: &str,
    record: &mut FileEditRecord,
    tmp: &Path,
    substitutions: &[(String, String)],
    file_seeds: &mut HashMap<String, String>,
) {
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
            existing.push_str(&substitute(
                record.content.as_deref().unwrap_or_default(),
                substitutions,
            ));
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
                .entry(relative_path)
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
            if !existing.contains(&target) {
                record.success = false;
                record.error = Some(format!("target text not found in {}", path.display()));
                return;
            }
            fs::write(&path, existing.replace(&target, &replacement))
        }
        "delete_file" => fs::remove_file(&path),
        other => panic!(
            "{scenario}/{workflow}/{}: unsupported file edit operation {other}",
            record.name
        ),
    };
    record.success = result.is_ok();
    record.error = result.err().map(|error| error.to_string());
}

fn read_or_seed_file(
    scenario: &str,
    workflow: &str,
    step_name: &str,
    relative_path: &str,
    path: &Path,
    file_seeds: &mut HashMap<String, String>,
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
        Err(error) => panic!(
            "{scenario}/{workflow}/{step_name}: read {}: {error}",
            path.display()
        ),
    }
}

fn substitutions_for_recording(
    scenario: &str,
    tmp: &Path,
    server: &MockServer,
    target_account_id: &str,
    target_project_id: &str,
) -> Vec<(String, String)> {
    let run_id = recording_run_id();
    vec![
        ("${TMP}".to_string(), tmp.to_string_lossy().to_string()),
        ("${ACCOUNT_ID}".to_string(), target_account_id.to_string()),
        ("${PROJECT_ID}".to_string(), target_project_id.to_string()),
        (
            "${HTTPMOCK_BASE_URL}".to_string(),
            httpmock_adk_base_url(server),
        ),
        ("${HTTPMOCK_ROOT_URL}".to_string(), server.base_url()),
        (
            "${BRANCH_NAME}".to_string(),
            format!("adk-rs-recording-{scenario}-{run_id}"),
        ),
        (
            "${MAIN_PUSH_TEXT}".to_string(),
            format!(
                "\n\n# ADK recording push-from-main {run_id}\nThis line was pushed by the manifest-driven Python ADK recorder.\n"
            ),
        ),
        (
            "${MERGE_TEXT}".to_string(),
            format!(
                "\n\n# ADK recording branch merge {run_id}\nThis line was merged by the manifest-driven Python ADK recorder.\n"
            ),
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
    ]
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
    let account_id = lookup_optional("${ACCOUNT_ID}", substitutions)
        .unwrap_or_else(|| TARGET_ACCOUNT_ID.to_string());
    let project_id = lookup_optional("${PROJECT_ID}", substitutions)
        .unwrap_or_else(|| TARGET_PROJECT_ID.to_string());
    base.join(account_id).join(project_id)
}

fn output_replacements(expansions: &[(String, String)]) -> Vec<(String, String)> {
    let mut all = machine_path_replacements();
    all.extend(
        expansions
            .iter()
            .filter(|(placeholder, _)| reverse_placeholder_in_output(placeholder))
            .map(|(placeholder, actual)| (actual.clone(), placeholder.clone())),
    );
    all
}

fn reverse_placeholder_in_output(placeholder: &str) -> bool {
    !matches!(
        placeholder,
        "${DEPLOYMENT_NEWEST_HASH}" | "${DEPLOYMENT_PREVIOUS_HASH}" | "${MERGE_RESOLUTIONS}"
    )
}

fn dynamic_placeholders() -> &'static [&'static str] {
    &[
        "${CHAT_RESUME_CONVERSATION_ID}",
        "${DEPLOYMENT_NEWEST_HASH}",
        "${DEPLOYMENT_PREVIOUS_HASH}",
        "${MERGE_RESOLUTIONS}",
    ]
}

fn upsert_substitution(substitutions: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some((_, existing)) = substitutions
        .iter_mut()
        .find(|(placeholder, _)| placeholder == key)
    {
        *existing = value;
    } else {
        substitutions.push((key.to_string(), value));
    }
}

fn machine_path_replacements() -> Vec<(String, String)> {
    let mut replacements = Vec::new();
    let bin = PathBuf::from(python_adk_bin());
    if bin.is_absolute()
        && let Some(bin_dir) = bin.parent()
        && let Some(venv_dir) = bin_dir.parent()
    {
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
    if let Ok(cwd) = std::env::var("PYTHON_ADK_CWD") {
        replacements.push((cwd, "${PYTHON_ADK_ROOT}".to_string()));
    }
    if let Ok(home) = std::env::var("HOME") {
        replacements.push((home, "${HOME}".to_string()));
    }
    replacements
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

fn lookup(name: &str, substitutions: &[(String, String)]) -> String {
    substitutions
        .iter()
        .find_map(|(from, to)| (from == name).then(|| to.clone()))
        .unwrap_or_else(|| panic!("missing substitution {name}"))
}

fn lookup_optional(name: &str, substitutions: &[(String, String)]) -> Option<String> {
    substitutions
        .iter()
        .find_map(|(from, to)| (from == name).then(|| to.clone()))
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

fn api_key_from_env() -> Option<String> {
    ["POLY_ADK_KEY_US", "POLY_ADK_KEY"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

fn recording_run_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    format!("{:x}", ts & 0xffff_ffff)
}
