use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    ParseLocalResource, ResourceParseErrors, ResourceParseResult, default_if_null,
    deserialize_yaml, duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use regex::Regex;
use serde::Deserialize;
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
        <Self as ParseLocalResource>::validate_local_yaml(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = ApiIntegration::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <ApiIntegration as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for ApiIntegration {
    type Parsed = ApiIntegrationsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<ApiIntegrationsFileUnchecked>(path, yaml)?;
        ApiIntegrationsFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ApiIntegrationsFile {
    api_integrations: Vec<ApiIntegrationItem>,
}

impl ApiIntegrationsFile {
    fn try_from_unchecked(
        path: &str,
        raw: ApiIntegrationsFileUnchecked,
    ) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(
            raw.api_integrations
                .iter()
                .map(|integration| integration.name.as_str())
                .filter(|name| !name.is_empty()),
        ) {
            errors.push(
                &format!("{path}/api_integrations/{duplicate}"),
                format!("duplicate API integration name '{duplicate}'."),
            );
        }
        for (idx, integration) in raw.api_integrations.iter().enumerate() {
            integration.validate(path, idx, &mut errors);
        }
        if errors.is_empty() {
            Ok(Self {
                api_integrations: raw.api_integrations,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiIntegrationsFileUnchecked {
    #[serde(default)]
    api_integrations: Vec<ApiIntegrationItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiIntegrationItem {
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    environments: ApiIntegrationEnvironments,
    #[serde(default, deserialize_with = "default_if_null")]
    operations: Vec<ApiOperation>,
}

impl ApiIntegrationItem {
    fn validate(&self, path: &str, idx: usize, errors: &mut ResourceParseErrors) {
        let name = clean_name(&self.name, false);
        let error_name = if name.is_empty() {
            idx.to_string()
        } else {
            name.clone()
        };
        if self.name.is_empty() {
            errors.push(
                &format!("{path}/api_integrations/{idx}"),
                "Name cannot be empty.",
            );
        } else if !is_python_function_name(&name) {
            errors.push(
                &format!("{path}/api_integrations/{name}"),
                format!(
                    "API integration name '{name}' must follow Python function naming convention (lowercase letters, numbers, and underscores only, starting with letter or underscore)."
                ),
            );
        }
        self.environments.validate(path, &error_name, errors);
        self.validate_operations(path, &error_name, errors);
    }

    fn validate_operations(
        &self,
        path: &str,
        integration_name: &str,
        errors: &mut ResourceParseErrors,
    ) {
        let mut seen = BTreeSet::new();
        for (idx, operation) in self.operations.iter().enumerate() {
            operation.validate(path, integration_name, idx, errors);
            let method = operation.method();
            if !operation.name.is_empty()
                && !method.is_empty()
                && !seen.insert((operation.name.clone(), method.clone()))
            {
                errors.push(
                    &format!(
                        "{path}/api_integrations/{integration_name}/operations/{}",
                        operation.name
                    ),
                    format!(
                        "Duplicate operation: name='{}', method='{method}'.",
                        operation.name
                    ),
                );
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ApiIntegrationEnvironments {
    #[serde(default, deserialize_with = "default_if_null")]
    sandbox: ApiIntegrationConfig,
    #[serde(default, alias = "pre-release", deserialize_with = "default_if_null")]
    pre_release: ApiIntegrationConfig,
    #[serde(default, deserialize_with = "default_if_null")]
    live: ApiIntegrationConfig,
}

impl ApiIntegrationEnvironments {
    fn validate(&self, path: &str, integration_name: &str, errors: &mut ResourceParseErrors) {
        for (env_name, config) in [
            ("sandbox", &self.sandbox),
            ("pre_release", &self.pre_release),
            ("live", &self.live),
        ] {
            config.validate(path, integration_name, env_name, errors);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ApiIntegrationConfig {
    #[serde(default, deserialize_with = "default_if_null")]
    base_url: String,
    #[serde(default, deserialize_with = "default_if_null")]
    auth_type: String,
}

impl ApiIntegrationConfig {
    fn auth_type(&self) -> &str {
        if self.auth_type.is_empty() {
            "none"
        } else {
            &self.auth_type
        }
    }

    fn validate(
        &self,
        path: &str,
        integration_name: &str,
        env_name: &str,
        errors: &mut ResourceParseErrors,
    ) {
        let auth_type = self.auth_type();
        if !URL_PATTERN.is_match(&self.base_url) {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/environments/{env_name}"),
                format!(
                    "Environment '{env_name}': base_url '{}' is not a valid URL path.",
                    self.base_url
                ),
            );
        }
        if self.base_url.is_empty() && auth_type != "none" {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/environments/{env_name}"),
                format!("Environment '{env_name}': base_url cannot be empty."),
            );
        }
        if !matches!(auth_type, "none" | "basic" | "apiKey" | "oauth2") {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/environments/{env_name}"),
                format!(
                    "Environment '{env_name}': auth_type '{auth_type}' invalid, must be one of ['apiKey', 'basic', 'none', 'oauth2']."
                ),
            );
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiOperation {
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    method: String,
    #[serde(default, deserialize_with = "default_if_null")]
    resource: String,
}

impl ApiOperation {
    fn method(&self) -> String {
        self.method.to_ascii_uppercase()
    }

    fn validate(
        &self,
        path: &str,
        integration_name: &str,
        idx: usize,
        errors: &mut ResourceParseErrors,
    ) {
        let method = self.method();
        if self.name.is_empty() {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/operations/{idx}"),
                "Operation name cannot be empty.",
            );
        }
        if method.is_empty() {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/operations/{idx}"),
                format!("Operation '{}': method cannot be empty.", self.name),
            );
        } else if !matches!(method.as_str(), "GET" | "POST" | "PATCH" | "PUT" | "DELETE") {
            errors.push(
                &format!("{path}/api_integrations/{integration_name}/operations/{}", self.name),
                format!(
                    "Operation '{}': method '{method}' invalid, must be one of ['DELETE', 'GET', 'PATCH', 'POST', 'PUT'].",
                    self.name
                ),
            );
        }
        if self.resource.is_empty() {
            errors.push(
                &format!(
                    "{path}/api_integrations/{integration_name}/operations/{}",
                    self.name
                ),
                format!("Operation '{}': resource cannot be empty.", self.name),
            );
        } else if !OPERATION_RESOURCE_PATTERN.is_match(&self.resource) {
            errors.push(
                &format!(
                    "{path}/api_integrations/{integration_name}/operations/{}",
                    self.name
                ),
                format!(
                    "Operation '{}': resource '{}' is not a valid URL path.",
                    self.name, self.resource
                ),
            );
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
