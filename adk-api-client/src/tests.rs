use super::*;

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
