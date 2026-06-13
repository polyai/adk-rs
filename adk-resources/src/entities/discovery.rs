use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NoEdgeWhitespace, NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult,
    default_if_null, deserialize_yaml, duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
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
    let path = Entity::LOCAL_PATH.primary_path().expect("local file path");
    <Entity as ParseLocalResource>::append_parse_errors(path, yaml, errors);
}

impl ParseLocalResource for Entity {
    type Parsed = EntitiesFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let mut errors = duplicate_entity_name_errors(path, yaml);
        match deserialize_entities_file(path, yaml) {
            Ok(raw) if errors.is_empty() => Ok(EntitiesFile {
                entities: raw.entities,
            }),
            Ok(_) => Err(errors),
            Err(parse_errors) => {
                errors.extend(parse_errors);
                Err(errors)
            }
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct EntitiesFile {
    entities: Vec<EntityItem>,
}

fn duplicate_entity_name_errors(path: &str, yaml: &Value) -> ResourceParseErrors {
    let mut errors = ResourceParseErrors::new();
    let Ok(raw) = deserialize_yaml::<EntityNamesFile>(path, yaml) else {
        return errors;
    };
    for duplicate in duplicate_names(raw.entities.iter().filter_map(EntityName::name)) {
        errors.push(
            &format!("{path}/entities/{duplicate}"),
            format!("duplicate entity name '{duplicate}'."),
        );
    }
    errors
}

fn deserialize_entities_file(
    path: &str,
    yaml: &Value,
) -> ResourceParseResult<EntitiesFileUnchecked> {
    serde_yaml_ng::from_value(yaml.clone()).map_err(|error| {
        ResourceParseErrors::single(path, normalize_entity_deserialize_error(error.to_string()))
    })
}

fn normalize_entity_deserialize_error(error: String) -> String {
    if let Some((_, tail)) = error.split_once("unknown variant `")
        && let Some((entity_type, _)) = tail.split_once('`')
    {
        return format!("unsupported entity_type '{entity_type}'");
    }
    error
}

#[derive(Debug, Deserialize)]
struct EntitiesFileUnchecked {
    #[serde(default)]
    entities: Vec<EntityItem>,
}

#[derive(Debug, Deserialize)]
struct EntityNamesFile {
    #[serde(default)]
    entities: Vec<EntityName>,
}

#[derive(Debug, Deserialize)]
struct EntityName {
    name: Option<String>,
}

impl EntityName {
    fn name(&self) -> Option<&str> {
        self.name.as_deref().filter(|name| !name.is_empty())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
#[allow(dead_code)]
enum EntityItem {
    Numeric(EntityWithConfig<NumericConfig>),
    Alphanumeric(EntityWithConfig<AlphanumericConfig>),
    #[serde(rename = "enum")]
    Enum(EntityWithConfig<EnumConfig>),
    Date(EntityWithConfig<DateConfig>),
    PhoneNumber(EntityWithConfig<PhoneNumberConfig>),
    Time(EntityWithConfig<TimeConfig>),
    Address(EntityWithoutConfig),
    FreeText(EntityWithoutConfig),
    NameConfig(EntityWithoutConfig),
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Default + Deserialize<'de>"))]
#[allow(dead_code)]
struct EntityWithConfig<T> {
    name: NonEmptyString,
    #[serde(default)]
    description: Option<NoEdgeWhitespace>,
    #[serde(default, deserialize_with = "default_if_null")]
    config: T,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EntityWithoutConfig {
    name: NonEmptyString,
    #[serde(default)]
    description: Option<NoEdgeWhitespace>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct NumericConfig {
    has_decimal: Option<bool>,
    has_range: Option<bool>,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct AlphanumericConfig {
    enabled: Option<bool>,
    validation_type: Option<String>,
    regular_expression: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct EnumConfig {
    options: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct DateConfig {
    relative_date: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct PhoneNumberConfig {
    enabled: Option<bool>,
    country_codes: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct TimeConfig {
    enabled: Option<bool>,
    start_time: Option<String>,
    end_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("entity YAML");
        let mut errors = Vec::new();
        append_parse_errors(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_entity_type_description_and_config_rules() {
        let missing_type = validation_errors(
            r#"
entities:
  - name: Missing type
"#,
        );
        assert!(
            missing_type
                .iter()
                .any(|error| error.contains("missing field `entity_type`"))
        );

        let bad_type = validation_errors(
            r#"
entities:
  - name: Bad type
    entity_type: unsupported
"#,
        );
        assert!(
            bad_type
                .iter()
                .any(|error| error.contains("unsupported entity_type 'unsupported'"))
        );

        let duplicate_and_bad_type = validation_errors(
            r#"
entities:
  - name: customer
    entity_type: unsupported
  - name: customer
    entity_type: enum
"#,
        );
        assert!(
            duplicate_and_bad_type
                .iter()
                .any(|error| error.contains("duplicate entity name 'customer'"))
        );
        assert!(
            duplicate_and_bad_type
                .iter()
                .any(|error| error.contains("unsupported entity_type 'unsupported'"))
        );

        let padded_description = validation_errors(
            r#"
entities:
  - name: Amount
    entity_type: numeric
    description: " has padding "
"#,
        );
        assert!(padded_description.iter().any(|error| {
            error.contains("Description cannot contain leading or trailing whitespace")
        }));

        let bad_numeric_config = validation_errors(
            r#"
entities:
  - name: Amount
    entity_type: numeric
    config:
      has_decimal: "yes"
      min: low
"#,
        );
        assert!(
            bad_numeric_config
                .iter()
                .any(|error| error.contains("invalid type: string \"yes\", expected a boolean"))
        );

        let bad_enum_config = validation_errors(
            r#"
entities:
  - name: Options
    entity_type: enum
    config:
      options: one
"#,
        );
        assert!(
            bad_enum_config
                .iter()
                .any(|error| error.contains("invalid type: string \"one\", expected a sequence"))
        );

        let null_config = validation_errors(
            r#"
entities:
  - name: Amount
    entity_type: numeric
    config: null
"#,
        );
        assert!(null_config.is_empty());
    }
}
