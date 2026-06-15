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

#![allow(clippy::disallowed_methods)]

use adk_api_client::{ApiError, InMemoryPlatformClient, PlatformClient, ProjectionSnapshot};
use adk_io::{compute_hash, parse_multi_resource_path};
use adk_resources::projection_to_resource_map;
use adk_service::AdkService;
use adk_types::{
    BranchDescriptor, BranchMergeResult, ConversationDetail, ConversationListResponse,
    DeploymentList, ORDERED_TYPE_NAMES, PushResult, Resource, ResourceMap,
};
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

fn service_offline() -> AdkService<InMemoryPlatformClient> {
    AdkService::new(InMemoryPlatformClient::default())
}

fn discovered_total_count(map: &indexmap::IndexMap<String, Vec<String>>) -> usize {
    map.values().map(|v| v.len()).sum()
}

fn assert_discovered_paths(
    map: &indexmap::IndexMap<String, Vec<String>>,
    type_name: &str,
    expected: &[&str],
) {
    let mut actual = map
        .get(type_name)
        .unwrap_or_else(|| panic!("{type_name}"))
        .clone();
    actual.sort();
    let actual = actual.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(actual, expected, "{type_name} discovery paths");
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

#[cfg(unix)]
fn symlink_path(target: &std::path::Path, link: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).expect("create symlink");
}

fn regular_file_paths(dir: &std::path::Path) -> Vec<String> {
    fn visit(root: &std::path::Path, dir: &std::path::Path, paths: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("read directory") {
            let path = entry.expect("directory entry").path();
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, paths);
            } else if path.is_file() {
                paths.push(
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut paths = Vec::new();
    visit(dir, dir, &mut paths);
    paths.sort();
    paths
}

fn write_status_snapshot_from_discovered(
    project_root: &std::path::Path,
    discovered: &indexmap::IndexMap<String, Vec<String>>,
) {
    let mut resources = serde_json::Map::new();
    let mut file_paths = std::collections::BTreeSet::new();
    for (type_name, paths) in discovered {
        let Some(resource_name) =
            adk_types::descriptor_by_type_name(type_name).map(|d| d.status_resource_name)
        else {
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

fn write_status_snapshot_json(project_root: &std::path::Path, status: serde_json::Value) {
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&status).expect("status json"));
    let gen_dir = project_root.join("_gen");
    fs::create_dir_all(&gen_dir).expect("mkdir _gen");
    fs::write(gen_dir.join(".agent_studio_config"), encoded).expect("write status snapshot");
}

fn replace_remote_resource_content(client: &InMemoryPlatformClient, path: &str, content: &str) {
    let mut remote = client.pull_resources().expect("pull remote resources");
    let resource = remote.get_mut(path).expect("remote resource");
    let payload = resource.payload.as_object_mut().expect("resource payload");
    payload.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    client
        .push_resources(&remote)
        .expect("replace remote resources");
}

struct ProjectionOnlyClient {
    projection: serde_json::Value,
}

impl ProjectionOnlyClient {
    fn new(projection: serde_json::Value) -> Self {
        Self { projection }
    }
}

impl PlatformClient for ProjectionOnlyClient {
    fn pull_projection_snapshot(&self) -> Result<ProjectionSnapshot, ApiError> {
        Ok(ProjectionSnapshot {
            projection: self.projection.clone(),
            last_known_sequence: 0,
        })
    }

    fn pull_resources(&self) -> Result<ResourceMap, ApiError> {
        Ok(ResourceMap::new())
    }

    fn push_resources(&self, _resources: &ResourceMap) -> Result<PushResult, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn list_deployments(&self, _environment: &str) -> Result<DeploymentList, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn promote_deployment(
        &self,
        _deployment_id: &str,
        _target_env: &str,
        _message: &str,
    ) -> Result<serde_json::Value, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn rollback_deployment(
        &self,
        _deployment_id: &str,
        _message: &str,
    ) -> Result<serde_json::Value, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn create_chat_session(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn send_chat_message(
        &self,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn end_chat_session(&self, _payload: serde_json::Value) -> Result<serde_json::Value, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn list_conversations(
        &self,
        _limit: usize,
        _offset: usize,
    ) -> Result<ConversationListResponse, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn get_conversation(&self, _conversation_id: &str) -> Result<ConversationDetail, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn get_conversation_audio(
        &self,
        _conversation_id: &str,
        _direction: &str,
        _redacted: bool,
    ) -> Result<Vec<u8>, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn list_branches(&self) -> Result<Vec<BranchDescriptor>, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn create_branch(&self, _branch_name: &str) -> Result<String, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn delete_branch(&self, _branch_id: &str) -> Result<(), ApiError> {
        unreachable!("not needed for projection-only push tests")
    }

    fn merge_branch(
        &self,
        _deployment_message: &str,
        _conflict_resolutions: Option<Vec<serde_json::Value>>,
    ) -> Result<BranchMergeResult, ApiError> {
        unreachable!("not needed for projection-only push tests")
    }
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

#[test]
fn load_project_config_uses_status_branch_when_project_yaml_omits_branch_id() {
    let service = service_offline();
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "project_id: proj\naccount_id: test\nregion: eu-west-1\n",
    )
    .expect("write project yaml");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "branch_id": "BRANCH-feature",
            "resources": {},
            "file_structure_info": {}
        }),
    );

    let cfg = service
        .load_project_config(&root)
        .expect("project.yaml should parse");

    assert_eq!(cfg.branch_id, "BRANCH-feature");
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
    let project_yaml = fs::read_to_string(root.join("project.yaml")).expect("project yaml");
    let init_py = fs::read_to_string(gen_dir.join("__init__.py")).expect("generated __init__");
    let decorators =
        fs::read_to_string(gen_dir.join("decorators.py")).expect("generated decorators");
    let conversation =
        fs::read_to_string(gen_dir.join("conversation.pyi")).expect("generated conversation");
    let value_extraction = fs::read_to_string(gen_dir.join("value_extraction.pyi"))
        .expect("generated value extraction");

    assert!(project_yaml.contains("project_id: test-project"));
    assert!(!project_yaml.contains("branch_id:"));
    assert!(init_py.contains("from _gen.conversation import ("));
    assert!(init_py.contains("\"func_latency_control\""));
    assert!(!init_py.starts_with("# Copyright PolyAI Limited\n"));
    assert!(decorators.contains("def func_description("));
    assert!(decorators.contains("def func_latency_control("));
    assert!(!decorators.starts_with("# Copyright PolyAI Limited\n"));
    assert!(conversation.starts_with("# Copyright PolyAI Limited\n"));
    assert!(conversation.contains("class Conversation"));
    assert!(conversation.contains("EmotionKindValue: Any"));
    assert!(value_extraction.contains("class Address"));

    fs::write(gen_dir.join("stale.pyi"), "class Stale: ...\n").expect("write stale pyi");
    service.pull(&root, true).expect("pull project");

    assert!(!gen_dir.join("stale.pyi").exists());
    assert!(gen_dir.join(".agent_studio_config").exists());
    assert!(gen_dir.join("sms.pyi").exists());
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

#[test]
#[cfg(unix)]
fn pull_does_not_clean_stale_python_stubs_through_gen_child_symlink() {
    let service = service_offline();
    let base = make_temp_project_dir();
    service
        .init_project(
            &base,
            "us-1".to_string(),
            "test-account".to_string(),
            "test-project".to_string(),
        )
        .expect("init project");

    let root = base.join("test-account").join("test-project");
    let gen_dir = root.join("_gen");
    let outside_dir = base.join("outside-gen");
    fs::create_dir_all(&outside_dir).expect("outside dir");
    let escaped_stub = outside_dir.join("escaped.pyi");
    fs::write(&escaped_stub, "class Escaped: ...\n").expect("escaped stub");
    symlink_path(&outside_dir, &gen_dir.join("outside"));

    service.pull(&root, true).expect("pull project");

    assert!(
        escaped_stub.exists(),
        "stale-stub cleanup must not follow child symlinks outside _gen"
    );
}

#[test]
#[cfg(unix)]
fn pull_deduplicates_stale_python_stubs_reached_through_internal_gen_symlink() {
    let service = service_offline();
    let base = make_temp_project_dir();
    service
        .init_project(
            &base,
            "us-1".to_string(),
            "test-account".to_string(),
            "test-project".to_string(),
        )
        .expect("init project");

    let root = base.join("test-account").join("test-project");
    let gen_dir = root.join("_gen");
    let subdir = gen_dir.join("subdir");
    fs::create_dir_all(&subdir).expect("subdir");
    let stale_stub = subdir.join("stale.pyi");
    fs::write(&stale_stub, "class Stale: ...\n").expect("stale stub");
    symlink_path(&subdir, &gen_dir.join("link"));

    service.pull(&root, true).expect("pull project");

    assert!(
        !stale_stub.exists(),
        "stale-stub cleanup should delete each canonical _gen file once"
    );
}

#[test]
#[cfg(unix)]
fn init_allows_gen_directory_symlink_and_cleans_stubs_inside_target() {
    let service = service_offline();
    let base = make_temp_project_dir();
    let root = base.join("test-account").join("test-project");
    let shared_gen = base.join("shared-gen");
    fs::create_dir_all(&root).expect("project root");
    fs::create_dir_all(&shared_gen).expect("shared gen");
    let stale_stub = shared_gen.join("stale.pyi");
    fs::write(&stale_stub, "class Stale: ...\n").expect("stale stub");
    symlink_path(&shared_gen, &root.join("_gen"));

    service
        .init_project(
            &base,
            "us-1".to_string(),
            "test-account".to_string(),
            "test-project".to_string(),
        )
        .expect("init project");

    assert!(shared_gen.join("__init__.py").exists());
    assert!(
        !stale_stub.exists(),
        "stale-stub cleanup should still operate inside a symlinked _gen target"
    );
}

#[test]
fn pushed_status_snapshot_contains_python_loadable_resource_payloads() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::create_dir_all(root.join("agent_settings")).expect("mkdir agent settings");
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    fs::write(
        root.join("functions/lookup.py"),
        "from _gen import *  # <AUTO GENERATED>\n@func_description('Look up saved value.')\n@func_parameter('customer_id', 'Customer id')\ndef lookup(conv, customer_id: str):\n    conv.state.saved_value = customer_id\n    return 'ok'\n",
    )
    .expect("write function");
    fs::write(
        root.join("agent_settings/rules.txt"),
        "Always call {{fn:lookup}} when needed.",
    )
    .expect("write rules");
    fs::write(
        root.join("config/variant_attributes.yaml"),
        "variants:\n  - name: default\n    is_default: true\nattributes:\n  - name: tone\n    values:\n      default: friendly\n",
    )
    .expect("write variant attributes");
    fs::write(
        root.join("config/api_integrations.yaml"),
        "api_integrations:\n  - name: crm_lookup\n    description: CRM lookup\n",
    )
    .expect("write api integrations");
    fs::write(
        root.join("config/entities.yaml"),
        "entities:\n  - name: account_id\n    description: Account id\n    entity_type: free_text\n",
    )
    .expect("write entities");
    fs::write(
        root.join("config/handoffs.yaml"),
        "handoffs:\n  - name: support\n    description: Support handoff\n",
    )
    .expect("write handoffs");
    fs::write(
        root.join("config/sms_templates.yaml"),
        "sms_templates:\n  - name: booking_confirmed\n    body: Booking confirmed\n",
    )
    .expect("write sms templates");

    service_offline()
        .push(root.as_path(), true, true, false)
        .expect("push");
    let status = read_status_snapshot_json(&root);

    let function = status
        .pointer("/resources/functions/variables:functions~1lookup.py")
        .or_else(|| {
            status
                .get("resources")
                .and_then(|resources| resources.get("functions"))
                .and_then(serde_json::Value::as_object)
                .and_then(|functions| functions.values().next())
        })
        .expect("function status payload");
    for key in [
        "resource_id",
        "name",
        "description",
        "code",
        "parameters",
        "latency_control",
        "function_type",
        "variable_references",
    ] {
        assert!(function.get(key).is_some(), "missing function key {key}");
    }
    assert_eq!(
        function
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Look up saved value.")
    );
    let parameters = function
        .get("parameters")
        .and_then(serde_json::Value::as_array)
        .expect("function parameters");
    assert_eq!(parameters.len(), 1);
    assert_eq!(
        parameters[0]
            .get("name")
            .and_then(serde_json::Value::as_str),
        Some("customer_id")
    );
    assert_eq!(
        parameters[0]
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Customer id")
    );
    assert_eq!(
        parameters[0]
            .get("type")
            .and_then(serde_json::Value::as_str),
        Some("string")
    );
    assert!(
        parameters[0]
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id.starts_with("PARAMETER-"))
    );
    assert!(function.get("file_path").is_some());

    for (resource_name, expected_id) in [
        (
            "api_integration",
            "api_integration:api_integrations:crm_lookup",
        ),
        ("entities", "entities:entities:account_id"),
        ("handoffs", "handoffs:handoffs:support"),
        (
            "sms_templates",
            "sms_templates:sms_templates:booking_confirmed",
        ),
    ] {
        let entries = status
            .pointer(&format!("/resources/{resource_name}"))
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("missing {resource_name} status resources"));
        assert!(
            entries.contains_key(expected_id),
            "missing {expected_id} in {resource_name} status resources"
        );
    }

    let variables = status
        .pointer("/resources/variables")
        .and_then(serde_json::Value::as_object)
        .expect("variable status payloads");
    assert!(!variables.is_empty());
    assert!(
        status
            .pointer("/file_structure_info/variables~1saved_value/hash")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );

    let variant_attribute = status
        .pointer("/resources/variant_attributes")
        .and_then(serde_json::Value::as_object)
        .and_then(|attributes| attributes.values().next())
        .expect("variant attribute status payload");
    assert!(variant_attribute.get("mappings").is_some());
    assert!(variant_attribute.get("values").is_none());
}

#[test]
fn pull_does_not_conflict_on_clean_python_status_function_files() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: us-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");

    let raw_function = "def lookup(conv):\n    return 'ok'\n";
    fs::write(
        root.join("functions/lookup.py"),
        format!("from _gen import *  # <AUTO GENERATED>\n\n{raw_function}"),
    )
    .expect("write Python-formatted function");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "resources": {
                "functions": {
                    "fn-1": {
                        "resource_id": "fn-1",
                        "name": "lookup",
                        "description": "",
                        "code": raw_function,
                        "parameters": [],
                        "latency_control": {"enabled": false},
                        "function_type": "regular"
                    }
                }
            },
            "file_structure_info": {},
            "branch_id": "main"
        }),
    );

    let mut remote = ResourceMap::new();
    remote.insert(
        "functions/lookup.py".to_string(),
        Resource {
            resource_id: "fn-1".to_string(),
            name: "lookup".to_string(),
            file_path: "functions/lookup.py".to_string(),
            payload: serde_json::json!({ "content": raw_function }),
        },
    );

    let conflicts = AdkService::new(InMemoryPlatformClient::with_resources(remote))
        .pull(&root, false)
        .expect("pull");

    assert!(
        conflicts.is_empty(),
        "clean Python status snapshot should not create conflicts: {conflicts:?}"
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
    let actual_names = regular_file_paths(&actual_gen);
    let expected_names = regular_file_paths(&expected_gen);
    assert_eq!(actual_names, expected_names);

    for file_name in expected_names {
        let actual = fs::read_to_string(actual_gen.join(&file_name))
            .unwrap_or_else(|err| panic!("read generated {file_name}: {err}"));
        let expected = fs::read_to_string(expected_gen.join(&file_name))
            .unwrap_or_else(|err| panic!("read fixture {file_name}: {err}"));
        assert_eq!(actual, expected, "{file_name} should match Python fixture");
        if file_name == "__init__.py" || file_name == "decorators.py" {
            assert!(
                !actual.starts_with("# Copyright PolyAI Limited\n"),
                "{file_name} should match Python's generated header shape"
            );
        } else {
            assert!(
                actual.starts_with("# Copyright PolyAI Limited\n"),
                "{file_name} should retain the copied poly.types copyright header"
            );
        }
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

    assert_discovered_paths(
        &map,
        "ApiIntegration",
        &["config/api_integrations.yaml/api_integrations/customer_api"],
    );

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
    assert_discovered_paths(&map, "VoiceSafetyFilters", &["voice/safety_filters.yaml"]);
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
    assert_discovered_paths(&map, "ChatSafetyFilters", &["chat/safety_filters.yaml"]);
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
    assert_discovered_paths(
        &map,
        "GeneralSafetyFilters",
        &["agent_settings/safety_filters.yaml"],
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
    assert_discovered_paths(
        &map,
        "Handoff",
        &[
            "config/handoffs.yaml/handoffs/default",
            "config/handoffs.yaml/handoffs/front_desk",
            "config/handoffs.yaml/handoffs/urgent_support",
        ],
    );
    assert_discovered_paths(
        &map,
        "Variant",
        &["config/variant_attributes.yaml/variants/default"],
    );
    assert_discovered_paths(
        &map,
        "VariantAttribute",
        &[
            "config/variant_attributes.yaml/attributes/booking_status",
            "config/variant_attributes.yaml/attributes/customer_name",
            "config/variant_attributes.yaml/attributes/email_address",
            "config/variant_attributes.yaml/attributes/member_status",
        ],
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
    assert_discovered_paths(
        &map,
        "PhraseFilter",
        &[
            "voice/response_control/phrase_filtering.yaml/phrase_filtering/Block_Competitor_Names",
            "voice/response_control/phrase_filtering.yaml/phrase_filtering/Block_Profanity",
        ],
    );
    assert_discovered_paths(
        &map,
        "Pronunciation",
        &[
            "voice/response_control/pronunciations.yaml/pronunciations/0",
            "voice/response_control/pronunciations.yaml/pronunciations/1",
        ],
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
    let discovered_types = map.keys().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(discovered_types, ORDERED_TYPE_NAMES.as_slice());
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
fn status_reads_legacy_python_snapshot_without_file_paths() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(
        root.join("topics/topic_1.yaml"),
        "name: Topic 1\nenabled: true\nactions: ''\ncontent: ''\nexample_queries: []\n",
    )
    .expect("write topic");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "resources": {
                "topics": {
                    "TOPIC-topic_1": {
                        "resource_id": "TOPIC-topic_1",
                        "name": "Topic 1",
                        "enabled": true,
                        "actions": "",
                        "content": "",
                        "example_queries": []
                    }
                }
            },
            "file_structure_info": null,
            "branch_id": "main"
        }),
    );

    let summary = service_offline().status(root.as_path()).expect("status");

    assert!(summary.new_files.is_empty());
    assert!(summary.deleted_files.is_empty());
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
fn status_detects_function_changes_from_legacy_python_snapshot() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(
        root.join("functions/lookup.py"),
        "from _gen import *  # <AUTO GENERATED>\n@func_description('Looks up a customer\\'s order')\ndef lookup(conv):\n    return 'old'\n",
    )
    .expect("write function");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "resources": {
                "functions": {
                    "FUNCTION-lookup": {
                        "resource_id": "FUNCTION-lookup",
                        "name": "lookup",
                        "description": "Looks up a customer's order",
                        "code": "def lookup(conv):\n    return 'old'\n",
                        "parameters": [],
                        "latency_control": {},
                        "function_type": "global",
                        "variable_references": {}
                    }
                }
            },
            "file_structure_info": {
                "functions/lookup.py": {
                    "type": "functions",
                    "resource_id": "FUNCTION-lookup",
                    "resource_name": "lookup",
                    "hash": "stale-file-structure-hash"
                }
            },
            "branch_id": "main"
        }),
    );
    let clean_summary = service_offline().status(root.as_path()).expect("status");
    assert!(clean_summary.modified_files.is_empty());

    fs::write(
        root.join("functions/lookup.py"),
        "from _gen import *  # <AUTO GENERATED>\n@func_description('Looks up a customer\\'s order')\ndef lookup(conv):\n    return 'new'\n",
    )
    .expect("modify function");

    let summary = service_offline().status(root.as_path()).expect("status");

    assert_eq!(summary.modified_files, vec!["functions/lookup.py"]);
    assert!(summary.new_files.is_empty());
    assert!(summary.deleted_files.is_empty());
}

#[test]
fn status_normalizes_legacy_python_flow_function_imports() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("flows/booking/functions")).expect("mkdir flow functions");
    fs::write(
        root.join("flows/booking/flow_config.yaml"),
        "name: Booking\ndescription: Book things.\nstart_step: Start\n",
    )
    .expect("write flow config");
    fs::write(
        root.join("flows/booking/functions/reject.py"),
        "from _gen import *  # <AUTO GENERATED>\nfrom flows.booking.functions.shared import helper\n@func_description('Rejects a booking')\ndef reject(conv, flow):\n    return helper()\n",
    )
    .expect("write flow function");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "resources": {
                "flow_config": {
                    "FLOW-abc123": {
                        "resource_id": "FLOW-abc123",
                        "name": "Booking",
                        "description": "Book things.",
                        "start_step": "Start"
                    }
                },
                "functions": {
                    "FUNCTION-reject": {
                        "resource_id": "FUNCTION-reject",
                        "name": "reject",
                        "description": "Rejects a booking",
                        "code": "from functions.flow_abc123.shared import helper\ndef reject(conv, flow):\n    return helper()\n",
                        "parameters": [],
                        "latency_control": {},
                        "function_type": "transition",
                        "flow_id": "FLOW-abc123",
                        "flow_name": "Booking",
                        "variable_references": {}
                    }
                }
            },
            "file_structure_info": null,
            "branch_id": "main"
        }),
    );

    let summary = service_offline().status(root.as_path()).expect("status");

    assert!(summary.modified_files.is_empty(), "{summary:#?}");
    assert!(summary.new_files.is_empty());
    assert!(summary.deleted_files.is_empty());
}

#[test]
fn diff_uses_legacy_python_status_snapshot_without_remote_pull() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("functions")).expect("mkdir functions");
    fs::write(
        root.join("functions/lookup.py"),
        "from _gen import *  # <AUTO GENERATED>\n@func_description('Looks up a customer')\ndef lookup(conv):\n    return 'new'\n",
    )
    .expect("write function");
    write_status_snapshot_json(
        &root,
        serde_json::json!({
            "resources": {
                "functions": {
                    "FUNCTION-lookup": {
                        "resource_id": "FUNCTION-lookup",
                        "name": "lookup",
                        "description": "Looks up a customer",
                        "code": "def lookup(conv):\n    return 'old'\n",
                        "parameters": [],
                        "latency_control": {},
                        "function_type": "global",
                        "variable_references": {}
                    }
                }
            },
            "file_structure_info": {
                "functions/lookup.py": {
                    "type": "functions",
                    "resource_id": "FUNCTION-lookup",
                    "resource_name": "lookup",
                    "hash": "stale-file-structure-hash"
                }
            },
            "branch_id": "main"
        }),
    );

    let diffs = service_offline()
        .diff(root.as_path(), &[], None, None)
        .expect("diff");

    let diff = diffs.get("functions/lookup.py").expect("lookup diff");
    assert!(diff.contains("-    return 'old'"));
    assert!(diff.contains("+    return 'new'"));
}

#[test]
fn format_check_ignores_python_trailing_whitespace_only_differences() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(root.join("topics/topic_1.yaml"), "name: Topic 1").expect("write topic");

    let changed = service_offline()
        .format_local_resources(root.as_path(), &[], true)
        .expect("format check");

    assert!(changed.is_empty(), "{changed:#?}");
}

#[test]
fn pull_preserves_semantically_unchanged_yaml_bytes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    let local_content = "name: Billing General\nenabled: true\nactions: Transfer the caller.\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n- Question about my bill\n";
    let remote_content = "name: \"Billing General\"\nenabled: true\nactions: \"Transfer the caller.\"\ncontent: |-\n  Line one.\n  Line two.\nexample_queries:\n  - Question about my bill\n";
    fs::write(root.join("topics/billing_general.yaml"), local_content).expect("write topic");

    let client = InMemoryPlatformClient::default();
    let service = AdkService::new(client.clone());
    service
        .push(root.as_path(), false, true, false)
        .expect("seed status snapshot and remote");
    replace_remote_resource_content(&client, "topics/billing_general.yaml", remote_content);

    let conflicts = service.pull(root.as_path(), false).expect("pull");

    assert!(conflicts.is_empty(), "{conflicts:#?}");
    assert_eq!(
        fs::read_to_string(root.join("topics/billing_general.yaml")).expect("read topic"),
        local_content
    );
}

#[test]
fn pull_preserves_semantically_unchanged_multi_resource_yaml_bytes() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    let local_content = "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n";
    let remote_content = "entities:\n  - name: \"Age\"\n    description: \"Customer age\"\n    entity_type: numeric\n    config:\n      min: 1\n      max: 120\n";
    fs::write(root.join("config/entities.yaml"), local_content).expect("write entities");

    let client = InMemoryPlatformClient::default();
    let service = AdkService::new(client.clone());
    service
        .push(root.as_path(), false, true, false)
        .expect("seed status snapshot and remote");
    replace_remote_resource_content(&client, "config/entities.yaml", remote_content);

    let conflicts = service.pull(root.as_path(), false).expect("pull");

    assert!(conflicts.is_empty(), "{conflicts:#?}");
    assert_eq!(
        fs::read_to_string(root.join("config/entities.yaml")).expect("read entities"),
        local_content
    );
}

#[test]
fn pull_applies_remote_multi_resource_additions_when_existing_entries_are_unchanged() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    let local_content = "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n";
    let remote_content = "entities:\n- name: Age\n  description: Customer age\n  entity_type: numeric\n  config:\n    min: 1\n    max: 120\n- name: Code\n  description: Customer code\n  entity_type: alphanumeric\n  config: {}\n";
    fs::write(root.join("config/entities.yaml"), local_content).expect("write entities");

    let client = InMemoryPlatformClient::default();
    let service = AdkService::new(client.clone());
    service
        .push(root.as_path(), false, true, false)
        .expect("seed status snapshot and remote");
    replace_remote_resource_content(&client, "config/entities.yaml", remote_content);

    let conflicts = service.pull(root.as_path(), false).expect("pull");

    assert!(conflicts.is_empty(), "{conflicts:#?}");
    assert_eq!(
        fs::read_to_string(root.join("config/entities.yaml")).expect("read entities"),
        remote_content
    );
}

#[test]
fn validate_flow_config_start_step_matches_step_display_name() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("flows/appointment_booking/steps")).expect("mkdir flow");
    fs::write(
        root.join("flows/appointment_booking/flow_config.yaml"),
        "name: Appointment Booking\ndescription: Book an appointment.\nstart_step: Check Appointment Details with User\n",
    )
    .expect("write flow config");
    fs::write(
        root.join("flows/appointment_booking/steps/check_appointment_details_with_user.yaml"),
        "step_type: advanced_step\nname: Check Appointment Details with User\nasr_biasing: {}\ndtmf_config: {}\nprompt: Confirm the appointment details.\n",
    )
    .expect("write flow step");

    let errors = service_offline()
        .validate_local_resources(root.as_path())
        .expect("validate");

    assert!(errors.is_empty(), "{errors:#?}");
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
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));
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
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));
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
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));
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
    fs::write(
        root.join("topics/good.yaml"),
        "name: good\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n",
    )
    .expect("write valid topic yaml");

    let service = AdkService::new(InMemoryPlatformClient::default());
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
        "# <<<<<<< ours\nname: Ours\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n# =======\n# >>>>>>> theirs\n",
    )
    .expect("write conflicted file");

    let service = AdkService::new(InMemoryPlatformClient::default());
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
fn push_status_deletion_of_last_resource_emits_delete_command() {
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");
    fs::create_dir_all(root.join("topics")).expect("mkdir topics");
    fs::write(
        root.join("topics/sample.yaml"),
        "name: sample\nenabled: true\nactions: \"\"\ncontent: \"hello\"\nexample_queries: []\n",
    )
    .expect("write topic");

    let snapshot_service = service_offline();
    let discovered = snapshot_service.discover_local_resources(&root);
    write_status_snapshot_from_discovered(&root, &discovered);
    fs::remove_file(root.join("topics/sample.yaml")).expect("delete final topic");

    let projection = serde_json::json!({
        "knowledgeBase": {
            "topics": {
                "entities": {
                    "topic-1": {
                        "name": "sample",
                        "isActive": true,
                        "actions": "",
                        "content": "hello",
                        "exampleQueries": []
                    }
                }
            }
        }
    });
    let service = AdkService::new(ProjectionOnlyClient::new(projection));

    let result = service
        .push(root.as_path(), false, true, true)
        .expect("dry-run push");

    assert!(result.success, "{result:#?}");
    assert!(
        result.commands.iter().any(|command| command
            .get("type")
            .and_then(serde_json::Value::as_str)
            == Some("delete_topic")),
        "{:#?}",
        result.commands
    );
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
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));

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
        Some("Remote Topic")
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
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));

    let conflicts = service.pull(root.as_path(), true).expect("pull force");

    assert!(conflicts.is_empty());
    assert!(root.join("topics/remote_topic.yaml").exists());
    assert!(!root.join("topics/local_only.yaml").exists());
    assert!(!root.join("flows/old_flow").exists());
}

#[test]
fn pull_force_materializes_python_formatting_without_phantom_status_diffs() {
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-lookup": {
                        "id": "fn-lookup",
                        "name": "lookup",
                        "description": "Looks up data.",
                        "code": "\"\"\"Helpers.\"\"\"\n\nimport json\n\ndef lookup(conv):\n    return json.dumps({})\n",
                        "archived": false
                    }
                }
            }
        },
        "flows": {
            "flows": {
                "entities": {
                    "flow-1": {
                        "id": "flow-1",
                        "name": "Support Flow",
                        "description": "",
                        "startStepId": "step-1",
                        "steps": {
                            "entities": {
                                "step-1": {
                                    "name": "Collect Rating",
                                    "type": "default_step",
                                    "prompt": "\nRate the call\n",
                                    "references": {"extractedEntities": {}}
                                }
                            }
                        },
                        "transitionFunctions": {"entities": {}}
                    }
                }
            }
        }
    });
    let remote = projection_to_resource_map(&projection).expect("remote resources");
    let service = AdkService::new(InMemoryPlatformClient::with_resources(remote));
    let root = make_temp_project_dir();
    fs::write(
        root.join("project.yaml"),
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n",
    )
    .expect("write project yaml");

    let conflicts = service.pull(root.as_path(), true).expect("pull force");

    assert!(conflicts.is_empty(), "{conflicts:#?}");
    let function_content =
        fs::read_to_string(root.join("functions/lookup.py")).expect("function content");
    assert!(function_content.starts_with(
        "\"\"\"Helpers.\"\"\"\n\nfrom _gen import *  # <AUTO GENERATED>\nimport json\n"
    ));
    let step_content =
        fs::read_to_string(root.join("flows/support_flow/steps/collect_rating.yaml"))
            .expect("flow step content");
    assert!(step_content.contains("prompt: Rate the call\n"));
    assert!(!step_content.contains("prompt: |"));

    let summary = service.status(root.as_path()).expect("status");
    assert!(summary.modified_files.is_empty(), "{summary:#?}");
    assert!(summary.new_files.is_empty(), "{summary:#?}");
    assert!(summary.deleted_files.is_empty(), "{summary:#?}");
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
    let service = AdkService::new(InMemoryPlatformClient::with_named_resources(
        ResourceMap::new(),
        named,
        deployments,
    ));
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
    let service = AdkService::new(InMemoryPlatformClient::with_named_resources(
        ResourceMap::new(),
        indexmap::IndexMap::new(),
        deployments,
    ));
    let error = service
        .diff(root.as_path(), &[], None, Some("abcdef123".to_string()))
        .expect_err("should fail with no previous deployment");
    assert!(error.to_string().contains("No previous version found."));
}
