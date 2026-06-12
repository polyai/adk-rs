use crate::local_parse::{
    ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml, duplicate_names,
};
use crate::resource_utils::clean_name;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::collections::BTreeSet;
use std::sync::LazyLock;

static OPERATION_RESOURCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(/([a-zA-Z0-9._~!$&'()*+,;=:@%-]|%[0-9A-Fa-f]{2}|\{[a-zA-Z_][a-zA-Z0-9_]*\})*)+(?:\?([a-zA-Z0-9._~!$&'()*+,;=:@%/?-]|%[0-9A-Fa-f]{2}|\{[a-zA-Z_][a-zA-Z0-9_]*\})*)?$",
    )
    .expect("valid operation resource regex")
});
static URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(https?://[^\s]+)?$").expect("valid URL regex"));

#[derive(Debug, Serialize)]
pub(crate) struct ApiIntegrationsFile {
    pub(crate) api_integrations: Vec<ApiIntegrationItem>,
}

impl ApiIntegrationsFile {
    pub(crate) fn new(api_integrations: Vec<ApiIntegrationItem>) -> Self {
        Self { api_integrations }
    }

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

pub(crate) fn parse_api_integrations_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<ApiIntegrationsFile> {
    let raw = deserialize_yaml::<ApiIntegrationsFileUnchecked>(path, yaml)?;
    ApiIntegrationsFile::try_from_unchecked(path, raw)
}

#[derive(Debug, Deserialize)]
struct ApiIntegrationsFileUnchecked {
    #[serde(default)]
    api_integrations: Vec<ApiIntegrationItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ApiIntegrationItem {
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    description: String,
    #[serde(default, deserialize_with = "default_if_null")]
    environments: ApiIntegrationEnvironments,
    #[serde(default, deserialize_with = "default_if_null")]
    operations: Vec<ApiOperation>,
}

impl ApiIntegrationItem {
    pub(crate) fn from_projection(
        name: String,
        description: String,
        environments: ApiIntegrationEnvironments,
        operations: Vec<ApiOperation>,
    ) -> Result<Self, String> {
        if name.is_empty() {
            return Err("Name cannot be empty.".to_string());
        }
        Ok(Self {
            name,
            description,
            environments,
            operations,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn environments(&self) -> &ApiIntegrationEnvironments {
        &self.environments
    }

    pub(crate) fn operations(&self) -> &[ApiOperation] {
        &self.operations
    }

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

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct ApiIntegrationEnvironments {
    #[serde(default, deserialize_with = "default_if_null")]
    sandbox: ApiIntegrationConfig,
    #[serde(
        default,
        rename = "pre-release",
        alias = "pre_release",
        deserialize_with = "default_if_null"
    )]
    pre_release: ApiIntegrationConfig,
    #[serde(default, deserialize_with = "default_if_null")]
    live: ApiIntegrationConfig,
}

impl ApiIntegrationEnvironments {
    pub(crate) fn new(
        sandbox: Option<ApiIntegrationConfig>,
        pre_release: Option<ApiIntegrationConfig>,
        live: Option<ApiIntegrationConfig>,
    ) -> Self {
        Self {
            sandbox: sandbox.unwrap_or_default(),
            pre_release: pre_release.unwrap_or_default(),
            live: live.unwrap_or_default(),
        }
    }

    pub(crate) fn entries(&self) -> [(&'static str, &ApiIntegrationConfig); 3] {
        [
            ("sandbox", &self.sandbox),
            ("pre_release", &self.pre_release),
            ("live", &self.live),
        ]
    }

    fn validate(&self, path: &str, integration_name: &str, errors: &mut ResourceParseErrors) {
        for (env_name, config) in self.entries() {
            config.validate(path, integration_name, env_name, errors);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ApiIntegrationConfig {
    #[serde(default, deserialize_with = "default_if_null")]
    base_url: String,
    #[serde(default = "default_auth_type", deserialize_with = "auth_type")]
    auth_type: String,
}

impl Default for ApiIntegrationConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            auth_type: default_auth_type(),
        }
    }
}

impl ApiIntegrationConfig {
    pub(crate) fn new(base_url: String, auth_type: String) -> Self {
        Self {
            base_url,
            auth_type: normalize_auth_type(auth_type),
        }
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn auth_type(&self) -> &str {
        &self.auth_type
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ApiOperation {
    #[serde(default, skip_serializing)]
    id: String,
    #[serde(default, deserialize_with = "default_if_null")]
    name: String,
    #[serde(default, deserialize_with = "default_if_null")]
    method: String,
    #[serde(default, deserialize_with = "default_if_null")]
    resource: String,
}

impl ApiOperation {
    pub(crate) fn from_projection(
        id: String,
        name: String,
        method: String,
        resource: String,
    ) -> Result<Self, String> {
        if name.is_empty() {
            return Err("Operation name cannot be empty.".to_string());
        }
        Ok(Self {
            id,
            name,
            method: method.to_ascii_uppercase(),
            resource,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn method(&self) -> String {
        self.method.to_ascii_uppercase()
    }

    pub(crate) fn resource(&self) -> &str {
        &self.resource
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

fn default_auth_type() -> String {
    "none".to_string()
}

fn normalize_auth_type(value: String) -> String {
    if value.is_empty() {
        default_auth_type()
    } else {
        value
    }
}

fn auth_type<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(normalize_auth_type(
        Option::<String>::deserialize(deserializer)?.unwrap_or_default(),
    ))
}

fn is_python_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}
