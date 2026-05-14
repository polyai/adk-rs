//! Domain-level tests against the same on-disk trees as Python `poly/tests/project_test.py`.
//! Uses `adk-cli/tests/fixtures/test_projects/` (synced from `adk/src/poly/tests/test_projects`).
//!
//! Each `#[test]` has a `/// Port:` or `/// Related:` rustdoc line naming the Python module path and
//! class method (`poly/tests/project_test.py` - `ClassName.method_name`).
//!
//! - No subprocess: exercises `AdkService` directly.
//! - No server: `InMemoryPlatformClient` has an empty remote so `status` / `diff` stay local.
//!
//! **Parity note:** `AdkService::discover_local_resources` mirrors Python typed logical paths
//! (per-type lists keyed by Python class names). `collect_local_resources()` remains a flat walk of
//! real files on disk.

use adk_core::AdkService;
use adk_io::{compute_hash, parse_multi_resource_path};
use adk_platform_api::InMemoryPlatformClient;
use adk_types::{DeploymentList, Resource, ResourceMap};
use base64::Engine;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_full_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../adk-cli/tests/fixtures/test_projects/test_project")
        .canonicalize()
        .expect(
            "missing fixture tree: copy Python poly/tests/test_projects into \
             adk-cli/tests/fixtures/test_projects (see adk-cli/tests/fixtures/SYNC_FROM_PYTHON_ADK.txt)",
        )
}

fn fixture_empty_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../adk-cli/tests/fixtures/test_projects/test_empty_project")
        .canonicalize()
        .expect("missing test_empty_project fixture")
}

fn service_offline() -> AdkService {
    AdkService::new(Box::new(InMemoryPlatformClient::default()))
}

fn discovered_total_count(map: &indexmap::IndexMap<String, Vec<String>>) -> usize {
    map.values().map(|v| v.len()).sum()
}

fn make_temp_project_dir() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("adk-rs-core-status-{ts}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn regular_file_names(dir: &std::path::Path) -> Vec<String> {
    let mut names = fs::read_dir(dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .filter(|name| !name.starts_with('.'))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn write_status_snapshot_from_discovered(
    project_root: &std::path::Path,
    discovered: &indexmap::IndexMap<String, Vec<String>>,
) {
    let mut resources = serde_json::Map::new();
    let mut file_paths = std::collections::BTreeSet::new();
    for (type_name, paths) in discovered {
        let Some(resource_name) = adk_core::discover::type_name_to_resource_name(type_name) else {
            continue;
        };
        let mut entries = serde_json::Map::new();
        for (idx, p) in paths.iter().enumerate() {
            file_paths.insert(parse_multi_resource_path(p).0);
            entries.insert(
                format!("{}_{}", resource_name.to_uppercase(), idx),
                serde_json::json!({
                    "resource_id": format!("{}_{}", resource_name.to_uppercase(), idx),
                    "name": p,
                    "file_path": p,
                }),
            );
        }
        resources.insert(
            resource_name.to_string(),
            serde_json::Value::Object(entries),
        );
    }
    let mut file_structure_info = serde_json::Map::new();
    for file_path in file_paths {
        let content = fs::read_to_string(project_root.join(&file_path)).unwrap_or_default();
        file_structure_info.insert(
            file_path.clone(),
            serde_json::json!({
                "type": "unknown",
                "resource_id": file_path,
                "resource_name": file_path,
                "hash": compute_hash(&content),
            }),
        );
    }
    let status = serde_json::json!({
        "resources": resources,
        "file_structure_info": file_structure_info,
        "branch_id": "main",
    });
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&status).expect("status json"));
    let gen_dir = project_root.join("_gen");
    fs::create_dir_all(&gen_dir).expect("mkdir _gen");
    fs::write(gen_dir.join(".agent_studio_config"), encoded).expect("write status snapshot");
}

fn read_status_snapshot_json(project_root: &std::path::Path) -> serde_json::Value {
    let encoded =
        fs::read(project_root.join("_gen/.agent_studio_config")).expect("read status snapshot");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("decode status snapshot");
    serde_json::from_slice(&decoded).expect("status json")
}

/// Port: `poly/tests/project_test.py` - `InitTest.test_init`
#[test]
fn load_project_config_matches_python_project_test_init() {
    let root = fixture_full_project();
    let cfg = service_offline()
        .load_project_config(&root)
        .expect("project.yaml should parse");

    assert_eq!(cfg.region, "us-1");
    assert_eq!(cfg.account_id, "test_account");
    assert_eq!(cfg.project_id, "test_project");
    assert_eq!(cfg.branch_id, "main");
}

/// Port: `poly/utils.py` - `export_decorators` and `save_imports`
#[test]
fn init_and_pull_write_python_compatible_gen_package() {
    let service = service_offline();
    let base = make_temp_project_dir();
    service
        .init_project_with_name(
            &base,
            "us-1".to_string(),
            "test-account".to_string(),
            "test-project".to_string(),
            Some("Test Project".to_string()),
        )
        .expect("init project");

    let root = base.join("test-account").join("test-project");
    let gen_dir = root.join("_gen");
    let init_py = fs::read_to_string(gen_dir.join("__init__.py")).expect("generated __init__");
    let decorators =
        fs::read_to_string(gen_dir.join("decorators.py")).expect("generated decorators");
    let conversation =
        fs::read_to_string(gen_dir.join("conversation.py")).expect("generated conversation");

    assert!(init_py.contains("from _gen.conversation import ("));
    assert!(init_py.contains("\"func_latency_control\""));
    assert!(decorators.contains("def func_description("));
    assert!(decorators.contains("def func_latency_control("));
    assert!(conversation.contains("class Conversation"));

    fs::write(gen_dir.join("stale.pyi"), "class Stale: ...\n").expect("write stale pyi");
    service.pull(&root, true).expect("pull project");

    assert!(!gen_dir.join("stale.pyi").exists());
    assert!(gen_dir.join(".agent_studio_config").exists());
    assert!(gen_dir.join("sms.py").exists());
    let status = read_status_snapshot_json(&root);
    assert_eq!(
        status.get("region").and_then(|value| value.as_str()),
        Some("us-1")
    );
    assert_eq!(
        status.get("account_id").and_then(|value| value.as_str()),
        Some("test-account")
    );
    assert_eq!(
        status.get("project_id").and_then(|value| value.as_str()),
        Some("test-project")
    );
    assert_eq!(
        status.get("project_name").and_then(|value| value.as_str()),
        Some("Test Project")
    );
    assert!(
        status
            .get("last_updated")
            .and_then(|value| value.as_str())
            .is_some()
    );
    assert!(
        status
            .get("resources")
            .and_then(serde_json::Value::as_object)
            .is_some()
    );
    assert!(
        status
            .get("file_structure_info")
            .and_then(serde_json::Value::as_object)
            .is_some()
    );
    assert!(
        status
            .get("migration_flags")
            .and_then(serde_json::Value::as_array)
            .is_some()
    );
}

/// Port: `poly/utils.py` - `_gen` package template files
#[test]
fn init_python_gen_package_matches_synced_fixture_files() {
    let service = service_offline();
    let base = make_temp_project_dir();
    service
        .init_project_with_name(
            &base,
            "us-1".to_string(),
            "test-account".to_string(),
            "test-project".to_string(),
            Some("Test Project".to_string()),
        )
        .expect("init project");

    let actual_gen = base.join("test-account").join("test-project").join("_gen");
    let expected_gen = fixture_full_project().join("_gen");
    let actual_names = regular_file_names(&actual_gen);
    let expected_names = regular_file_names(&expected_gen);
    assert_eq!(actual_names, expected_names);

    for file_name in expected_names {
        let actual = fs::read_to_string(actual_gen.join(&file_name))
            .unwrap_or_else(|err| panic!("read generated {file_name}: {err}"));
        let expected = fs::read_to_string(expected_gen.join(&file_name))
            .unwrap_or_else(|err| panic!("read fixture {file_name}: {err}"));
        assert_eq!(actual, expected, "{file_name} should match Python fixture");
    }
}

/// Port: `poly/tests/migration_test.py` - `TestMigrateLegacyTopicFiles.test_migrates_nested_subdirectory_topics`
#[test]
fn load_project_config_migrates_legacy_topic_files_and_persists_flag() {
    let service = service_offline();
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project config");
    fs::create_dir_all(root.join("topics/Billing")).expect("mkdir nested topics");
    fs::write(
        root.join("topics/Topic Name.yaml"),
        "enabled: true\ncontent: hello\n",
    )
    .expect("write legacy topic");
    fs::write(
        root.join("topics/Billing/Refunds.yaml"),
        "enabled: false\ncontent: refunds\n",
    )
    .expect("write nested legacy topic");

    service
        .load_project_config(&root)
        .expect("load project config runs migrations");

    assert!(!root.join("topics/Topic Name.yaml").exists());
    assert!(!root.join("topics/Billing/Refunds.yaml").exists());
    assert!(!root.join("topics/Billing").exists());

    let flat_topic =
        fs::read_to_string(root.join("topics/topic_name.yaml")).expect("migrated flat topic");
    assert!(flat_topic.contains("name: Topic Name"));
    assert!(flat_topic.contains("enabled: true"));
    let nested_topic = fs::read_to_string(root.join("topics/billing_refunds.yaml"))
        .expect("migrated nested topic");
    assert!(nested_topic.contains("name: Billing/Refunds"));
    assert!(nested_topic.contains("enabled: false"));

    let status = read_status_snapshot_json(&root);
    assert!(
        status
            .get("migration_flags")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|flags| flags
                .iter()
                .any(|flag| flag.as_str() == Some("migrated_legacy_topic_files")))
    );
}

/// Port: `poly/tests/migration_test.py` - `TestMigrateLegacyTopicFiles.test_duplicate_clean_names_raises`
#[test]
fn load_project_config_errors_when_legacy_topic_names_clean_to_same_file() {
    let service = service_offline();
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project config");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/Topic-A.yaml"), "enabled: true\n")
        .expect("write first legacy topic");
    fs::write(root.join("topics/Topic A.yaml"), "enabled: true\n")
        .expect("write duplicate legacy topic");

    let error = service
        .load_project_config(&root)
        .expect_err("duplicate cleaned topic names should error");

    assert!(error.to_string().contains("topic_a"));
    assert!(root.join("topics/Topic-A.yaml").exists());
    assert!(root.join("topics/Topic A.yaml").exists());
}

/// Port: `poly/tests/project_test.py` - `DiscoverLocalResourcesTest.test_discover_local_resources`
/// (typed logical paths, same counts and paths as Python for this fixture).
#[test]
fn discover_local_resources_matches_python_discover_local_resources_test() {
    let root = fixture_full_project();
    let map = service_offline().discover_local_resources(&root);

    let speech = "voice/speech_recognition";

    assert_eq!(map.get("Entity").map(|v| v.len()), Some(6));
    let mut entities = map.get("Entity").expect("Entity").clone();
    entities.sort();
    assert_eq!(
        entities,
        vec![
            "config/entities.yaml/entities/confirmation_status",
            "config/entities.yaml/entities/customer_name",
            "config/entities.yaml/entities/date",
            "config/entities.yaml/entities/email",
            "config/entities.yaml/entities/party_size",
            "config/entities.yaml/entities/phone_number",
        ]
    );

    let mut flows = map.get("FlowConfig").expect("FlowConfig").clone();
    flows.sort();
    assert_eq!(
        flows,
        vec![
            "flows/test_flow/flow_config.yaml",
            "flows/test_flow_with_punctuation/flow_config.yaml",
        ]
    );

    assert_eq!(
        map.get("VoiceDisclaimerMessage").cloned(),
        Some(vec![format!(
            "voice/configuration.yaml/disclaimer_messages"
        )])
    );
    assert_eq!(
        map.get("VoiceGreeting").cloned(),
        Some(vec!["voice/configuration.yaml/greeting".into()])
    );
    assert_eq!(
        map.get("VoiceStylePrompt").cloned(),
        Some(vec!["voice/configuration.yaml/style_prompt".into()])
    );
    assert_eq!(
        map.get("ChatGreeting").cloned(),
        Some(vec!["chat/configuration.yaml/greeting".into()])
    );
    assert_eq!(
        map.get("ChatStylePrompt").cloned(),
        Some(vec!["chat/configuration.yaml/style_prompt".into()])
    );

    assert_eq!(
        map.get("SettingsPersonality").cloned(),
        Some(vec!["agent_settings/personality.yaml".into()])
    );
    assert_eq!(
        map.get("SettingsRole").cloned(),
        Some(vec!["agent_settings/role.yaml".into()])
    );
    assert_eq!(
        map.get("SettingsRules").cloned(),
        Some(vec!["agent_settings/rules.txt".into()])
    );

    assert_eq!(map.get("Function").map(|v| v.len()), Some(13));
    assert_eq!(map.get("FlowStep").map(|v| v.len()), Some(9));
    assert_eq!(map.get("FunctionStep").map(|v| v.len()), Some(2));

    assert_eq!(
        map.get("ExperimentalConfig").cloned(),
        Some(vec!["agent_settings/experimental_config.json".into()])
    );

    assert_eq!(map.get("SMSTemplate").map(|v| v.len()), Some(2));
    let mut sms = map.get("SMSTemplate").expect("SMSTemplate").clone();
    sms.sort();
    assert_eq!(
        sms,
        vec![
            "config/sms_templates.yaml/sms_templates/test_template_1",
            "config/sms_templates.yaml/sms_templates/test_template_2",
        ]
    );

    assert_eq!(map.get("Variable").map(|v| v.len()), Some(3));
    let mut vars = map.get("Variable").expect("Variable").clone();
    vars.sort();
    assert_eq!(
        vars,
        vec![
            "variables/customer_name",
            "variables/data_processed",
            "variables/payment_success",
        ]
    );

    assert_eq!(map.get("KeyphraseBoosting").map(|v| v.len()), Some(3));
    let mut kp = map
        .get("KeyphraseBoosting")
        .expect("KeyphraseBoosting")
        .clone();
    kp.sort();
    assert_eq!(
        kp,
        vec![
            format!("{speech}/keyphrase_boosting.yaml/keyphrases/PolyAI"),
            format!("{speech}/keyphrase_boosting.yaml/keyphrases/check_in"),
            format!("{speech}/keyphrase_boosting.yaml/keyphrases/reservation"),
        ]
    );

    assert_eq!(map.get("TranscriptCorrection").map(|v| v.len()), Some(2));
    let mut tc = map
        .get("TranscriptCorrection")
        .expect("TranscriptCorrection")
        .clone();
    tc.sort();
    assert_eq!(
        tc,
        vec![
            format!("{speech}/transcript_corrections.yaml/corrections/Email_domain_fix"),
            format!("{speech}/transcript_corrections.yaml/corrections/Number_normalization"),
        ]
    );

    assert_eq!(
        map.get("AsrSettings").cloned(),
        Some(vec![format!("{speech}/asr_settings.yaml")])
    );
}

/// Port: `poly/tests/project_test.py` - `DiscoverLocalResourcesTest.test_discover_local_resources`
/// (flat file inventory subset).
#[test]
fn collect_local_resources_includes_core_paths_from_fixture_tree() {
    let root = fixture_full_project();
    let map = service_offline()
        .collect_local_resources(&root)
        .expect("collect");

    for rel in [
        "topics/topic_1.yaml",
        "topics/topic_2.yaml",
        "config/entities.yaml",
        "functions/validate_email.py",
        "flows/test_flow/flow_config.yaml",
        "flows/test_flow_with_punctuation/flow_config.yaml",
        "agent_settings/experimental_config.json",
        "voice/configuration.yaml",
        "chat/configuration.yaml",
        "test_project.json",
    ] {
        assert!(
            map.contains_key(rel),
            "expected path `{rel}` in collected resources"
        );
    }
}

/// Related: `poly/tests/project_test.py` - `DiscoverLocalResourcesTest.test_discover_local_resources`
///
/// Python checks per-type counts (entities, functions, ...); this is a **Rust-only** flat file count
/// band over the same tree so fixture drift is caught until typed discovery exists.
#[test]
fn collect_local_resources_counts_files_like_flat_inventory() {
    let root = fixture_full_project();
    let map = service_offline()
        .collect_local_resources(&root)
        .expect("collect");

    assert!(
        map.len() >= 45 && map.len() <= 55,
        "unexpected file count {}; fixture inventory may have changed",
        map.len()
    );
}

/// Port: `poly/tests/project_test.py` - `DiscoverLocalResourcesTest.test_discover_local_resources_empty_project`
///
/// Typed discovery: every resource-type list is empty. The flat file scan still sees
/// `empty_project.json` on disk.
#[test]
fn discover_empty_fixture_typed_lists_all_empty() {
    let root = fixture_empty_project();
    let map = service_offline().discover_local_resources(&root);
    for (_k, paths) in map {
        assert!(
            paths.is_empty(),
            "expected empty discovery list for empty fixture"
        );
    }
}

#[test]
fn discover_empty_fixture_flat_collect_sees_only_snapshot_json() {
    let root = fixture_empty_project();
    let map = service_offline()
        .collect_local_resources(&root)
        .expect("collect");

    assert_eq!(map.len(), 1);
    assert!(map.contains_key("empty_project.json"));
}

/// Port: `poly/tests/project_test.py` - `FindNewKeptDeletedTest.test_find_new_kept_deleted_nothing_changed`
#[test]
fn typed_find_new_kept_deleted_nothing_changed() {
    let root = fixture_full_project();
    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    let changes = service.find_new_kept_deleted(&discovered, &discovered);

    assert!(changes.new_resources.values().all(std::vec::Vec::is_empty));
    assert!(
        changes
            .deleted_resources
            .values()
            .all(std::vec::Vec::is_empty)
    );
    assert_eq!(
        discovered_total_count(&changes.kept_resources),
        discovered_total_count(&discovered)
    );
}

/// Port: `poly/tests/project_test.py` - `FindNewKeptDeletedTest.test_find_new_kept_deleted_new_resource`
#[test]
fn typed_find_new_kept_deleted_new_resource() {
    let root = fixture_full_project();
    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    let mut existing = discovered.clone();

    let topic_path = "topics/topic_1.yaml".to_string();
    existing
        .get_mut("Topic")
        .expect("Topic list should exist")
        .retain(|p| p != &topic_path);

    let changes = service.find_new_kept_deleted(&discovered, &existing);

    assert!(
        changes
            .deleted_resources
            .values()
            .all(std::vec::Vec::is_empty)
    );
    assert_eq!(
        discovered_total_count(&changes.kept_resources),
        discovered_total_count(&discovered) - 1
    );
    assert_eq!(
        changes
            .new_resources
            .get("Topic")
            .cloned()
            .unwrap_or_default(),
        vec![topic_path]
    );
}

/// Port: `poly/tests/project_test.py` - `FindNewKeptDeletedTest.test_find_new_kept_deleted_deleted_resource`
#[test]
fn typed_find_new_kept_deleted_deleted_resource() {
    let root = fixture_full_project();
    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    let mut existing = discovered.clone();

    let extra = "functions/extra_function.py".to_string();
    existing
        .get_mut("Function")
        .expect("Function list should exist")
        .push(extra.clone());

    let changes = service.find_new_kept_deleted(&discovered, &existing);

    assert!(changes.new_resources.values().all(std::vec::Vec::is_empty));
    assert_eq!(
        discovered_total_count(&changes.kept_resources),
        discovered_total_count(&discovered)
    );
    assert_eq!(
        changes
            .deleted_resources
            .get("Function")
            .cloned()
            .unwrap_or_default(),
        vec![extra]
    );
}

/// Port: `poly/tests/project_test.py` - `FindNewKeptDeletedTest.test_find_new_kept_deleted_mixed_changes`
#[test]
fn typed_find_new_kept_deleted_mixed_changes() {
    let root = fixture_full_project();
    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    let mut existing = discovered.clone();

    let topic_path = "topics/topic_1.yaml".to_string();
    existing
        .get_mut("Topic")
        .expect("Topic list should exist")
        .retain(|p| p != &topic_path);
    let extra = "functions/extra_function.py".to_string();
    existing
        .get_mut("Function")
        .expect("Function list should exist")
        .push(extra.clone());

    let changes = service.find_new_kept_deleted(&discovered, &existing);

    assert_eq!(
        changes
            .new_resources
            .get("Topic")
            .cloned()
            .unwrap_or_default(),
        vec![topic_path]
    );
    assert_eq!(
        changes
            .deleted_resources
            .get("Function")
            .cloned()
            .unwrap_or_default(),
        vec![extra]
    );
    assert_eq!(
        discovered_total_count(&changes.kept_resources),
        discovered_total_count(&discovered) - 1
    );
}

/// Related: Python status path classification uses local snapshot state, not "empty remote = all new".
#[test]
fn status_uses_typed_snapshot_for_new_and_deleted_when_status_file_exists() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    let summary = service.status(root.as_path()).expect("status");
    assert!(
        summary.new_files.is_empty(),
        "expected no new files when snapshot matches local"
    );
    assert!(
        summary.deleted_files.is_empty(),
        "expected no deleted files when snapshot matches local"
    );
    assert!(summary.modified_files.is_empty());
}

#[test]
fn diff_uses_typed_snapshot_for_no_changes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    let diffs = service.diff(root.as_path(), &[], None, None).expect("diff");
    assert!(
        diffs.is_empty(),
        "expected no diffs when snapshot and local discovery match"
    );
}

#[test]
fn diff_uses_typed_snapshot_to_limit_changed_files() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic 1");
    fs::write(root.join("topics/topic_2.yaml"), "name: Topic 2\n").expect("write topic 2");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    let mut existing = discovered.clone();
    existing
        .get_mut("Topic")
        .expect("Topic list should exist")
        .retain(|p| p != "topics/topic_1.yaml");
    write_status_snapshot_from_discovered(&root, &existing);

    let diffs = service.diff(root.as_path(), &[], None, None).expect("diff");
    assert_eq!(diffs.len(), 1, "expected single changed file diff");
    assert!(diffs.contains_key("topics/topic_1.yaml"));
}

#[test]
fn status_uses_typed_snapshot_for_modified_files_when_file_changes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1 Updated\n").expect("update topic");
    let summary = service.status(root.as_path()).expect("status");

    assert_eq!(summary.modified_files, vec!["topics/topic_1.yaml"]);
    assert!(summary.new_files.is_empty());
    assert!(summary.deleted_files.is_empty());
}

#[test]
fn status_uses_typed_snapshot_for_mixed_changes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic 1");
    fs::write(
        root.join("functions/old.py"),
        "def old(conv):\n    return 'old'\n",
    )
    .expect("write old function");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1 modified\n")
        .expect("modify topic 1");
    fs::write(root.join("topics/topic_2.yaml"), "name: Topic 2\n").expect("new topic 2");
    fs::remove_file(root.join("functions/old.py")).expect("delete old function");

    let summary = service.status(root.as_path()).expect("status");
    assert_eq!(summary.modified_files, vec!["topics/topic_1.yaml"]);
    assert_eq!(summary.new_files, vec!["topics/topic_2.yaml"]);
    assert_eq!(summary.deleted_files, vec!["functions/old.py"]);
}

#[test]
fn diff_can_filter_specific_changed_files_with_snapshot_classification() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic 1");
    fs::write(
        root.join("functions/old.py"),
        "def old(conv):\n    return 'old'\n",
    )
    .expect("write old function");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    fs::write(root.join("topics/topic_2.yaml"), "name: Topic 2\n").expect("new topic 2");
    fs::remove_file(root.join("functions/old.py")).expect("delete old function");

    let diffs = service
        .diff(
            root.as_path(),
            &[String::from("topics/topic_2.yaml")],
            None,
            None,
        )
        .expect("diff");
    assert_eq!(diffs.len(), 1);
    assert!(diffs.contains_key("topics/topic_2.yaml"));
}

#[test]
fn typed_resource_lifecycle_preserves_existing_ids_and_generates_new() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1\n").expect("write topic 1");

    let service = service_offline();
    let discovered = service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);

    fs::write(root.join("topics/topic_2.yaml"), "name: Topic 2\n").expect("write topic 2");
    let lifecycle = service
        .typed_resource_lifecycle(root.as_path())
        .expect("typed lifecycle");

    let existing = lifecycle
        .iter()
        .find(|r| r.file_path == "topics/topic_1.yaml")
        .expect("topic_1 lifecycle");
    assert!(existing.is_existing);
    assert!(existing.resource_id.starts_with("TOPICS_"));

    let generated = lifecycle
        .iter()
        .find(|r| r.file_path == "topics/topic_2.yaml")
        .expect("topic_2 lifecycle");
    assert!(!generated.is_existing);
    assert!(generated.resource_id.starts_with("topic-"));
}

/// Related: `poly/tests/project_test.py` - `ProjectStatusTest.test_project_status_new_resource`
///
/// Python marks **one** path as new (topic) by mutating in-memory `PROJECT_DATA` vs disk. Here the
/// in-memory platform remote is **empty**, so every collected file is "new" (same classification,
/// maximal case). See also `ProjectStatusTest` for no-changes / modified / deleted variants (not
/// ported yet).
#[test]
fn status_with_empty_remote_marks_full_fixture_files_as_new() {
    let root = fixture_full_project();
    let summary = service_offline().status(root.as_path()).expect("status");

    assert!(
        summary.new_files.len() >= 45,
        "expected many new files vs empty remote, got {}",
        summary.new_files.len()
    );
    assert!(summary.modified_files.is_empty());
    assert!(summary.deleted_files.is_empty());
}

/// Related: `poly/tests/project_test.py` - `GetDiffsTest.test_get_diffs_new_resource`
///
/// Python builds a diff for a single "new" topic by dropping it from project state. Empty remote
/// here means every local file differs from remote (same idea as a broad `get_diffs` over new work).
/// See `GetDiffsTest` for no-changes, deleted, modified, mixed (not ported yet).
#[test]
fn diff_with_empty_remote_is_non_empty_for_full_fixture() {
    let root = fixture_full_project();
    let diffs = service_offline()
        .diff(root.as_path(), &[], None, None)
        .expect("diff");

    assert!(
        diffs.len() >= 45,
        "expected many diffs vs empty remote, got {}",
        diffs.len()
    );
}

#[test]
fn diff_named_state_local_vs_remote_reports_changes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Local Topic\n").expect("write local topic");

    let mut remote: ResourceMap = ResourceMap::new();
    remote.insert(
        "topics/topic_1.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-topic_1".to_string(),
            name: "Topic 1".to_string(),
            file_path: "topics/topic_1.yaml".to_string(),
            payload: serde_json::json!({
                "content": "name: Remote Topic\n"
            }),
        },
    );
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(remote)));
    let diffs = service
        .diff(
            root.as_path(),
            &[],
            Some("sandbox".to_string()),
            Some("local".to_string()),
        )
        .expect("diff");
    assert!(
        !diffs.is_empty(),
        "expected at least one local-vs-remote diff entry"
    );
    assert!(diffs.contains_key("topics/topic_1.yaml"));
}

#[test]
fn diff_named_state_with_file_filter_applies_glob() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(root.join("topics/topic_1.yaml"), "name: Local Topic\n").expect("write local topic");
    fs::write(
        root.join("functions/test.py"),
        "def test(conv):\n    return 'local'\n",
    )
    .expect("write local function");

    let mut remote: ResourceMap = ResourceMap::new();
    remote.insert(
        "topics/topic_1.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-topic_1".to_string(),
            name: "Topic 1".to_string(),
            file_path: "topics/topic_1.yaml".to_string(),
            payload: serde_json::json!({"content": "name: Remote Topic\n"}),
        },
    );
    remote.insert(
        "functions/test.py".to_string(),
        Resource {
            resource_id: "FUNCTION-test".to_string(),
            name: "test".to_string(),
            file_path: "functions/test.py".to_string(),
            payload: serde_json::json!({"content": "def test(conv):\n    return 'remote'\n"}),
        },
    );
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(remote)));
    let diffs = service
        .diff(
            root.as_path(),
            &[String::from("topics/*")],
            Some("sandbox".to_string()),
            Some("local".to_string()),
        )
        .expect("diff");
    assert_eq!(diffs.len(), 1);
    assert!(diffs.contains_key("topics/topic_1.yaml"));
}

#[test]
fn revert_changes_restores_remote_content_for_selected_files() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(root.join("topics/topic_1.yaml"), "name: Local Topic\n").expect("write local topic");
    fs::write(
        root.join("functions/test.py"),
        "def test(conv):\n    return 'local'\n",
    )
    .expect("write local function");

    let mut remote: ResourceMap = ResourceMap::new();
    remote.insert(
        "topics/topic_1.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-topic_1".to_string(),
            name: "Topic 1".to_string(),
            file_path: "topics/topic_1.yaml".to_string(),
            payload: serde_json::json!({"content": "name: Remote Topic\n"}),
        },
    );
    remote.insert(
        "functions/test.py".to_string(),
        Resource {
            resource_id: "FUNCTION-test".to_string(),
            name: "test".to_string(),
            file_path: "functions/test.py".to_string(),
            payload: serde_json::json!({"content": "def test(conv):\n    return 'remote'\n"}),
        },
    );
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(remote)));
    let selected = vec![
        root.join("topics/topic_1.yaml")
            .to_string_lossy()
            .to_string(),
    ];

    let reverted = service
        .revert_changes(root.as_path(), &selected)
        .expect("revert");

    assert_eq!(reverted.len(), 1);
    assert_eq!(
        fs::read_to_string(root.join("topics/topic_1.yaml")).expect("topic file"),
        "name: Remote Topic\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("functions/test.py")).expect("function file"),
        "def test(conv):\n    return 'local'\n"
    );
}

#[test]
fn push_dry_run_and_validation_flags_are_respected() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::write(root.join("sample.json"), "{\"b\":2,\"a\":1}").expect("write json");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(
        root.join("topics/bad.yaml"),
        "name: bad\ncontent: [unterminated\n",
    )
    .expect("write invalid yaml");

    let service = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let dry_run = service
        .push(root.as_path(), false, true, true)
        .expect("dry run push");
    assert!(dry_run.success);
    assert!(dry_run.message.contains("Dry run completed"));

    let error = service
        .push(root.as_path(), false, false, false)
        .expect_err("validation should fail");
    let error = error.to_string();
    assert!(error.contains("Error reading resource bad at"));
    assert!(error.contains("Error loading YAML file:"));
}

#[test]
fn push_force_bypasses_conflict_marker_guard() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(
        root.join("topics/topic_1.yaml"),
        "<<<<<<< ours\nname: Ours\n=======\nname: Theirs\n>>>>>>> theirs\n",
    )
    .expect("write conflicted file");

    let service = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let blocked = service
        .push(root.as_path(), false, true, false)
        .expect("push result");
    assert!(!blocked.success);
    assert!(blocked.message.contains("Merge conflicts detected"));

    let forced = service
        .push(root.as_path(), true, true, false)
        .expect("forced push result");
    assert!(forced.success);
}

#[test]
fn pull_force_controls_overwrite_of_conflict_files() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    let target = root.join("topics/topic_1.yaml");
    fs::write(
        &target,
        "<<<<<<< ours\nname: Ours\n=======\nname: Theirs\n>>>>>>> theirs\n",
    )
    .expect("write conflicted file");

    let mut remote: ResourceMap = ResourceMap::new();
    remote.insert(
        "topics/topic_1.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-topic_1".to_string(),
            name: "Topic 1".to_string(),
            file_path: "topics/topic_1.yaml".to_string(),
            payload: serde_json::json!({"content": "name: Remote Topic\n"}),
        },
    );
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(remote)));

    let conflicts = service
        .pull(root.as_path(), false)
        .expect("pull without force");
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].contains("topics/topic_1.yaml"));
    assert!(
        fs::read_to_string(&target)
            .expect("target")
            .contains("<<<<<<<")
    );

    let conflicts_after_force = service.pull(root.as_path(), true).expect("pull force");
    assert!(conflicts_after_force.is_empty());
    assert_eq!(
        fs::read_to_string(&target).expect("target"),
        "name: Remote Topic\n"
    );
    let status = read_status_snapshot_json(&root);
    assert_eq!(
        status.get("region").and_then(serde_json::Value::as_str),
        Some("eu-west-1")
    );
    let file_info = status
        .pointer("/file_structure_info/topics~1topic_1.yaml")
        .expect("topic file structure info");
    assert_eq!(
        file_info.get("type").and_then(serde_json::Value::as_str),
        Some("topics")
    );
    assert_eq!(
        file_info
            .get("resource_id")
            .and_then(serde_json::Value::as_str),
        Some("TOPIC-topic_1")
    );
    assert_eq!(
        file_info
            .get("resource_name")
            .and_then(serde_json::Value::as_str),
        Some("Topic 1")
    );
    assert!(
        file_info
            .get("hash")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
}

#[test]
fn pull_force_deletes_local_only_resources_and_empty_flow_folders() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::create_dir_all(root.join("flows/old_flow/steps")).expect("mkdir old flow");
    fs::write(root.join("topics/local_only.yaml"), "name: Local Only\n")
        .expect("write local-only topic");
    fs::write(
        root.join("flows/old_flow/flow_config.yaml"),
        "name: Old Flow\n",
    )
    .expect("write local-only flow");
    fs::write(
        root.join("flows/old_flow/steps/start.yaml"),
        "name: Start\n",
    )
    .expect("write local-only step");

    let mut remote: ResourceMap = ResourceMap::new();
    remote.insert(
        "topics/remote_topic.yaml".to_string(),
        Resource {
            resource_id: "TOPIC-remote".to_string(),
            name: "Remote Topic".to_string(),
            file_path: "topics/remote_topic.yaml".to_string(),
            payload: serde_json::json!({"content": "name: Remote Topic\n"}),
        },
    );
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(remote)));

    let conflicts = service.pull(root.as_path(), true).expect("pull force");

    assert!(conflicts.is_empty());
    assert!(root.join("topics/remote_topic.yaml").exists());
    assert!(!root.join("topics/local_only.yaml").exists());
    assert!(!root.join("flows/old_flow").exists());
}

#[test]
fn diff_after_only_uses_previous_deployment_version() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    let current_hash = "abcdef123456";
    let previous_hash = "fedcba987654";
    let mut named = indexmap::IndexMap::new();
    named.insert(
        current_hash.to_string(),
        ResourceMap::from([(
            "topics/topic_1.yaml".to_string(),
            Resource {
                resource_id: "TOPIC-topic_1".to_string(),
                name: "Topic 1".to_string(),
                file_path: "topics/topic_1.yaml".to_string(),
                payload: serde_json::json!({"content": "name: Current Topic\n"}),
            },
        )]),
    );
    named.insert(
        previous_hash.to_string(),
        ResourceMap::from([(
            "topics/topic_1.yaml".to_string(),
            Resource {
                resource_id: "TOPIC-topic_1".to_string(),
                name: "Topic 1".to_string(),
                file_path: "topics/topic_1.yaml".to_string(),
                payload: serde_json::json!({"content": "name: Previous Topic\n"}),
            },
        )]),
    );
    let deployments = DeploymentList {
        versions: vec![
            serde_json::json!({"version_hash": current_hash}),
            serde_json::json!({"version_hash": previous_hash}),
        ],
        active_deployment_hashes: indexmap::IndexMap::new(),
    };
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_named_resources(
        ResourceMap::new(),
        named,
        deployments,
    )));
    let diffs = service
        .diff(root.as_path(), &[], None, Some(current_hash.to_string()))
        .expect("diff");
    assert!(diffs.contains_key("topics/topic_1.yaml"));
}

#[test]
fn diff_after_only_errors_when_previous_version_missing() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    let deployments = DeploymentList {
        versions: vec![serde_json::json!({"version_hash": "abcdef123456"})],
        active_deployment_hashes: indexmap::IndexMap::new(),
    };
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_named_resources(
        ResourceMap::new(),
        indexmap::IndexMap::new(),
        deployments,
    )));
    let error = service
        .diff(root.as_path(), &[], None, Some("abcdef123".to_string()))
        .expect_err("should fail with no previous deployment");
    assert!(error.to_string().contains("No previous version found."));
}
