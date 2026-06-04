use super::*;
use adk_types::Resource;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use serde_json::json;
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvVarGuard {
    fn new(vars: &[&'static str]) -> Self {
        let lock = ENV_LOCK.lock().expect("env lock");
        let saved = vars
            .iter()
            .map(|&name| (name, std::env::var_os(name)))
            .collect();
        Self { _lock: lock, saved }
    }

    fn set(&self, name: &'static str, value: &str) {
        unsafe {
            std::env::set_var(name, value);
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

fn adk_v1_test_client(server: &MockServer) -> HttpPlatformClient {
    HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: format!("{}/adk/v1", server.base_url()),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
        command_user_override: None,
    }
}

fn active_deployments_mock<'a>(server: &'a MockServer, body: Value) -> Mock<'a> {
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test-account/projects/test-project/deployments/active");
        then.status(200).json_body(body);
    })
}

fn branches_mock<'a>(server: &'a MockServer, body: Value) -> Mock<'a> {
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test-account/projects/test-project/branches");
        then.status(200).json_body(body);
    })
}

fn branch_projection_mock<'a>(
    server: &'a MockServer,
    branch_id: &'static str,
    projection: Value,
) -> Mock<'a> {
    server.mock(move |when, then| {
        when.method(GET).path(format!(
            "/adk/v1/accounts/test-account/projects/test-project/branches/{branch_id}/projection"
        ));
        then.status(200)
            .json_body(json!({ "projection": projection }));
    })
}

fn deployments_mock<'a>(server: &'a MockServer, body: Value) -> Mock<'a> {
    server.mock(|when, then| {
        when.method(GET)
            .path("/adk/v1/accounts/test-account/projects/test-project/deployments");
        then.status(200).json_body(body);
    })
}

fn deployment_projection_mock<'a>(
    server: &'a MockServer,
    deployment_id: &'static str,
    projection: Value,
) -> Mock<'a> {
    server.mock(move |when, then| {
        when.method(GET).path(format!(
            "/adk/v1/accounts/test-account/projects/test-project/deployments/{deployment_id}/projection"
        ));
        then.status(200).json_body(json!({ "projection": projection }));
    })
}

fn empty_projection() -> Value {
    json!({})
}

#[test]
fn api_key_env_names_match_python_resolution_order() {
    assert_eq!(
        api_key_env_names("us-1"),
        vec!["POLY_ADK_KEY_US", "POLY_ADK_KEY"]
    );
    assert_eq!(
        api_key_env_names("euw-1"),
        vec!["POLY_ADK_KEY_EUW", "POLY_ADK_KEY"]
    );
    assert_eq!(
        api_key_env_names("uk-1"),
        vec!["POLY_ADK_KEY_UK", "POLY_ADK_KEY"]
    );
}

#[test]
fn command_user_override_env_is_captured_when_client_is_created() {
    let guard = EnvVarGuard::new(&["ADK_COMMAND_USER_OVERRIDE"]);
    guard.set("ADK_COMMAND_USER_OVERRIDE", "env-user@example.com");

    let client = HttpPlatformClient::new_with_api_key(
        "us-1",
        "test-account",
        "test-project",
        Some("main"),
        "test-key",
    )
    .expect("client");

    guard.set("ADK_COMMAND_USER_OVERRIDE", "later-user@example.com");

    assert_eq!(
        client.command_user_override.as_deref(),
        Some("env-user@example.com")
    );
}

#[test]
fn empty_command_user_override_env_is_ignored() {
    let guard = EnvVarGuard::new(&["ADK_COMMAND_USER_OVERRIDE"]);
    guard.set("ADK_COMMAND_USER_OVERRIDE", "");

    let client = HttpPlatformClient::new_with_api_key(
        "us-1",
        "test-account",
        "test-project",
        Some("main"),
        "test-key",
    )
    .expect("client");

    assert_eq!(client.command_user_override, None);
}

#[test]
fn base_url_env_names_match_python_resolution_order() {
    assert_eq!(
        base_url_env_names("us-1"),
        vec![
            "POLY_ADK_BASE_URL_US",
            "POLY_ADK_BASE_URL_US_1",
            "POLY_ADK_BASE_URL"
        ]
    );
    assert_eq!(
        base_url_env_names("euw-1"),
        vec![
            "POLY_ADK_BASE_URL_EUW",
            "POLY_ADK_BASE_URL_EUW_1",
            "POLY_ADK_BASE_URL"
        ]
    );
    assert_eq!(
        base_url_env_names("uk-1"),
        vec![
            "POLY_ADK_BASE_URL_UK",
            "POLY_ADK_BASE_URL_UK_1",
            "POLY_ADK_BASE_URL"
        ]
    );
}

#[test]
fn unknown_region_error_matches_python_platform_handler() {
    let message = base_url_for_region("moon-1")
        .expect_err("unknown region")
        .to_string();
    assert!(message.contains("Unknown region: moon-1"));
}

#[test]
fn default_voice_ids_match_python_region_defaults() {
    assert_eq!(default_voice_id("us-1"), "VOICE-6fad73f6");
    assert_eq!(default_voice_id("euw-1"), "VOICE-8b814724");
    assert_eq!(default_voice_id("uk-1"), "VOICE-37966683");
    assert_eq!(default_voice_id("dev"), "VOICE-e2b01d55");
    assert_eq!(default_voice_id("staging"), "VOICE-e2b01d55");
    assert_eq!(default_voice_id("studio"), "VOICE-071db756");
}

#[test]
fn deployment_version_prefix_matches_first_nine_hash_characters() {
    let server = MockServer::start();
    let deployments = deployments_mock(
        &server,
        json!({
            "deployments": [
                {
                    "id": "dep-old",
                    "version_hash": "111111111bbbbbbbb",
                },
                {
                    "deploymentId": "dep-target",
                    "versionHash": "ABCDEF12399999999",
                }
            ]
        }),
    );
    let client = adk_v1_test_client(&server);

    let deployment_id = client
        .deployment_id_from_version_prefix("abcdef123extra")
        .expect("deployment lookup");

    assert_eq!(deployment_id.as_deref(), Some("dep-target"));
    deployments.assert_calls(1);
}

#[test]
fn empty_deployment_version_prefix_does_not_call_platform() {
    let server = MockServer::start();
    let client = adk_v1_test_client(&server);

    let deployment_id = client
        .deployment_id_from_version_prefix("")
        .expect("deployment lookup");

    assert_eq!(deployment_id, None);
}

#[test]
fn named_projection_uses_active_environment_deployment_id() {
    let server = MockServer::start();
    let active = active_deployments_mock(
        &server,
        json!({
            "sandbox": {
                "deployment_id": "dep-sandbox",
            }
        }),
    );
    let deployment_projection =
        deployment_projection_mock(&server, "dep-sandbox", json!({ "source": "sandbox" }));
    let client = adk_v1_test_client(&server);

    let projection = client
        .pull_projection_json_by_name("sandbox")
        .expect("sandbox projection");

    assert_eq!(projection, json!({ "source": "sandbox" }));
    active.assert_calls(1);
    deployment_projection.assert_calls(1);
}

#[test]
fn named_projection_resolves_active_environment_version_hash() {
    let server = MockServer::start();
    let active = active_deployments_mock(
        &server,
        json!({
            "live": {
                "version_hash": "abc123def99999999",
            }
        }),
    );
    let deployments = deployments_mock(
        &server,
        json!({
            "deployments": [
                {
                    "id": "dep-live",
                    "hash": "ABC123DEF00000000",
                }
            ]
        }),
    );
    let deployment_projection =
        deployment_projection_mock(&server, "dep-live", json!({ "source": "live" }));
    let client = adk_v1_test_client(&server);

    let projection = client
        .pull_projection_json_by_name("live")
        .expect("live projection");

    assert_eq!(projection, json!({ "source": "live" }));
    active.assert_calls(1);
    deployments.assert_calls(1);
    deployment_projection.assert_calls(1);
}

#[test]
fn named_projection_reports_missing_active_environment() {
    let server = MockServer::start();
    let active = active_deployments_mock(&server, json!({}));
    let client = adk_v1_test_client(&server);

    let message = client
        .pull_projection_json_by_name("pre-release")
        .expect_err("missing active pre-release deployment")
        .to_string();

    assert_eq!(
        message,
        "http error: No active deployment found for environment 'pre-release'"
    );
    active.assert_calls(1);
}

#[test]
fn named_projection_resolves_branch_by_name_and_id() {
    for (name, expected_branch_id) in [
        ("feature branch", "branch-123"),
        ("branch-456", "branch-456"),
    ] {
        let server = MockServer::start();
        let branches = branches_mock(
            &server,
            json!({
                "branches": [
                    {
                        "branchId": "branch-123",
                        "name": "feature branch",
                    },
                    {
                        "branchId": "branch-456",
                        "name": "other branch",
                    }
                ]
            }),
        );
        let branch_projection =
            branch_projection_mock(&server, expected_branch_id, json!({ "branch": name }));
        let client = adk_v1_test_client(&server);

        let projection = client
            .pull_projection_json_by_name(name)
            .expect("branch projection");

        assert_eq!(projection, json!({ "branch": name }));
        branches.assert_calls(1);
        branch_projection.assert_calls(1);
    }
}

#[test]
fn named_projection_resolves_deployment_hash_after_branch_miss() {
    let server = MockServer::start();
    let branches = branches_mock(&server, json!({ "branches": [] }));
    let deployments = deployments_mock(
        &server,
        json!({
            "deployments": [
                {
                    "id": "dep-direct",
                    "version_hash": "123456789abcdef",
                }
            ]
        }),
    );
    let deployment_projection =
        deployment_projection_mock(&server, "dep-direct", json!({ "source": "deployment" }));
    let client = adk_v1_test_client(&server);

    let projection = client
        .pull_projection_json_by_name("123456789")
        .expect("deployment projection");

    assert_eq!(projection, json!({ "source": "deployment" }));
    branches.assert_calls(1);
    deployments.assert_calls(1);
    deployment_projection.assert_calls(1);
}

#[test]
fn named_projection_reports_unknown_name_after_all_resolution_attempts() {
    let server = MockServer::start();
    let branches = branches_mock(&server, json!({ "branches": [] }));
    let deployments = deployments_mock(&server, json!({ "deployments": [] }));
    let client = adk_v1_test_client(&server);

    let message = client
        .pull_projection_json_by_name("missing")
        .expect_err("missing named projection")
        .to_string();

    assert_eq!(
        message,
        "http error: Name 'missing' not found in environments, branches, or deployments"
    );
    branches.assert_calls(1);
    deployments.assert_calls(4);
}

#[test]
fn named_resources_uses_active_environment_deployment() {
    let server = MockServer::start();
    let active = active_deployments_mock(
        &server,
        json!({
            "sandbox": {
                "id": "dep-sandbox",
            }
        }),
    );
    let deployment_projection =
        deployment_projection_mock(&server, "dep-sandbox", empty_projection());
    let client = adk_v1_test_client(&server);

    let resources = client
        .pull_resources_by_name("sandbox")
        .expect("sandbox resources");

    assert!(resources.is_empty());
    active.assert_calls(1);
    deployment_projection.assert_calls(1);
}

#[test]
fn named_resources_resolves_branch_by_name() {
    let server = MockServer::start();
    let branches = branches_mock(
        &server,
        json!({
            "branches": [
                {
                    "branchId": "branch-123",
                    "name": "feature branch",
                }
            ]
        }),
    );
    let branch_projection = branch_projection_mock(&server, "branch-123", empty_projection());
    let client = adk_v1_test_client(&server);

    let resources = client
        .pull_resources_by_name("feature branch")
        .expect("branch resources");

    assert!(resources.is_empty());
    branches.assert_calls(1);
    branch_projection.assert_calls(1);
}

#[test]
fn named_resources_resolves_deployment_hash() {
    let server = MockServer::start();
    let branches = branches_mock(&server, json!({ "branches": [] }));
    let deployments = deployments_mock(
        &server,
        json!({
            "deployments": [
                {
                    "deployment_id": "dep-direct",
                    "version_hash": "fedcba987abcdef",
                }
            ]
        }),
    );
    let deployment_projection =
        deployment_projection_mock(&server, "dep-direct", empty_projection());
    let client = adk_v1_test_client(&server);

    let resources = client
        .pull_resources_by_name("fedcba987")
        .expect("deployment resources");

    assert!(resources.is_empty());
    branches.assert_calls(1);
    deployments.assert_calls(1);
    deployment_projection.assert_calls(1);
}

#[test]
fn named_resources_reports_unknown_name_after_all_resolution_attempts() {
    let server = MockServer::start();
    let branches = branches_mock(&server, json!({ "branches": [] }));
    let deployments = deployments_mock(&server, json!({ "deployments": [] }));
    let client = adk_v1_test_client(&server);

    let message = client
        .pull_resources_by_name("missing")
        .expect_err("missing named resources")
        .to_string();

    assert_eq!(
        message,
        "http error: Name 'missing' not found in environments, branches, or deployments"
    );
    branches.assert_calls(1);
    deployments.assert_calls(4);
}

#[test]
fn push_no_changes_uses_python_failure_contract() {
    let client = HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: "http://localhost".to_string(),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
        command_user_override: None,
    };
    let resources = ResourceMap::new();
    let projection = serde_json::json!({});

    let result = client
        .push_resources_with_options(&resources, Some(&projection))
        .expect("push result");

    assert!(!result.success);
    assert_eq!(result.message, "No changes detected");
    assert!(result.commands.is_empty());
}

#[test]
fn changed_resource_preview_does_not_delete_unmentioned_remote_resources() {
    let client = HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: "http://localhost".to_string(),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
        command_user_override: None,
    };
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/new_func.py".to_string(),
        Resource {
            resource_id: "local".to_string(),
            name: "new_func".to_string(),
            file_path: "functions/new_func.py".to_string(),
            payload: serde_json::json!({
                "content": "def new_func(conv):\n    return 'ok'\n"
            }),
        },
    );
    let projection = serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "name": "existing_func",
                        "code": "def existing_func(conv):\n    return 'remote'\n",
                        "archived": false
                    }
                }
            }
        },
        "knowledgeBase": {
            "topics": {
                "ids": ["topic-1"],
                "entities": {
                    "topic-1": {
                        "name": "Existing Topic",
                        "content": "Remote content",
                        "actions": "",
                        "isActive": true
                    }
                }
            }
        }
    });

    let result = client
        .preview_push_changed_resources_with_options(&resources, Some(&projection))
        .expect("preview changed resources");
    let command_types = result
        .commands
        .iter()
        .filter_map(|command| command.get("type").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(command_types, vec!["create_function"]);
}

#[test]
fn extract_response_commands_reads_common_response_shapes() {
    let direct = serde_json::json!({
        "commands": [{"type": "create_topic"}]
    });
    assert_eq!(extract_response_commands(&direct).len(), 1);

    let nested_batch = serde_json::json!({
        "commandBatch": {"commands": [{"type": "delete_topic"}]}
    });
    assert_eq!(extract_response_commands(&nested_batch).len(), 1);

    let nested_result = serde_json::json!({
        "result": {"commands": [{"type": "update_topic"}]}
    });
    assert_eq!(extract_response_commands(&nested_result).len(), 1);
}

#[test]
fn conversations_endpoints_use_public_platform_api() {
    let server = MockServer::start();
    let list = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/agents/test-project/conversations")
            .query_param("limit", "20")
            .query_param("offset", "5");
        then.status(200).json_body(json!({
            "conversations": [{"conversationId": "KA-123"}],
            "count": 1,
            "limit": 20,
            "offset": 5
        }));
    });
    let get = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/agents/test-project/conversations/KA-123");
        then.status(200).json_body(json!({
            "conversationId": "KA-123",
            "turns": []
        }));
    });
    let audio = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/agents/test-project/conversations/KA-123/audio")
            .query_param("direction", "user")
            .query_param("redacted", "true");
        then.status(200).body("RIFFtest");
    });

    let client = test_client(server.base_url());
    let conversations = client
        .list_conversations(20, 5)
        .expect("list conversations");
    assert_eq!(
        conversations["conversations"][0]["conversationId"],
        "KA-123"
    );

    let conversation = client.get_conversation("KA-123").expect("get conversation");
    assert_eq!(conversation["conversationId"], "KA-123");

    let bytes = client
        .get_conversation_audio("KA-123", "user", true)
        .expect("get conversation audio");
    assert_eq!(bytes, b"RIFFtest");
    list.assert();
    get.assert();
    audio.assert();
}

fn test_client(base_url: String) -> HttpPlatformClient {
    HttpPlatformClient {
        client: reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .expect("test client"),
        base_url,
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
        command_user_override: None,
    }
}

fn test_client_with_command_user_override(base_url: String, email: &str) -> HttpPlatformClient {
    let mut client = test_client(base_url);
    client.command_user_override = Some(email.to_string());
    client
}

fn projection_with_function(name: &str) -> Value {
    serde_json::json!({
        "functions": {
            "functions": {
                "entities": {
                    "fn-1": {
                        "id": "fn-1",
                        "name": name,
                        "description": "Look up a customer",
                        "code": "def lookup_customer(conv):\n    return None\n"
                    }
                }
            }
        }
    })
}

#[test]
fn preview_push_uses_command_user_override_for_metadata_created_by() {
    let client = test_client_with_command_user_override(
        "http://localhost".to_string(),
        "env-user@example.com",
    );
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/override_author.py".to_string(),
        function_resource(
            "functions/override_author.py",
            "override_author",
            "def override_author(conv):\n    return 'ok'\n",
        ),
    );

    let result = client
        .preview_push_resources_with_options(&resources, Some(&serde_json::json!({})))
        .expect("preview push");

    assert_eq!(
        result.commands[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("created_by"))
            .and_then(Value::as_str),
        Some("env-user@example.com")
    );
}

#[test]
fn push_resources_sends_command_user_override_header_on_json_and_command_batch_requests() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let server = MockServer::start();
    let projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/projection")
            .header("X-PolyAI-Email", "env-user@example.com");
        then.status(200).json_body(serde_json::json!({
            "lastKnownSequence": "12",
            "projection": {}
        }));
    });
    let push = server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/command-batch")
            .header("X-PolyAI-Email", "env-user@example.com");
        then.status(200).json_body(serde_json::json!({
            "message": "pushed",
            "commands": [{"type": "create_function"}]
        }));
    });
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/header_author.py".to_string(),
        function_resource(
            "functions/header_author.py",
            "header_author",
            "def header_author(conv):\n    return 'ok'\n",
        ),
    );

    let result = test_client_with_command_user_override(server.base_url(), "env-user@example.com")
        .push_resources_with_options(&resources, None)
        .expect("push resources");

    assert!(result.success);
    projection.assert();
    push.assert();
}

#[test]
fn pull_projection_by_environment_uses_active_deployment_id() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    let server = MockServer::start();
    let active = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments/active");
        then.status(200).json_body(serde_json::json!({
            "sandbox": {"deployment_id": "dep-active"}
        }));
    });
    let projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments/dep-active/projection");
        then.status(200).json_body(serde_json::json!({
            "projection": {"selected": "active-deployment"}
        }));
    });

    let pulled = test_client(server.base_url())
        .pull_projection_json_by_name("sandbox")
        .expect("pull sandbox projection");

    assert_eq!(pulled, serde_json::json!({"selected": "active-deployment"}));
    active.assert();
    projection.assert();
}

#[test]
fn pull_projection_by_environment_falls_back_from_active_hash_to_deployment_prefix() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    let server = MockServer::start();
    let active = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments/active");
        then.status(200).json_body(serde_json::json!({
            "live": {"version_hash": "abcdef123999"}
        }));
    });
    let deployments = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments");
        then.status(200).json_body(serde_json::json!({
            "deployments": [
                {"id": "dep-from-hash", "version_hash": "abcdef123456"}
            ]
        }));
    });
    let projection = server.mock(|when, then| {
        when.method(GET).path(
            "/accounts/test-account/projects/test-project/deployments/dep-from-hash/projection",
        );
        then.status(200).json_body(serde_json::json!({
            "projection": {"selected": "hash-fallback"}
        }));
    });

    let pulled = test_client(server.base_url())
        .pull_projection_json_by_name("live")
        .expect("pull live projection");

    assert_eq!(pulled["selected"], "hash-fallback");
    active.assert();
    deployments.assert();
    projection.assert();
}

#[test]
fn pull_projection_by_branch_matches_name_and_branch_id() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    for (query_name, expected_branch_id) in [("feature", "br-123"), ("br-456", "br-456")] {
        let server = MockServer::start();
        let branches = server.mock(|when, then| {
            when.method(GET)
                .path("/accounts/test-account/projects/test-project/branches");
            then.status(200).json_body(serde_json::json!({
                "branches": [
                    {"branchId": "br-123", "name": "feature"},
                    {"branchId": "br-456", "name": "other"}
                ]
            }));
        });
        let projection = server.mock(|when, then| {
            when.method(GET).path(format!(
                "/accounts/test-account/projects/test-project/branches/{expected_branch_id}/projection"
            ));
            then.status(200).json_body(serde_json::json!({
                "projection": {"branch": expected_branch_id}
            }));
        });

        let pulled = test_client(server.base_url())
            .pull_projection_json_by_name(query_name)
            .expect("pull branch projection");

        assert_eq!(pulled["branch"], expected_branch_id);
        branches.assert();
        projection.assert();
    }
}

#[test]
fn deployment_prefix_resolution_uses_first_matching_deployment_for_ambiguous_prefixes() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    let server = MockServer::start();
    let base = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments");
        then.status(200).json_body(serde_json::json!({
            "deployments": [
                {"id": "dep-base", "version_hash": "111222333000"}
            ]
        }));
    });
    let sandbox = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments")
            .query_param("client_env", "sandbox");
        then.status(200).json_body(serde_json::json!({
            "deployments": [
                {"id": "dep-sandbox", "version_hash": "111222333aaa"}
            ]
        }));
    });
    let pre_release = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments")
            .query_param("client_env", "pre-release");
        then.status(200).json_body(serde_json::json!({
            "deployments": [
                {"id": "dep-pre", "version_hash": "111222333bbb"}
            ]
        }));
    });
    let projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments/dep-base/projection");
        then.status(200).json_body(serde_json::json!({
            "projection": {"deployment": "dep-base"}
        }));
    });
    let branches = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches");
        then.status(200)
            .json_body(serde_json::json!({"branches": []}));
    });

    let pulled = test_client(server.base_url())
        .pull_projection_json_by_name("111222333")
        .expect("pull deployment projection");

    assert_eq!(pulled["deployment"], "dep-base");
    branches.assert();
    base.assert();
    sandbox.assert_calls(0);
    pre_release.assert_calls(0);
    projection.assert();
}

#[test]
fn pull_resources_by_name_reports_missing_environment_and_unknown_name() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    let missing_env_server = MockServer::start();
    let active = missing_env_server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/deployments/active");
        then.status(200).json_body(serde_json::json!({}));
    });
    let missing_env = test_client(missing_env_server.base_url())
        .pull_resources_by_name("pre-release")
        .expect_err("missing active environment should error")
        .to_string();
    assert!(missing_env.contains("No active deployment found for environment 'pre-release'"));
    active.assert();

    let unknown_server = MockServer::start();
    let branches = unknown_server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches");
        then.status(200)
            .json_body(serde_json::json!({"branches": []}));
    });
    for env_name in [None, Some("sandbox"), Some("pre-release"), Some("live")] {
        unknown_server.mock(|when, then| {
            let when = when
                .method(GET)
                .path("/accounts/test-account/projects/test-project/deployments");
            if let Some(env_name) = env_name {
                when.query_param("client_env", env_name);
            }
            then.status(200)
                .json_body(serde_json::json!({"deployments": []}));
        });
    }

    let unknown = test_client(unknown_server.base_url())
        .pull_resources_by_name("does-not-exist")
        .expect_err("unknown name should error")
        .to_string();
    assert!(unknown.contains("Name 'does-not-exist' not found"));
    branches.assert();
}

#[test]
fn pull_resources_by_branch_materializes_projection() {
    use httpmock::Method::GET;
    use httpmock::MockServer;

    let server = MockServer::start();
    let branches = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches");
        then.status(200).json_body(serde_json::json!({
            "branches": [{"branchId": "br-funcs", "name": "functions-branch"}]
        }));
    });
    let projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/br-funcs/projection");
        then.status(200).json_body(serde_json::json!({
            "projection": projection_with_function("Lookup Customer")
        }));
    });

    let resources = test_client(server.base_url())
        .pull_resources_by_name("functions-branch")
        .expect("pull branch resources");

    let function = resources
        .get("functions/lookup_customer.py")
        .and_then(|resource| resource.payload.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(function.contains("Look up a customer"));
    assert!(function.contains("def lookup_customer"));
    branches.assert();
    projection.assert();
}

#[test]
fn default_voice_id_matches_region_defaults_and_fallback() {
    assert_eq!(default_voice_id("us-1"), "VOICE-6fad73f6");
    assert_eq!(default_voice_id("euw-1"), "VOICE-8b814724");
    assert_eq!(default_voice_id("uk-1"), "VOICE-37966683");
    assert_eq!(default_voice_id("dev"), "VOICE-e2b01d55");
    assert_eq!(default_voice_id("staging"), "VOICE-e2b01d55");
    assert_eq!(default_voice_id("studio"), "VOICE-071db756");
}

fn function_resource(path: &str, name: &str, code: &str) -> Resource {
    Resource {
        resource_id: "local".to_string(),
        name: name.to_string(),
        file_path: path.to_string(),
        payload: serde_json::json!({"content": code}),
    }
}

#[test]
fn push_main_resources_to_new_branch_pushes_generated_commands() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let server = MockServer::start();
    let main_projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/projection");
        then.status(200).json_body(serde_json::json!({
            "lastKnownSequence": "41",
            "projection": {}
        }));
    });
    let create_branch = server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches")
            .body_includes("feature-push")
            .body_includes("expectedMainLastKnownSequence");
        then.status(200)
            .json_body(serde_json::json!({"branchId": "br-new"}));
    });
    let push = server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/br-new/command-batch");
        then.status(200).json_body(serde_json::json!({
            "message": "pushed to new branch",
            "commands": [{"type": "create_function"}]
        }));
    });
    let mut resources = ResourceMap::new();
    resources.insert(
        "functions/new_branch_fn.py".to_string(),
        function_resource(
            "functions/new_branch_fn.py",
            "new_branch_fn",
            "def new_branch_fn(conv):\n    return 'ok'\n",
        ),
    );

    let (branch_id, result) = test_client(server.base_url())
        .push_main_resources_to_new_branch("feature-push", &resources)
        .expect("push main resources to new branch");

    assert_eq!(branch_id, "br-new");
    assert!(result.success);
    assert_eq!(result.message, "pushed to new branch");
    assert_eq!(
        result.commands,
        vec![serde_json::json!({"type": "create_function"})]
    );
    main_projection.assert();
    create_branch.assert();
    push.assert();
}

#[test]
fn push_main_resources_to_new_branch_reports_no_changes_without_command_batch() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let server = MockServer::start();
    let main_projection = server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/projection");
        then.status(200).json_body(serde_json::json!({
            "lastKnownSequence": 7,
            "projection": {}
        }));
    });
    let create_branch = server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches")
            .body_includes("empty-branch");
        then.status(200)
            .json_body(serde_json::json!({"branchId": "br-empty"}));
    });
    let push = server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/br-empty/command-batch");
        then.status(200).json_body(serde_json::json!({}));
    });

    let (branch_id, result) = test_client(server.base_url())
        .push_main_resources_to_new_branch("empty-branch", &ResourceMap::new())
        .expect("push no-op resources to new branch");

    assert_eq!(branch_id, "br-empty");
    assert!(!result.success);
    assert_eq!(result.message, "No changes detected");
    assert!(result.commands.is_empty());
    main_projection.assert();
    create_branch.assert();
    push.assert_calls(0);
}

#[test]
fn create_chat_session_maps_live_and_draft_payloads() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let live_server = MockServer::start();
    let live_chat = live_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/chat")
            .body_includes("client_env")
            .body_includes("live")
            .body_includes("variant_id")
            .body_includes("asr_lang_code")
            .body_includes("tts_lang_code");
        then.status(200).json_body(serde_json::json!({
            "conversation_id": "conv-live",
            "response": "hello"
        }));
    });

    let live = test_client(live_server.base_url())
        .create_chat_session(serde_json::json!({
            "environment": "live",
            "channel": "web",
            "variant": "variant-1",
            "input_lang": "en-US",
            "output_lang": "en-GB"
        }))
        .expect("create live chat");
    assert_eq!(live["conversation_id"], "conv-live");
    live_chat.assert();

    let draft_server = MockServer::start();
    let sequence = draft_server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/sequence");
        then.status(200)
            .json_body(serde_json::json!({"lastKnownSequence": "5"}));
    });
    let prepare = draft_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/chat")
            .body_includes("expectedBranchLastKnownSequence");
        then.status(200).json_body(serde_json::json!({
            "artifactVersion": "artifact-1",
            "lambdaDeploymentVersion": "lambda-1"
        }));
    });
    let draft_chat = draft_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/draft/chat")
            .body_includes("artifact_version")
            .body_includes("lambda_deployment_version")
            .body_excludes("client_env");
        then.status(200).json_body(serde_json::json!({
            "conversation_id": "conv-draft"
        }));
    });

    let draft = test_client(draft_server.base_url())
        .create_chat_session(serde_json::json!({"environment": "draft"}))
        .expect("create draft chat");
    assert_eq!(draft["conversation_id"], "conv-draft");
    sequence.assert();
    prepare.assert();
    draft_chat.assert();
}

#[test]
fn create_chat_session_reports_missing_draft_chat_versions() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/sequence");
        then.status(200)
            .json_body(serde_json::json!({"lastKnownSequence": "5"}));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/chat");
        then.status(200).json_body(serde_json::json!({
            "lambdaDeploymentVersion": "lambda-1"
        }));
    });

    let error = test_client(server.base_url())
        .create_chat_session(serde_json::json!({"environment": "draft"}))
        .expect_err("missing artifact version should fail")
        .to_string();

    assert!(error.contains("missing artifactVersion in branch chat response"));
}

#[test]
fn send_chat_message_maps_environment_and_requires_conversation_id() {
    use httpmock::Method::POST;
    use httpmock::MockServer;

    let missing = test_client("http://localhost".to_string())
        .send_chat_message(serde_json::json!({"message": "hello"}))
        .expect_err("conversation_id is required")
        .to_string();
    assert!(missing.contains("conversation_id"));

    let live_server = MockServer::start();
    let live_send = live_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/chat/conv-live")
            .body_includes("client_env")
            .body_includes("pre-release")
            .body_includes("message")
            .body_includes("asr_lang_code")
            .body_includes("tts_lang_code");
        then.status(200)
            .json_body(serde_json::json!({"response": "ok"}));
    });
    let live = test_client(live_server.base_url())
        .send_chat_message(serde_json::json!({
            "conversation_id": "conv-live",
            "environment": "pre-release",
            "message": "Hello",
            "input_lang": "en-US",
            "output_lang": "en-GB"
        }))
        .expect("send live chat message");
    assert_eq!(live["response"], "ok");
    live_send.assert();

    let draft_server = MockServer::start();
    let draft_send = draft_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/draft/chat/conv-draft")
            .body_includes("Hello draft")
            .body_excludes("client_env");
        then.status(200)
            .json_body(serde_json::json!({"response": "draft ok"}));
    });
    let draft = test_client(draft_server.base_url())
        .send_chat_message(serde_json::json!({
            "conversation_id": "conv-draft",
            "environment": "draft",
            "message": "Hello draft"
        }))
        .expect("send draft chat message");
    assert_eq!(draft["response"], "draft ok");
    draft_send.assert();
}

#[test]
fn merge_branch_returns_success_and_conflict_results() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let success_server = MockServer::start();
    let success_sequence = success_server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/sequence");
        then.status(200)
            .json_body(serde_json::json!({"lastKnownSequence": "7"}));
    });
    let success_merge = success_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/merge")
            .body_includes("deploymentMessage")
            .body_includes("conflictResolutions");
        then.status(200)
            .json_body(serde_json::json!({"sequence": "8"}));
    });

    let success = test_client(success_server.base_url())
        .merge_branch(
            "ship it",
            Some(vec![
                serde_json::json!({"path": ["topics"], "resolution": "ours"}),
            ]),
        )
        .expect("merge branch");
    assert!(success.success);
    assert_eq!(success.sequence.as_deref(), Some("8"));
    assert!(success.conflicts.is_empty());
    success_sequence.assert();
    success_merge.assert();

    let conflict_server = MockServer::start();
    let conflict_sequence = conflict_server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/sequence");
        then.status(200)
            .json_body(serde_json::json!({"lastKnownSequence": 9}));
    });
    let conflict_merge = conflict_server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/merge")
            .body_excludes("conflictResolutions");
        then.status(400).json_body(serde_json::json!({
            "hasConflicts": true,
            "sequence": "10",
            "conflicts": [{"path": ["knowledgeBase", "topics"]}],
            "errors": [{"message": "resolve conflicts"}]
        }));
    });

    let conflict = test_client(conflict_server.base_url())
        .merge_branch("blocked", None)
        .expect("conflicting merge returns structured result");
    assert!(!conflict.success);
    assert_eq!(conflict.sequence.as_deref(), Some("10"));
    assert_eq!(conflict.conflicts.len(), 1);
    assert_eq!(conflict.errors.len(), 1);
    conflict_sequence.assert();
    conflict_merge.assert();
}

#[test]
fn merge_branch_reports_non_conflict_errors() {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/accounts/test-account/projects/test-project/branches/main/sequence");
        then.status(200)
            .json_body(serde_json::json!({"lastKnownSequence": 3}));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/accounts/test-account/projects/test-project/branches/main/merge");
        then.status(500)
            .json_body(serde_json::json!({"error": "boom"}));
    });

    let error = test_client(server.base_url())
        .merge_branch("boom", None)
        .expect_err("server error should fail")
        .to_string();

    assert!(error.contains("status=500"));
    assert!(error.contains("boom"));
}

#[test]
fn in_memory_pull_resources_by_name_uses_exact_prefix_and_default_fallback() {
    let fallback = ResourceMap::from([(
        "functions/fallback.py".to_string(),
        function_resource(
            "functions/fallback.py",
            "fallback",
            "def fallback(conv):\n    return 'fallback'\n",
        ),
    )]);
    let exact = ResourceMap::from([(
        "functions/exact.py".to_string(),
        function_resource(
            "functions/exact.py",
            "exact",
            "def exact(conv):\n    return 'exact'\n",
        ),
    )]);
    let prefixed = ResourceMap::from([(
        "functions/prefixed.py".to_string(),
        function_resource(
            "functions/prefixed.py",
            "prefixed",
            "def prefixed(conv):\n    return 'prefixed'\n",
        ),
    )]);
    let mut named = indexmap::IndexMap::new();
    named.insert("friendly".to_string(), exact.clone());
    named.insert("abcdef123999".to_string(), prefixed.clone());
    let client = InMemoryPlatformClient::with_named_resources(
        fallback.clone(),
        named,
        DeploymentList {
            versions: vec![],
            active_deployment_hashes: Default::default(),
        },
    );

    assert_eq!(
        client.pull_resources_by_name("friendly").expect("exact"),
        exact
    );
    assert_eq!(
        client
            .pull_resources_by_name("ABCDEF123000")
            .expect("prefix"),
        prefixed
    );
    assert_eq!(
        client
            .pull_resources_by_name("does-not-exist")
            .expect("fallback"),
        fallback
    );
}
