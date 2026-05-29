use super::*;
use adk_types::Resource;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use serde_json::json;

fn test_client(server: &MockServer) -> HttpPlatformClient {
    HttpPlatformClient {
        client: reqwest::blocking::Client::new(),
        base_url: format!("{}/adk/v1", server.base_url()),
        api_key: "test-key".to_string(),
        account_id: "test-account".to_string(),
        project_id: "test-project".to_string(),
        branch_id: "main".to_string(),
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
    assert_eq!(default_voice_id("studio"), "VOICE-afe2b8e8");
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
    let client = test_client(&server);

    let deployment_id = client
        .deployment_id_from_version_prefix("abcdef123extra")
        .expect("deployment lookup");

    assert_eq!(deployment_id.as_deref(), Some("dep-target"));
    deployments.assert_calls(1);
}

#[test]
fn empty_deployment_version_prefix_does_not_call_platform() {
    let server = MockServer::start();
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
        let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    let client = test_client(&server);

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
    };
    let resources = ResourceMap::new();
    let projection = serde_json::json!({});

    let result = client
        .push_resources_with_options(&resources, Some(&projection), None)
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
        .preview_push_changed_resources_with_options(&resources, Some(&projection), None)
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
