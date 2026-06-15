use crate::api_integrations::local::{ApiIntegrationsFile, parse_api_integrations_file};
use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{ParseLocalResource, ResourceParseResult};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

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

    fn append_local_resource_errors(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        <Self as ParseLocalResource>::append_parse_errors(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn append_parse_errors(yaml: &Value, errors: &mut Vec<String>) {
    let path = ApiIntegration::LOCAL_PATH
        .primary_path()
        .expect("local file path");
    <ApiIntegration as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for ApiIntegration {
    type Parsed = ApiIntegrationsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        parse_api_integrations_file(path, yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("API integration YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_api_integration_environment_and_operation_rules() {
        let errors = validation_errors(
            r#"
api_integrations:
  - name: orders-api
  - name: orders_api
  - name: 123-api
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

        assert!(!errors.iter().any(|error| error.contains(
            "API integration name 'orders_api' must follow Python function naming convention"
        )));
        assert!(
            errors
                .iter()
                .any(|error| { error.contains("duplicate API integration name 'orders_api'") })
        );
        assert!(errors.iter().any(|error| error.contains(
            "API integration name '123_api' must follow Python function naming convention"
        )));
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
