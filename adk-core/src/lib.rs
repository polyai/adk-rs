use adk_types::{DiffMap, DomainError, ProjectConfig, ResourceMap};

pub mod discover;
mod python_functions;
mod python_syntax;
mod resources;
mod service;
mod status_snapshot;
mod validation;
mod workspace;

use adk_api_client::ApiError;
use adk_io::{FileSystem, StdFileSystem, compute_hash, parse_multi_resource_path};
use anyhow::Result;
pub use discover::discover_local_resources;
use globset::{Glob, GlobSetBuilder};
use python_functions::{
    PYTHON_FLOW_IMPORT_STATUS_KEY_PREFIX, PYTHON_FUNCTION_STATUS_HASH_PREFIX,
    legacy_python_function_raw, legacy_python_snapshot_hashes, local_resource_content,
    normalize_legacy_python_status_function_resources, normalize_python_function_metadata_spacing,
    resource_file_content,
};
pub use resources::{DiscoveredResourceChanges, DiscoveredResourcePaths, TypedResourceLifecycle};
use serde_json::Value;
pub use service::{AdkService, PullOutcome};
use status_snapshot::{StatusResourcePayload, current_status_hash_for_expected};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;
use walkdir::WalkDir;
pub use workspace::ProjectWorkspace;

pub const PROJECT_CONFIG_FILE: &str = "project.yaml";
pub const STATUS_FILE: &str = "_gen/.agent_studio_config";
const MIGRATED_LEGACY_TOPIC_FILES: &str = "migrated_legacy_topic_files";
const PYTHON_VARIANT_STATUS_KEY_PREFIX: &str = "__python_variant__/";

struct PushChangeSet {
    resources: ResourceMap,
    has_deletions: bool,
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("{0}")]
    Api(#[from] ApiError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

fn project_config_yaml(cfg: &ProjectConfig) -> Result<String, CoreError> {
    let mut map = serde_yaml::Mapping::new();
    map.insert(
        serde_yaml::Value::String("project_id".to_string()),
        serde_yaml::Value::String(cfg.project_id.clone()),
    );
    map.insert(
        serde_yaml::Value::String("account_id".to_string()),
        serde_yaml::Value::String(cfg.account_id.clone()),
    );
    map.insert(
        serde_yaml::Value::String("region".to_string()),
        serde_yaml::Value::String(cfg.region.clone()),
    );
    if let Some(project_name) = &cfg.project_name
        && !project_name.is_empty()
    {
        map.insert(
            serde_yaml::Value::String("project_name".to_string()),
            serde_yaml::Value::String(project_name.clone()),
        );
    }
    serde_yaml::to_string(&serde_yaml::Value::Mapping(map))
        .map_err(|e| DomainError::InvalidData(e.to_string()).into())
}

fn project_config_contains_branch_id(raw: &str) -> bool {
    serde_yaml::from_str::<serde_yaml::Value>(raw)
        .ok()
        .and_then(|value| match value {
            serde_yaml::Value::Mapping(mapping) => Some(mapping),
            _ => None,
        })
        .is_some_and(|mapping| mapping.contains_key("branch_id"))
}

fn legacy_python_status_resource_path(
    resource_name: &str,
    payload: &Value,
    ordinal: usize,
) -> Option<String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let clean_name = |lowercase| discover::clean_name(name, lowercase);
    let flow_folder = || {
        payload
            .get("flow_name")
            .and_then(Value::as_str)
            .map(|flow_name| discover::clean_name(flow_name, true))
    };
    match resource_name {
        "api_integration" => Some(format!(
            "config/api_integrations.yaml/api_integrations/{}",
            clean_name(false)
        )),
        "functions" => {
            if let Some(flow_folder) = flow_folder() {
                Some(format!("flows/{flow_folder}/functions/{name}.py"))
            } else {
                Some(format!("functions/{name}.py"))
            }
        }
        "topics" => Some(format!("topics/{}.yaml", clean_name(true))),
        "personality" => Some("agent_settings/personality.yaml".to_string()),
        "role" => Some("agent_settings/role.yaml".to_string()),
        "rules" => Some("agent_settings/rules.txt".to_string()),
        "flow_steps" => flow_folder()
            .map(|flow_folder| format!("flows/{flow_folder}/steps/{}.yaml", clean_name(true))),
        "function_steps" => {
            flow_folder().map(|flow_folder| format!("flows/{flow_folder}/function_steps/{name}.py"))
        }
        "flow_config" => Some(format!("flows/{}/flow_config.yaml", clean_name(true))),
        "entities" => Some(format!(
            "config/entities.yaml/entities/{}",
            clean_name(false)
        )),
        "experimental_config" => Some("agent_settings/experimental_config.json".to_string()),
        "safety_filters" => Some("agent_settings/safety_filters.yaml".to_string()),
        "sms_templates" => Some(format!(
            "config/sms_templates.yaml/sms_templates/{}",
            clean_name(false)
        )),
        "handoffs" => Some(format!(
            "config/handoffs.yaml/handoffs/{}",
            clean_name(false)
        )),
        "variants" => Some(format!(
            "config/variant_attributes.yaml/variants/{}",
            clean_name(false)
        )),
        "variant_attributes" => Some(format!(
            "config/variant_attributes.yaml/attributes/{}",
            clean_name(false)
        )),
        "variables" => Some(format!("variables/{name}")),
        "voice_greeting" => Some("voice/configuration.yaml/greeting".to_string()),
        "voice_safety_filters" => Some("voice/safety_filters.yaml".to_string()),
        "voice_style_prompt" => Some("voice/configuration.yaml/style_prompt".to_string()),
        "voice_disclaimer" => Some("voice/configuration.yaml/disclaimer_messages".to_string()),
        "chat_greeting" => Some("chat/configuration.yaml/greeting".to_string()),
        "chat_safety_filters" => Some("chat/safety_filters.yaml".to_string()),
        "chat_style_prompt" => Some("chat/configuration.yaml/style_prompt".to_string()),
        "keyphrase_boosting" => {
            let keyphrase = payload
                .get("keyphrase")
                .and_then(Value::as_str)
                .unwrap_or(name);
            Some(format!(
                "voice/speech_recognition/keyphrase_boosting.yaml/keyphrases/{}",
                discover::clean_name(keyphrase, false)
            ))
        }
        "transcript_corrections" => Some(format!(
            "voice/speech_recognition/transcript_corrections.yaml/corrections/{}",
            clean_name(false)
        )),
        "asr_settings" => Some("voice/speech_recognition/asr_settings.yaml".to_string()),
        "phrase_filtering" => Some(format!(
            "voice/response_control/phrase_filtering.yaml/phrase_filtering/{}",
            clean_name(false)
        )),
        "pronunciations" => {
            let position = payload
                .get("position")
                .and_then(Value::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| ordinal.to_string());
            Some(format!(
                "voice/response_control/pronunciations.yaml/pronunciations/{}",
                discover::clean_name(&position, false)
            ))
        }
        _ => None,
    }
}

fn legacy_python_rules_reference_names(
    resources: &indexmap::IndexMap<String, indexmap::IndexMap<String, StatusResourcePayload>>,
) -> Vec<(String, String, String)> {
    [
        ("functions", "fn"),
        ("sms_templates", "sms"),
        ("handoffs", "handoff"),
        ("variant_attributes", "attr"),
        ("variables", "vrbl"),
    ]
    .into_iter()
    .flat_map(|(resource_name, reference_prefix)| {
        resources
            .get(resource_name)
            .into_iter()
            .flat_map(move |entries| {
                entries.values().filter_map(move |payload| {
                    let id = payload.resource_id()?;
                    let name = payload.name()?;
                    Some((
                        reference_prefix.to_string(),
                        id.to_string(),
                        name.to_string(),
                    ))
                })
            })
    })
    .collect()
}

fn replace_resource_ids_with_names(
    content: &str,
    replacements: &[(String, String, String)],
) -> String {
    let mut normalized = content.to_string();
    for (prefix, id, name) in replacements {
        if id.is_empty() || id == name {
            continue;
        }
        normalized = normalized.replace(
            &format!("{{{{{prefix}:{id}}}}}"),
            &format!("{{{{{prefix}:{name}}}}}"),
        );
    }
    normalized
}

fn legacy_python_status_resource_file_hash<Fs: FileSystem>(
    fs: &Fs,
    root: &Path,
    resource_name: &str,
    file_path: &str,
    payload: &Value,
    rules_reference_names: &[(String, String, String)],
) -> Option<String> {
    if payload.get("file_path").is_some() {
        return None;
    }
    match resource_name {
        "functions" => {
            let raw = legacy_python_function_raw(payload, true)?;
            Some(format!(
                "{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{}",
                compute_hash(&normalize_python_function_metadata_spacing(&raw))
            ))
        }
        "function_steps" => {
            let raw = legacy_python_function_raw(payload, false)?;
            Some(format!(
                "{PYTHON_FUNCTION_STATUS_HASH_PREFIX}{}",
                compute_hash(&normalize_python_function_metadata_spacing(&raw))
            ))
        }
        "rules" => payload
            .get("behaviour")
            .and_then(Value::as_str)
            .map(|raw| compute_hash(&replace_resource_ids_with_names(raw, rules_reference_names))),
        "variables" => payload
            .get("name")
            .and_then(Value::as_str)
            .map(|name| compute_hash(&format!("vrbl:{name}"))),
        _ => fs
            .read_to_string(&root.join(file_path))
            .ok()
            .map(|content| compute_hash(&content)),
    }
}

fn legacy_python_status_resource_content(resource_name: &str, payload: &Value) -> Option<String> {
    match resource_name {
        "functions" => legacy_python_function_raw(payload, true),
        "function_steps" => legacy_python_function_raw(payload, false),
        "rules" => payload
            .get("behaviour")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        _ => None,
    }
}

/// ADK operations backed by a concrete platform client.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    find_project_root_with_fs(&StdFileSystem, start)
}

fn find_project_root_with_fs<Fs: FileSystem>(fs: &Fs, start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if fs.exists(&current.join(PROJECT_CONFIG_FILE)) || fs.exists(&current.join(STATUS_FILE)) {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn migration_flags_from_status(
    status: &serde_json::Map<String, serde_json::Value>,
) -> BTreeSet<String> {
    status
        .get("migration_flags")
        .and_then(serde_json::Value::as_array)
        .map(|flags| {
            flags
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|flag| *flag == MIGRATED_LEGACY_TOPIC_FILES)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn migrate_legacy_topic_files<Fs: FileSystem>(
    fs: &Fs,
    project_root: &Path,
) -> Result<bool, CoreError> {
    let topics_dir = project_root.join("topics");
    if !fs.is_dir(&topics_dir) {
        return Ok(false);
    }

    let mut migrated_topics: std::collections::BTreeMap<PathBuf, serde_yaml::Value> =
        std::collections::BTreeMap::new();
    let mut old_files = Vec::new();
    let mut old_dirs = BTreeSet::new();

    for topic_path in recursive_file_paths(fs, &topics_dir)? {
        if !is_yaml_file(&topic_path) {
            continue;
        }
        let raw = fs.read_to_string(&topic_path)?;
        let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
            continue;
        };
        let serde_yaml::Value::Mapping(existing) = parsed else {
            continue;
        };
        if yaml_mapping_contains_key(&existing, "name") {
            continue;
        }

        let rel_path = topic_path.strip_prefix(&topics_dir).unwrap_or(&topic_path);
        let topic_name = rel_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        let clean_file_name = discover::clean_name(&topic_name, true);
        let clean_file_path = topics_dir.join(format!("{clean_file_name}.yaml"));
        if migrated_topics.contains_key(&clean_file_path) {
            return Err(DomainError::InvalidData(format!(
                "Can't migrate legacy topic files: multiple topics with the same file name after cleaning: {clean_file_name}"
            ))
            .into());
        }

        let mut updated = serde_yaml::Mapping::new();
        updated.insert(
            serde_yaml::Value::String("name".to_string()),
            serde_yaml::Value::String(topic_name),
        );
        for (key, value) in existing {
            updated.insert(key, value);
        }
        migrated_topics.insert(clean_file_path, serde_yaml::Value::Mapping(updated));
        old_files.push(topic_path.to_path_buf());
        if topic_path.parent() != Some(topics_dir.as_path())
            && let Some(parent) = topic_path.parent()
        {
            old_dirs.insert(parent.to_path_buf());
        }
    }

    for (path, content) in &migrated_topics {
        let serialized =
            serde_yaml::to_string(content).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        fs.write_string(path, &serialized)?;
    }
    for old_file in old_files {
        if !migrated_topics.contains_key(&old_file) {
            fs.remove_file(&old_file)?;
        }
    }
    let mut old_dirs = old_dirs.into_iter().collect::<Vec<_>>();
    old_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for old_dir in old_dirs {
        if fs.is_dir(&old_dir) && fs.read_dir(&old_dir)?.is_empty() {
            fs.remove_dir(&old_dir)?;
        }
    }
    Ok(!migrated_topics.is_empty())
}

fn recursive_file_paths<Fs: FileSystem>(fs: &Fs, root: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let mut files = Vec::new();
    if !fs.is_dir(root) {
        return Ok(files);
    }
    for path in fs.read_dir(root)? {
        if fs.is_dir(&path) {
            files.extend(recursive_file_paths(fs, &path)?);
        } else if fs.is_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn yaml_mapping_contains_key(mapping: &serde_yaml::Mapping, key: &str) -> bool {
    mapping
        .keys()
        .any(|candidate| candidate.as_str() == Some(key))
}

fn sort_json_value_keys(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let old = std::mem::take(object);
            let mut items = old.into_iter().collect::<Vec<_>>();
            items.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (key, mut value) in items {
                sort_json_value_keys(&mut value);
                object.insert(key, value);
            }
        }
        Value::Array(items) => {
            for item in items {
                sort_json_value_keys(item);
            }
        }
        _ => {}
    }
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

fn delete_local_only_resource_files(
    root: &Path,
    remote: &ResourceMap,
    local_resources: &DiscoveredResourcePaths,
) -> Result<(), CoreError> {
    let remote_file_paths: HashSet<String> = remote
        .iter()
        .flat_map(|(path, resource)| [path.clone(), resource.file_path.clone()])
        .map(|path| parse_multi_resource_path(&path).0)
        .collect();
    let mut local_only_files: Vec<String> = flatten_discovered_paths(local_resources)
        .into_iter()
        .map(|path| parse_multi_resource_path(&path).0)
        .filter(|path| !remote_file_paths.contains(path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    local_only_files.sort_by_key(|path| {
        std::cmp::Reverse((Path::new(path).components().count(), path.clone()))
    });

    for rel_path in local_only_files {
        let path = root.join(rel_path);
        if StdFileSystem.is_file(&path) {
            StdFileSystem.remove_file(&path)?;
        }
    }
    Ok(())
}

fn delete_empty_subdirectories(dir: &Path) -> Result<(), CoreError> {
    if !StdFileSystem.is_dir(dir) {
        return Ok(());
    }
    for entry in WalkDir::new(dir)
        .contents_first(true)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if StdFileSystem.is_dir(path) && StdFileSystem.read_dir(path)?.is_empty() {
            StdFileSystem.remove_dir(path)?;
        }
    }
    Ok(())
}

fn normalize_format_file_patterns(root: &Path, files: &[String]) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            let path = Path::new(file);
            let rel = if path.is_absolute() {
                path.strip_prefix(root).unwrap_or(path).to_path_buf()
            } else {
                path.to_path_buf()
            };
            rel.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

fn build_file_matcher(patterns: &[String]) -> Result<globset::GlobSet, CoreError> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).map_err(|e| DomainError::InvalidData(e.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| DomainError::InvalidData(e.to_string()).into())
}

fn format_python_content(_filename: &Path, content: &str) -> String {
    let fallback = || ensure_trailing_newline(content);
    let Some(formatted) = run_ruff_stdin(&["format", "-"], content) else {
        return fallback();
    };
    run_ruff_stdin(&["check", "--fix", "-"], &formatted).unwrap_or(formatted)
}

fn run_ruff_stdin(args: &[&str], content: &str) -> Option<String> {
    let mut child = Command::new("ruff")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(content.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn ensure_trailing_newline(content: &str) -> String {
    if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn flatten_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out: Vec<String> = paths.values().flat_map(|v| v.iter().cloned()).collect();
    out.sort();
    out
}

fn flatten_discovered_paths_by_type_order(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut out = Vec::new();
    for type_name in discover::ordered_type_names() {
        if let Some(type_paths) = paths.get(*type_name) {
            out.extend(type_paths.iter().cloned());
        }
    }
    let known_types = discover::ordered_type_names()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut remaining = paths
        .iter()
        .filter(|(type_name, _)| !known_types.contains(type_name.as_str()))
        .flat_map(|(_, type_paths)| type_paths.iter().cloned())
        .collect::<Vec<_>>();
    remaining.sort();
    out.extend(remaining);
    out
}

fn ordered_discovered_paths_for_modifications(
    paths: &DiscoveredResourcePaths,
    file_paths: &HashSet<String>,
    logical_paths: &HashSet<String>,
) -> Vec<String> {
    flatten_discovered_paths_by_type_order(paths)
        .into_iter()
        .filter(|logical_path| {
            logical_paths.contains(logical_path)
                || file_paths.contains(&parse_multi_resource_path(logical_path).0)
        })
        .collect()
}

fn flatten_deleted_discovered_paths(paths: &DiscoveredResourcePaths) -> Vec<String> {
    let mut entries = Vec::new();
    for (type_name, logical_paths) in paths {
        for path in logical_paths {
            entries.push((deleted_status_type_rank(type_name), path.clone()));
        }
    }
    entries.sort_by(|(left_rank, left_path), (right_rank, right_path)| {
        left_rank
            .cmp(right_rank)
            .then_with(|| left_path.cmp(right_path))
    });
    entries.into_iter().map(|(_, path)| path).collect()
}

fn deleted_status_type_rank(type_name: &str) -> usize {
    match type_name {
        "VoiceStylePrompt" => 0,
        "SettingsPersonality" => 1,
        "VoiceSafetyFilters" => 2,
        "SettingsRole" => 3,
        "GeneralSafetyFilters" => 4,
        "VoiceDisclaimerMessage" => 5,
        "VoiceGreeting" => 6,
        "AsrSettings" => 7,
        "Entity" => 8,
        "PhraseFilter" => 9,
        "Handoff" => 10,
        "SMSTemplate" => 11,
        other => discover::ordered_type_names()
            .iter()
            .position(|name| *name == other)
            .map(|position| position + 100)
            .unwrap_or(usize::MAX),
    }
}

fn stable_dedup(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

fn normalize_flow_resources_for_diff(resources: &mut ResourceMap, reference: Option<&ResourceMap>) {
    let step_ids = reference
        .map(flow_step_ids_by_folder_and_name)
        .unwrap_or_default();
    for (path, resource) in resources.iter_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = if path.starts_with("flows/") && path.ends_with("/flow_config.yaml") {
            canonical_flow_config_for_diff(path, &content, &step_ids)
        } else if path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml")
        {
            canonical_flow_step_for_diff(&content)
        } else if path.starts_with("flows/")
            && path.contains("/function_steps/")
            && path.ends_with(".py")
        {
            Some(strip_generated_flow_function_imports(&content))
        } else {
            None
        };
        if let Some(normalized) = normalized {
            resource.payload = serde_json::json!({ "content": normalized });
        }
    }
}

fn flow_step_ids_by_folder_and_name(resources: &ResourceMap) -> HashMap<(String, String), String> {
    resources
        .iter()
        .filter_map(|(path, resource)| {
            if !(path.starts_with("flows/") && path.contains("/steps/") && path.ends_with(".yaml"))
            {
                return None;
            }
            let folder = path.split('/').nth(1)?.to_string();
            let content = resource.payload.get("content")?.as_str()?;
            let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
            let name = yaml.get("name")?.as_str()?.to_string();
            Some(((folder, name), resource.resource_id.clone()))
        })
        .collect()
}

fn canonical_flow_config_for_diff(
    path: &str,
    content: &str,
    step_ids: &HashMap<(String, String), String>,
) -> Option<String> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let name = yaml
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let description = yaml
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let mut start_step = yaml
        .get("start_step")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if !start_step.starts_with("STEP-")
        && let Some(folder) = path.split('/').nth(1)
        && let Some(id) = step_ids.get(&(folder.to_string(), start_step.clone()))
    {
        start_step = id.clone();
    }
    Some(format!(
        "name: {name}\ndescription: {description}\nstart_step: {start_step}\n"
    ))
}

fn canonical_flow_step_for_diff(content: &str) -> Option<String> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    let step_type = yaml
        .get("step_type")
        .and_then(|value| value.as_str())
        .unwrap_or("advanced_step");
    if step_type == "default_step" {
        Some(canonical_default_step_for_diff(&yaml))
    } else {
        Some(canonical_advanced_step_for_diff(&yaml))
    }
}

fn canonical_advanced_step_for_diff(yaml: &serde_yaml::Value) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let asr = yaml.get("asr_biasing").or_else(|| yaml.get("asrBiasing"));
    let dtmf = yaml.get("dtmf_config").or_else(|| yaml.get("dtmfConfig"));
    let mut out = String::new();
    out.push_str("step_type: advanced_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("asr_biasing:\n");
    for (key, value) in [
        (
            "is_enabled",
            yaml_bool_value(asr, &["is_enabled", "isEnabled"], false),
        ),
        (
            "alphanumeric",
            yaml_bool_value(asr, &["alphanumeric"], false),
        ),
        (
            "name_spelling",
            yaml_bool_value(asr, &["name_spelling", "nameSpelling"], false),
        ),
        ("numeric", yaml_bool_value(asr, &["numeric"], false)),
        (
            "party_size",
            yaml_bool_value(asr, &["party_size", "partySize"], false),
        ),
        (
            "precise_date",
            yaml_bool_value(asr, &["precise_date", "preciseDate"], false),
        ),
        (
            "relative_date",
            yaml_bool_value(asr, &["relative_date", "relativeDate"], false),
        ),
        (
            "single_number",
            yaml_bool_value(asr, &["single_number", "singleNumber"], false),
        ),
        ("time", yaml_bool_value(asr, &["time"], false)),
        ("yes_no", yaml_bool_value(asr, &["yes_no", "yesNo"], false)),
        ("address", yaml_bool_value(asr, &["address"], false)),
    ] {
        out.push_str(&format!("  {key}: {value}\n"));
    }
    let keywords = yaml_string_sequence(asr.and_then(|value| {
        value
            .get("custom_keywords")
            .or_else(|| value.get("customKeywords"))
    }));
    if keywords.is_empty() {
        out.push_str("  custom_keywords: []\n");
    } else {
        out.push_str("  custom_keywords:\n");
        for keyword in keywords {
            out.push_str(&format!("  - {keyword}\n"));
        }
    }
    out.push_str("dtmf_config:\n");
    out.push_str(&format!(
        "  is_enabled: {}\n",
        yaml_bool_value(dtmf, &["is_enabled", "isEnabled"], false)
    ));
    out.push_str(&format!(
        "  inter_digit_timeout: {}\n",
        yaml_i64_value(dtmf, &["inter_digit_timeout", "interDigitTimeout"], 0)
    ));
    out.push_str(&format!(
        "  max_digits: {}\n",
        yaml_i64_value(dtmf, &["max_digits", "maxDigits"], 0)
    ));
    out.push_str(&format!(
        "  end_key: '{}'\n",
        yaml_string_value_from(dtmf, &["end_key", "endKey"])
    ));
    out.push_str(&format!(
        "  collect_while_agent_speaking: {}\n",
        yaml_bool_value(
            dtmf,
            &["collect_while_agent_speaking", "collectWhileAgentSpeaking"],
            false
        )
    ));
    out.push_str(&format!(
        "  is_pii: {}\n",
        yaml_bool_value(dtmf, &["is_pii", "isPii"], false)
    ));
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn canonical_default_step_for_diff(yaml: &serde_yaml::Value) -> String {
    let name = yaml_string_value(yaml, "name");
    let prompt = yaml_string_value(yaml, "prompt");
    let mut out = String::new();
    out.push_str("step_type: default_step\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str("conditions:\n");
    if let Some(conditions) = yaml.get("conditions").and_then(|value| value.as_sequence()) {
        for condition in conditions {
            out.push_str(&format!(
                "- name: {}\n",
                yaml_string_value(condition, "name")
            ));
            out.push_str(&format!(
                "  condition_type: {}\n",
                yaml_string_value(condition, "condition_type")
            ));
            out.push_str(&format!(
                "  description: {}\n",
                yaml_string_value(condition, "description")
            ));
            let required = yaml_string_sequence(condition.get("required_entities"));
            if required.is_empty() {
                out.push_str("  required_entities: []\n");
            } else {
                out.push_str("  required_entities:\n");
                for entity in required {
                    out.push_str(&format!("  - {entity}\n"));
                }
            }
        }
    }
    let extracted = yaml_string_sequence(yaml.get("extracted_entities"));
    if extracted.is_empty() {
        out.push_str("extracted_entities: []\n");
    } else {
        out.push_str("extracted_entities:\n");
        for entity in extracted {
            out.push_str(&format!("- {entity}\n"));
        }
    }
    out.push_str(&format!("prompt: {prompt}\n"));
    out
}

fn strip_generated_flow_function_imports(content: &str) -> String {
    let mut lines = content.lines().collect::<Vec<_>>();
    while lines
        .first()
        .is_some_and(|line| line.trim().is_empty() || line.starts_with("from _gen import"))
    {
        lines.remove(0);
    }
    format!("{}\n", lines.join("\n"))
        .trim_end_matches('\n')
        .to_string()
}

fn yaml_string_value(yaml: &serde_yaml::Value, key: &str) -> String {
    yaml.get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn yaml_string_value_from(yaml: Option<&serde_yaml::Value>, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string()
}

fn yaml_bool_value(yaml: Option<&serde_yaml::Value>, keys: &[&str], default: bool) -> bool {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(default)
}

fn yaml_i64_value(yaml: Option<&serde_yaml::Value>, keys: &[&str], default: i64) -> i64 {
    keys.iter()
        .find_map(|key| {
            yaml.and_then(|value| value.get(*key))
                .and_then(|value| value.as_i64())
        })
        .unwrap_or(default)
}

fn yaml_string_sequence(yaml: Option<&serde_yaml::Value>) -> Vec<String> {
    yaml.and_then(|value| value.as_sequence())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(ToString::to_string)
        .collect()
}

fn normalize_function_references_in_rules(resources: &mut ResourceMap) {
    let replacements = resources
        .values()
        .filter(|resource| {
            resource.file_path.starts_with("functions/") && resource.file_path.ends_with(".py")
        })
        .map(|resource| {
            let name = resource
                .file_path
                .split('/')
                .next_back()
                .unwrap_or(&resource.name)
                .trim_end_matches(".py")
                .to_string();
            (resource.resource_id.clone(), name)
        })
        .filter(|(id, name)| !id.is_empty() && id != name)
        .collect::<Vec<_>>();
    if replacements.is_empty() {
        return;
    }
    let Some(rules) = resources.get_mut("agent_settings/rules.txt") else {
        return;
    };
    let Some(content) = rules
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
    else {
        return;
    };
    let mut normalized = content;
    for (id, name) in replacements {
        normalized = normalized.replace(&format!("{{{{fn:{id}}}}}"), &format!("{{{{fn:{name}}}}}"));
    }
    rules.payload["content"] = serde_json::Value::String(normalized);
}

#[derive(Debug, Clone)]
struct ReferenceNameReplacement {
    prefix: String,
    id: String,
    name: String,
}

fn apply_reference_name_replacements(
    resources: &mut ResourceMap,
    replacements: &[ReferenceNameReplacement],
) {
    if replacements.is_empty() {
        return;
    }
    for resource in resources.values_mut() {
        let Some(content) = resource
            .payload
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
        else {
            continue;
        };
        let normalized = replace_reference_ids_with_names(&content, replacements);
        if normalized != content {
            resource.payload["content"] = serde_json::Value::String(normalized);
        }
    }
}

fn replace_reference_ids_with_names(
    content: &str,
    replacements: &[ReferenceNameReplacement],
) -> String {
    let mut normalized = content.to_string();
    for replacement in replacements {
        normalized = normalized.replace(
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.id),
            &format!("{{{{{}:{}}}}}", replacement.prefix, replacement.name),
        );
    }
    normalized
}

fn reference_name_from_logical_path(logical_path: &str) -> String {
    let (_, resource_suffix) = parse_multi_resource_path(logical_path);
    let source = resource_suffix.as_deref().unwrap_or(logical_path);
    let leaf = source.rsplit('/').next().unwrap_or(source);
    Path::new(leaf)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(leaf)
        .to_string()
}

fn absolutize_deleted_diff_keys(
    root: &Path,
    diffs: &mut DiffMap,
    deleted_file_paths: &HashSet<String>,
) {
    if deleted_file_paths.is_empty() {
        return;
    }
    let mut replacements = Vec::new();
    for path in deleted_file_paths {
        if let Some(diff) = diffs.shift_remove(path) {
            replacements.push((root.join(path).to_string_lossy().to_string(), diff));
        }
    }
    for (path, diff) in replacements {
        diffs.insert(path, diff);
    }
}

fn compute_modified_files_against_snapshot(
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
) -> Result<Vec<String>, CoreError> {
    let kept_paths = flatten_discovered_paths(kept_resources);
    let mut modified_file_paths = HashSet::new();
    let mut modified_logical_paths = HashSet::new();
    for logical_path in kept_paths {
        let (file_path, _) = parse_multi_resource_path(&logical_path);
        let (hash_path, expected_hash) =
            if let Some(expected_hash) = snapshot_hashes.get(&logical_path) {
                (logical_path.as_str(), expected_hash)
            } else if let Some(expected_hash) = snapshot_hashes.get(&file_path) {
                (file_path.as_str(), expected_hash)
            } else {
                continue;
            };
        let current_path = root.join(&file_path);
        let current_content = StdFileSystem
            .read_to_string(&current_path)
            .unwrap_or_default();
        let current_hash = current_status_hash_for_expected(
            hash_path,
            &current_content,
            expected_hash,
            snapshot_hashes,
        );
        if current_hash != *expected_hash {
            if hash_path == logical_path {
                modified_logical_paths.insert(logical_path);
            } else {
                modified_file_paths.insert(file_path);
            }
        }
    }
    Ok(ordered_discovered_paths_for_modifications(
        kept_resources,
        &modified_file_paths,
        &modified_logical_paths,
    ))
}

fn compute_modified_files_against_snapshot_with_replacements(
    root: &Path,
    kept_resources: &DiscoveredResourcePaths,
    snapshot_hashes: &indexmap::IndexMap<String, String>,
    replacements: &[ReferenceNameReplacement],
) -> Result<Vec<String>, CoreError> {
    if replacements.is_empty() {
        return Ok(Vec::new());
    }
    let kept_paths = flatten_discovered_paths(kept_resources);
    let mut modified_file_paths = HashSet::new();
    let mut modified_logical_paths = HashSet::new();
    for logical_path in kept_paths {
        let (file_path, _) = parse_multi_resource_path(&logical_path);
        let (hash_path, expected_hash) =
            if let Some(expected_hash) = snapshot_hashes.get(&logical_path) {
                (logical_path.as_str(), expected_hash)
            } else if let Some(expected_hash) = snapshot_hashes.get(&file_path) {
                (file_path.as_str(), expected_hash)
            } else {
                continue;
            };
        let current_path = root.join(&file_path);
        let current_content = StdFileSystem
            .read_to_string(&current_path)
            .unwrap_or_default();
        let normalized_content = replace_reference_ids_with_names(&current_content, replacements);
        if normalized_content == current_content {
            continue;
        }
        let current_hash = current_status_hash_for_expected(
            hash_path,
            &normalized_content,
            expected_hash,
            snapshot_hashes,
        );
        if current_hash != *expected_hash {
            if hash_path == logical_path {
                modified_logical_paths.insert(logical_path);
            } else {
                modified_file_paths.insert(file_path);
            }
        }
    }
    Ok(ordered_discovered_paths_for_modifications(
        kept_resources,
        &modified_file_paths,
        &modified_logical_paths,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_check_preserves_python_shaped_yaml_key_order() {
        let fs = adk_io::MemoryFileSystem::new();
        fs.write_string(
            Path::new("workspace/project.yaml"),
            "region: dev\naccount_id: acct\nproject_id: proj\nbranch_id: main\n",
        )
        .expect("write config");
        fs.write_string(
            Path::new("workspace/topics/billing_general.yaml"),
            "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n- Question about my bill\n",
        )
        .expect("write topic");
        fs.write_string(
            Path::new("workspace/config/entities.yaml"),
            "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n",
        )
        .expect("write entities");

        let service = AdkService::with_file_system(
            adk_api_client::InMemoryPlatformClient::default(),
            fs.clone(),
        );
        let changed = service
            .format_local_resources(Path::new("workspace"), &[], true)
            .expect("format check");

        assert!(changed.is_empty());
        assert_eq!(
            fs.read_to_string(Path::new("workspace/topics/billing_general.yaml"))
                .expect("read topic"),
            "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n- Question about my bill\n"
        );
        assert_eq!(
            fs.read_to_string(Path::new("workspace/config/entities.yaml"))
                .expect("read entities"),
            "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n"
        );
    }

    #[test]
    fn init_and_load_project_can_use_memory_filesystem() {
        let fs = adk_io::MemoryFileSystem::new();
        let service = AdkService::with_file_system(
            adk_api_client::InMemoryPlatformClient::default(),
            fs.clone(),
        );
        let base = Path::new("workspace");

        let config = service
            .init_project(
                base,
                "dev".to_string(),
                "acct".to_string(),
                "proj".to_string(),
            )
            .expect("init project");

        let root = base.join("acct").join("proj");
        assert_eq!(config.branch_id, "main");
        assert!(fs.is_file(&root.join(PROJECT_CONFIG_FILE)));
        assert!(fs.is_file(&root.join("_gen/decorators.py")));

        let loaded = service
            .load_project_config(&root.join("functions"))
            .expect("load project config from nested path");
        assert_eq!(loaded.account_id, "acct");
        assert_eq!(loaded.project_id, "proj");
        assert_eq!(loaded.region, "dev");
    }

    #[test]
    fn project_migrations_use_memory_filesystem() {
        let fs = adk_io::MemoryFileSystem::new();
        fs.write_string(
            Path::new("workspace/project.yaml"),
            "region: dev\naccount_id: acct\nproject_id: proj\nbranch_id: main\n",
        )
        .expect("write config");
        fs.write_string(
            Path::new("workspace/topics/nested/Hello Topic.yaml"),
            "content: Hello\n",
        )
        .expect("write legacy topic");

        let service = AdkService::with_file_system(
            adk_api_client::InMemoryPlatformClient::default(),
            fs.clone(),
        );
        service
            .load_project_config(Path::new("workspace"))
            .expect("load project config");

        let migrated = Path::new("workspace/topics/nested_hello_topic.yaml");
        assert!(fs.is_file(migrated));
        let migrated_content = fs.read_to_string(migrated).expect("read migrated topic");
        assert!(migrated_content.contains("name: nested/Hello Topic"));
        assert!(!fs.exists(Path::new("workspace/topics/nested/Hello Topic.yaml")));
        assert!(!fs.exists(Path::new("workspace/topics/nested")));
    }
}
