use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_resources::{is_file, read_yaml_mapping, validate_named_sequence};
use crate::resource_utils::{clean_name, rel_under_root};
use serde_yaml_ng::Value;
use std::path::Path;

// poly/resources/entities.py
/// Validation parity: implemented against Python Entity.validate().
pub(crate) struct Entity;
impl DiscoverResources for Entity {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: crate::specs::ENTITIES_FILE.file_path,
        yaml_path: &["entities"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let entities_path =
            base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &entities_path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &entities_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("entities") else {
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
                &entities_path.join("entities").join(&safe),
            ));
        }
        out
    }

    fn validate_local_yaml(_path: &str, yaml: &Value, errors: &mut Vec<String>) {
        validate_local_yaml(yaml, errors);
    }
}

pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Entity::LOCAL_PATH.primary_path().expect("local file path");
    validate_named_sequence(path, yaml, "entities", "entity", errors);
    let Some(items) = yaml.get("entities").and_then(Value::as_sequence) else {
        return;
    };
    let allowed = [
        "numeric",
        "alphanumeric",
        "enum",
        "date",
        "phone_number",
        "time",
        "address",
        "free_text",
        "name_config",
    ];
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        let Some(entity_type) = item.get("entity_type").and_then(Value::as_str) else {
            errors.push(format!(
                "Validation error in {path}/entities/{name}: entity_type is required."
            ));
            continue;
        };
        if !allowed.contains(&entity_type) {
            errors.push(format!(
                "Validation error in {path}/entities/{name}: unsupported entity_type '{entity_type}'."
            ));
            continue;
        }
        if let Some(description) = item.get("description").and_then(Value::as_str)
            && description != description.trim()
        {
            errors.push(format!(
                "Validation error in {path}/entities/{name}: Description cannot contain leading or trailing whitespace."
            ));
        }
        validate_entity_config(path, name, entity_type, item.get("config"), errors);
    }
}

fn validate_entity_config(
    path: &str,
    name: &str,
    entity_type: &str,
    config: Option<&Value>,
    errors: &mut Vec<String>,
) {
    let Some(config) = config.and_then(Value::as_mapping) else {
        return;
    };
    for (field, expected) in expected_config_fields(entity_type) {
        let Some(value) = config.get(Value::String(field.to_string())) else {
            continue;
        };
        if !expected.matches(value) {
            errors.push(format!(
                "Validation error in {path}/entities/{name}/config/{field}: Config field '{field}' should be of type '{}' for entity type '{entity_type}'.",
                expected.python_name()
            ));
        }
    }
}

#[derive(Clone, Copy)]
enum ExpectedConfigType {
    Bool,
    Number,
    String,
    List,
}

impl ExpectedConfigType {
    fn matches(self, value: &Value) -> bool {
        match self {
            Self::Bool => value.as_bool().is_some(),
            Self::Number => value.as_i64().is_some() || value.as_f64().is_some(),
            Self::String => value.as_str().is_some(),
            Self::List => value.as_sequence().is_some(),
        }
    }

    fn python_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Number => "float or int",
            Self::String => "str",
            Self::List => "list",
        }
    }
}

fn expected_config_fields(entity_type: &str) -> &'static [(&'static str, ExpectedConfigType)] {
    use ExpectedConfigType::{Bool, List, Number, String};
    match entity_type {
        "numeric" => &[
            ("has_decimal", Bool),
            ("has_range", Bool),
            ("min", Number),
            ("max", Number),
        ],
        "alphanumeric" => &[
            ("enabled", Bool),
            ("validation_type", String),
            ("regular_expression", String),
        ],
        "enum" => &[("options", List)],
        "date" => &[("relative_date", Bool)],
        "phone_number" => &[("enabled", Bool), ("country_codes", List)],
        "time" => &[
            ("enabled", Bool),
            ("start_time", String),
            ("end_time", String),
        ],
        "address" | "free_text" | "name_config" => &[],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("entity YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_entity_type_description_and_config_rules() {
        let errors = validation_errors(
            r#"
entities:
  - name: Missing type
  - name: Bad type
    entity_type: unsupported
  - name: Amount
    entity_type: numeric
    description: " has padding "
    config:
      has_decimal: "yes"
      min: low
  - name: Options
    entity_type: enum
    config:
      options: one
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("entity_type is required"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unsupported entity_type 'unsupported'"))
        );
        assert!(errors.iter().any(|error| {
            error.contains("Description cannot contain leading or trailing whitespace")
        }));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Config field 'has_decimal' should be of type 'bool'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Config field 'min' should be of type 'float or int'"))
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Config field 'options' should be of type 'list'"))
        );
    }
}
