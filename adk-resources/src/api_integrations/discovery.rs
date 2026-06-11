use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use regex::Regex;
use serde_yaml_ng::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

static OPERATION_RESOURCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(/([a-zA-Z0-9._~!$&'()*+,;=:@%-]|%[0-9A-Fa-f]{2}|\{[a-zA-Z_][a-zA-Z0-9_]*\})*)+(?:\?([a-zA-Z0-9._~!$&'()*+,;=:@%/?-]|%[0-9A-Fa-f]{2}|\{[a-zA-Z_][a-zA-Z0-9_]*\})*)?$",
    )
    .expect("valid operation resource regex")
});
static URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(https?://[^\s]+)?$").expect("valid URL regex"));

// poly/resources/api_integration.py
/// Validation parity: implemented against Python ApiIntegration.validate().
pub(crate) struct ApiIntegration;
impl DiscoverResources for ApiIntegration {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::API_INTEGRATIONS_FILE.file_path,
        yaml_path: &["api_integrations"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("api_integrations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &path.join("api_integrations").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = ApiIntegration::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    validate_named_sequence(path, yaml, "api_integrations", "API integration", errors);
    let Some(items) = yaml.get("api_integrations").and_then(Value::as_sequence) else {
        return;
    };
    for item in items {
        let Some(raw_name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let name = clean_name(raw_name, false);
        if !is_python_function_name(&name) {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{name}: API integration name '{name}' must follow Python function naming convention (lowercase letters, numbers, and underscores only, starting with letter or underscore)."
            ));
        }
        validate_environments(path, &name, item, errors);
        validate_operations(path, &name, item, errors);
    }
}

fn validate_environments(
    path: &str,
    integration_name: &str,
    item: &Value,
    errors: &mut Vec<String>,
) {
    let Some(environments) = item.get("environments") else {
        return;
    };
    for env_name in ["sandbox", "pre-release", "pre_release", "live"] {
        let Some(config) = environments.get(env_name) else {
            continue;
        };
        let base_url = config
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let auth_type = config
            .get("auth_type")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let python_env_name = if env_name == "pre-release" {
            "pre_release"
        } else {
            env_name
        };
        if !URL_PATTERN.is_match(base_url) {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/environments/{env_name}: Environment '{python_env_name}': base_url '{base_url}' is not a valid URL path."
            ));
        }
        if base_url.is_empty() && auth_type != "none" {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/environments/{env_name}: Environment '{python_env_name}': base_url cannot be empty."
            ));
        }
        if !matches!(auth_type, "none" | "basic" | "apiKey" | "oauth2") {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/environments/{env_name}: Environment '{python_env_name}': auth_type '{auth_type}' invalid, must be one of ['apiKey', 'basic', 'none', 'oauth2']."
            ));
        }
    }
}

fn validate_operations(path: &str, integration_name: &str, item: &Value, errors: &mut Vec<String>) {
    let Some(operations) = item.get("operations").and_then(Value::as_sequence) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for (idx, operation) in operations.iter().enumerate() {
        let name = operation
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let method = operation
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        let resource = operation
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.is_empty() {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{idx}: Operation name cannot be empty."
            ));
        }
        if method.is_empty() {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{idx}: Operation '{name}': method cannot be empty."
            ));
        } else if !matches!(method.as_str(), "GET" | "POST" | "PATCH" | "PUT" | "DELETE") {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{name}: Operation '{name}': method '{method}' invalid, must be one of ['DELETE', 'GET', 'PATCH', 'POST', 'PUT']."
            ));
        }
        if resource.is_empty() {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{name}: Operation '{name}': resource cannot be empty."
            ));
        } else if !OPERATION_RESOURCE_PATTERN.is_match(resource) {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{name}: Operation '{name}': resource '{resource}' is not a valid URL path."
            ));
        }
        if !name.is_empty()
            && !method.is_empty()
            && !seen.insert((name.to_string(), method.clone()))
        {
            errors.push(format!(
                "Validation error in {path}/api_integrations/{integration_name}/operations/{name}: Duplicate operation: name='{name}', method='{method}'."
            ));
        }
    }
}

fn is_python_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("API integration YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_api_integration_environment_and_operation_rules() {
        let errors = validation_errors(
            r#"
api_integrations:
  - name: Bad Name
    environments:
      sandbox:
        base_url: ftp://example.com
        auth_type: token
      live:
        base_url: ""
        auth_type: basic
    operations:
      - name: ""
        method: TRACE
        resource: "not/a/path"
      - name: fetch
        method: get
        resource: /users/{id}
      - name: fetch
        method: GET
        resource: /users/{id}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("must follow Python function naming convention"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("base_url 'ftp://example.com' is not a valid URL path"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("auth_type 'token' invalid"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("base_url cannot be empty"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Operation name cannot be empty"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("method 'TRACE' invalid"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("resource 'not/a/path' is not a valid URL path"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Duplicate operation: name='fetch', method='GET'"))
        );
    }
}
