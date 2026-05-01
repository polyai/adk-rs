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
use adk_platform_api::InMemoryPlatformClient;
use std::path::PathBuf;

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
