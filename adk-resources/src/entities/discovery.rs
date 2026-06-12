use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NoEdgeWhitespace, NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult,
    deserialize_yaml, duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde::de::{Error as DeError, Visitor};
use serde_yaml_ng::Value;
use std::fmt;
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
        <Self as ParseLocalResource>::validate_local_yaml(
            Self::LOCAL_PATH.primary_path().expect("local file path"),
            yaml,
            errors,
        );
    }
}

#[cfg(test)]
pub(crate) fn validate_local_yaml(yaml: &Value, errors: &mut Vec<String>) {
    let path = Entity::LOCAL_PATH.primary_path().expect("local file path");
    <Entity as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for Entity {
    type Parsed = EntitiesFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<EntitiesFileUnchecked>(path, yaml)?;
        EntitiesFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct EntitiesFile {
    entities: Vec<EntityItem>,
}

impl EntitiesFile {
    fn try_from_unchecked(path: &str, raw: EntitiesFileUnchecked) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.entities.iter().map(EntityItem::name)) {
            errors.push(
                &format!("{path}/entities/{duplicate}"),
                format!("duplicate entity name '{duplicate}'."),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                entities: raw.entities,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct EntitiesFileUnchecked {
    #[serde(default)]
    entities: Vec<EntityItem>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "EntityItemUnchecked")]
enum EntityItem {
    Numeric(EntityWithConfig<NumericConfig>),
    Alphanumeric(EntityWithConfig<AlphanumericConfig>),
    Enum(EntityWithConfig<EnumConfig>),
    Date(EntityWithConfig<DateConfig>),
    PhoneNumber(EntityWithConfig<PhoneNumberConfig>),
    Time(EntityWithConfig<TimeConfig>),
    Address(EntityWithoutConfig),
    FreeText(EntityWithoutConfig),
    NameConfig(EntityWithoutConfig),
}

impl EntityItem {
    fn name(&self) -> &str {
        match self {
            Self::Numeric(entity) => entity.name.as_str(),
            Self::Alphanumeric(entity) => entity.name.as_str(),
            Self::Enum(entity) => entity.name.as_str(),
            Self::Date(entity) => entity.name.as_str(),
            Self::PhoneNumber(entity) => entity.name.as_str(),
            Self::Time(entity) => entity.name.as_str(),
            Self::Address(entity) | Self::FreeText(entity) | Self::NameConfig(entity) => {
                entity.name.as_str()
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct EntityItemUnchecked {
    name: NonEmptyString,
    entity_type: EntityType,
    #[serde(default)]
    description: Option<NoEdgeWhitespace>,
    #[serde(default)]
    config: Value,
}

impl TryFrom<EntityItemUnchecked> for EntityItem {
    type Error = String;

    fn try_from(raw: EntityItemUnchecked) -> Result<Self, Self::Error> {
        match raw.entity_type {
            EntityType::Numeric => parse_entity_config(raw, EntityItem::Numeric),
            EntityType::Alphanumeric => parse_entity_config(raw, EntityItem::Alphanumeric),
            EntityType::Enum => parse_entity_config(raw, EntityItem::Enum),
            EntityType::Date => parse_entity_config(raw, EntityItem::Date),
            EntityType::PhoneNumber => parse_entity_config(raw, EntityItem::PhoneNumber),
            EntityType::Time => parse_entity_config(raw, EntityItem::Time),
            EntityType::Address => Ok(EntityItem::Address(EntityWithoutConfig {
                name: raw.name,
                description: raw.description,
            })),
            EntityType::FreeText => Ok(EntityItem::FreeText(EntityWithoutConfig {
                name: raw.name,
                description: raw.description,
            })),
            EntityType::NameConfig => Ok(EntityItem::NameConfig(EntityWithoutConfig {
                name: raw.name,
                description: raw.description,
            })),
        }
    }
}

fn parse_entity_config<T, F>(raw: EntityItemUnchecked, build: F) -> Result<EntityItem, String>
where
    T: Default + for<'de> Deserialize<'de>,
    F: FnOnce(EntityWithConfig<T>) -> EntityItem,
{
    let config = if matches!(raw.config, Value::Null) {
        T::default()
    } else {
        serde_yaml_ng::from_value::<T>(raw.config).map_err(|error| error.to_string())?
    };
    Ok(build(EntityWithConfig {
        name: raw.name,
        description: raw.description,
        config,
    }))
}

#[derive(Debug)]
#[allow(dead_code)]
struct EntityWithConfig<T> {
    name: NonEmptyString,
    description: Option<NoEdgeWhitespace>,
    config: T,
}

#[derive(Debug)]
#[allow(dead_code)]
struct EntityWithoutConfig {
    name: NonEmptyString,
    description: Option<NoEdgeWhitespace>,
}

#[derive(Debug)]
enum EntityType {
    Numeric,
    Alphanumeric,
    Enum,
    Date,
    PhoneNumber,
    Time,
    Address,
    FreeText,
    NameConfig,
}

impl<'de> Deserialize<'de> for EntityType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EntityTypeVisitor;

        impl Visitor<'_> for EntityTypeVisitor {
            type Value = EntityType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a supported entity_type")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "numeric" => Ok(EntityType::Numeric),
                    "alphanumeric" => Ok(EntityType::Alphanumeric),
                    "enum" => Ok(EntityType::Enum),
                    "date" => Ok(EntityType::Date),
                    "phone_number" => Ok(EntityType::PhoneNumber),
                    "time" => Ok(EntityType::Time),
                    "address" => Ok(EntityType::Address),
                    "free_text" => Ok(EntityType::FreeText),
                    "name_config" => Ok(EntityType::NameConfig),
                    _ => Err(E::custom(format!("unsupported entity_type '{value}'."))),
                }
            }
        }

        deserializer.deserialize_str(EntityTypeVisitor)
    }
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
        validate_local_yaml(&yaml, &mut errors);
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
    }
}
