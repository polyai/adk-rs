use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
    duplicate_names,
};
use crate::{snake_case_json_keys, to_camel_case, to_snake_case};
use adk_protobuf::entities;
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;

pub(crate) const ENTITIES_FILE_PATH: &str = crate::specs::ENTITIES_FILE.file_path;
pub(crate) const ENTITY_ITEM_PREFIX: &str = "config/entities.yaml/entities/";

#[derive(Debug, PartialEq)]
pub(crate) struct EntitiesFile {
    pub(crate) entities: Vec<EntityItem>,
}

impl EntitiesFile {
    fn from_raw(raw: RawEntitiesFile) -> Self {
        Self {
            entities: raw.entities,
        }
    }
}

pub(crate) fn parse_entities_file(
    path: &str,
    yaml: &YamlValue,
) -> ResourceParseResult<EntitiesFile> {
    let mut errors = duplicate_entity_name_errors(path, yaml);
    match deserialize_entities_file(path, yaml) {
        Ok(raw) if errors.is_empty() => Ok(EntitiesFile::from_raw(raw)),
        Ok(_) => Err(errors),
        Err(parse_errors) => {
            errors.extend(parse_errors);
            Err(errors)
        }
    }
}

pub(crate) fn parse_entities_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Vec<EntityItem>> {
    if path != ENTITIES_FILE_PATH && !path.starts_with(ENTITY_ITEM_PREFIX) {
        return Ok(Vec::new());
    }
    let yaml = serde_yaml_ng::from_str::<YamlValue>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    if path == ENTITIES_FILE_PATH {
        return parse_entities_file(path, &yaml).map(|file| file.entities);
    }
    if path.starts_with(ENTITY_ITEM_PREFIX) {
        return deserialize_entity_item(path, &yaml).map(|item| vec![item]);
    }
    Ok(Vec::new())
}

fn duplicate_entity_name_errors(path: &str, yaml: &YamlValue) -> ResourceParseErrors {
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

fn deserialize_entities_file(path: &str, yaml: &YamlValue) -> ResourceParseResult<RawEntitiesFile> {
    serde_yaml_ng::from_value(yaml.clone()).map_err(|error| {
        ResourceParseErrors::single(path, normalize_entity_deserialize_error(error.to_string()))
    })
}

fn deserialize_entity_item(path: &str, yaml: &YamlValue) -> ResourceParseResult<EntityItem> {
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
struct RawEntitiesFile {
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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub(crate) enum EntityItem {
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

impl EntityItem {
    pub(crate) fn from_projection(
        id: &str,
        entity: &JsonValue,
    ) -> ResourceParseResult<Option<Self>> {
        let name = entity.get("name").and_then(JsonValue::as_str).unwrap_or(id);
        if name.is_empty() {
            return Ok(None);
        }
        let value = serde_json::json!({
            "name": name,
            "description": entity
                .get("description")
                .and_then(JsonValue::as_str)
                .unwrap_or_default(),
            "entity_type": to_snake_case(entity.get("type").and_then(JsonValue::as_str).unwrap_or_default()),
            "config": projection_entity_config(entity),
        });
        let yaml = serde_yaml_ng::to_value(value)
            .map_err(|error| ResourceParseErrors::single(ENTITIES_FILE_PATH, error))?;
        deserialize_entity_item(ENTITIES_FILE_PATH, &yaml).map(Some)
    }

    pub(crate) fn name(&self) -> &str {
        self.body().name.as_str()
    }

    pub(crate) fn description(&self) -> &str {
        &self.body().description
    }

    pub(crate) fn entity_type(&self) -> &'static str {
        match self {
            Self::Numeric(_) => "numeric",
            Self::Alphanumeric(_) => "alphanumeric",
            Self::Enum(_) => "enum",
            Self::Date(_) => "date",
            Self::PhoneNumber(_) => "phone_number",
            Self::Time(_) => "time",
            Self::Address(_) => "address",
            Self::FreeText(_) => "free_text",
            Self::NameConfig(_) => "name_config",
        }
    }

    pub(crate) fn proto_type(&self) -> String {
        to_camel_case(self.entity_type())
    }

    pub(crate) fn create_config(&self) -> entities::entity_create::Config {
        match self {
            Self::Numeric(entity) => {
                entities::entity_create::Config::Numeric(entity.config.to_proto())
            }
            Self::Alphanumeric(entity) => {
                entities::entity_create::Config::Alphanumeric(entity.config.to_proto())
            }
            Self::Enum(entity) => entities::entity_create::Config::Enum(entity.config.to_proto()),
            Self::Date(entity) => entities::entity_create::Config::Date(entity.config.to_proto()),
            Self::PhoneNumber(entity) => {
                entities::entity_create::Config::PhoneNumber(entity.config.to_proto())
            }
            Self::Time(entity) => entities::entity_create::Config::Time(entity.config.to_proto()),
            Self::Address(_) => {
                entities::entity_create::Config::Address(entities::AddressConfig {})
            }
            Self::FreeText(_) => {
                entities::entity_create::Config::FreeText(entities::FreeTextConfig {})
            }
            Self::NameConfig(_) => {
                entities::entity_create::Config::NameConfig(entities::NameConfig {})
            }
        }
    }

    pub(crate) fn update_config(&self) -> entities::entity_update::Config {
        match self {
            Self::Numeric(entity) => {
                entities::entity_update::Config::Numeric(entity.config.to_proto())
            }
            Self::Alphanumeric(entity) => {
                entities::entity_update::Config::Alphanumeric(entity.config.to_proto())
            }
            Self::Enum(entity) => entities::entity_update::Config::Enum(entity.config.to_proto()),
            Self::Date(entity) => entities::entity_update::Config::Date(entity.config.to_proto()),
            Self::PhoneNumber(entity) => {
                entities::entity_update::Config::PhoneNumber(entity.config.to_proto())
            }
            Self::Time(entity) => entities::entity_update::Config::Time(entity.config.to_proto()),
            Self::Address(_) => {
                entities::entity_update::Config::Address(entities::AddressConfig {})
            }
            Self::FreeText(_) => {
                entities::entity_update::Config::FreeText(entities::FreeTextConfig {})
            }
            Self::NameConfig(_) => {
                entities::entity_update::Config::NameConfig(entities::NameConfig {})
            }
        }
    }

    pub(crate) fn config_json(&self) -> JsonValue {
        match self {
            Self::Numeric(entity) => Ok(entity.config.to_json()),
            Self::Alphanumeric(entity) => serde_json::to_value(&entity.config),
            Self::Enum(entity) => serde_json::to_value(&entity.config),
            Self::Date(entity) => serde_json::to_value(&entity.config),
            Self::PhoneNumber(entity) => serde_json::to_value(&entity.config),
            Self::Time(entity) => serde_json::to_value(&entity.config),
            Self::Address(_) | Self::FreeText(_) | Self::NameConfig(_) => Ok(serde_json::json!({})),
        }
        .unwrap_or_else(|_| serde_json::json!({}))
    }

    fn body(&self) -> &EntityBody {
        match self {
            Self::Numeric(entity) => &entity.body,
            Self::Alphanumeric(entity) => &entity.body,
            Self::Enum(entity) => &entity.body,
            Self::Date(entity) => &entity.body,
            Self::PhoneNumber(entity) => &entity.body,
            Self::Time(entity) => &entity.body,
            Self::Address(entity) => &entity.body,
            Self::FreeText(entity) => &entity.body,
            Self::NameConfig(entity) => &entity.body,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(bound(deserialize = "T: Default + Deserialize<'de>"))]
pub(crate) struct EntityWithConfig<T> {
    #[serde(flatten)]
    body: EntityBody,
    #[serde(default, deserialize_with = "default_if_null")]
    config: T,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct EntityWithoutConfig {
    #[serde(flatten)]
    body: EntityBody,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct EntityBody {
    name: NonEmptyString,
    #[serde(default, deserialize_with = "deserialize_description")]
    description: String,
}

fn deserialize_description<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
    if value != value.trim() {
        Err(D::Error::custom(
            "Description cannot contain leading or trailing whitespace.",
        ))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct NumericConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    has_decimal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_range: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<f64>,
}

impl NumericConfig {
    fn to_proto(&self) -> entities::NumberConfig {
        entities::NumberConfig {
            has_decimal: self.has_decimal.unwrap_or(false),
            has_range: self.has_range.unwrap_or(false),
            min: self.min.map(|value| value as f32),
            max: self.max.map(|value| value as f32),
        }
    }

    fn to_json(&self) -> JsonValue {
        let mut value = serde_json::Map::new();
        if let Some(has_decimal) = self.has_decimal {
            value.insert("has_decimal".to_string(), JsonValue::Bool(has_decimal));
        }
        if let Some(has_range) = self.has_range {
            value.insert("has_range".to_string(), JsonValue::Bool(has_range));
        }
        if let Some(min) = self.min {
            value.insert("min".to_string(), json_number_preserving_integer(min));
        }
        if let Some(max) = self.max {
            value.insert("max".to_string(), json_number_preserving_integer(max));
        }
        JsonValue::Object(value)
    }
}

fn json_number_preserving_integer(value: f64) -> JsonValue {
    if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        serde_json::json!(value as i64)
    } else {
        serde_json::json!(value)
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct AlphanumericConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    regular_expression: Option<String>,
}

impl AlphanumericConfig {
    fn to_proto(&self) -> entities::AlphanumericConfig {
        entities::AlphanumericConfig {
            enabled: self.enabled.unwrap_or(true),
            validation_type: self.validation_type.clone().unwrap_or_default(),
            regular_expression: self.regular_expression.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct EnumConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Vec<String>>,
}

impl EnumConfig {
    fn to_proto(&self) -> entities::MultipleOptionsConfig {
        entities::MultipleOptionsConfig {
            options: self.options.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct DateConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_date: Option<bool>,
}

impl DateConfig {
    fn to_proto(&self) -> entities::DateConfig {
        entities::DateConfig {
            relative_date: self.relative_date.unwrap_or(false),
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct PhoneNumberConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    country_codes: Option<Vec<String>>,
}

impl PhoneNumberConfig {
    fn to_proto(&self) -> entities::PhoneNumberConfig {
        entities::PhoneNumberConfig {
            enabled: self.enabled.unwrap_or(true),
            country_codes: self.country_codes.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct TimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<String>,
}

impl TimeConfig {
    fn to_proto(&self) -> entities::TimeConfig {
        entities::TimeConfig {
            enabled: self.enabled.unwrap_or(true),
            start_time: self.start_time.clone().unwrap_or_default(),
            end_time: self.end_time.clone().unwrap_or_default(),
        }
    }
}

fn projection_entity_config(entity: &JsonValue) -> JsonValue {
    if let Some(cfg) = entity.pointer("/config/value") {
        let mut cfg = cfg.clone();
        snake_case_json_keys(&mut cfg);
        return cfg;
    }
    if let Some(cfg) = entity.get("config") {
        let mut cfg = cfg.clone();
        snake_case_json_keys(&mut cfg);
        return cfg;
    }
    let entity_type = to_snake_case(entity.get("type").and_then(JsonValue::as_str).unwrap_or(""));
    let mut cfg = match entity_type.as_str() {
        "numeric" => entity
            .get("numberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "alphanumeric" => entity
            .get("alphanumericConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "enum" => entity
            .get("multipleOptionsConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "date" => entity
            .get("dateConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "phone_number" => entity
            .get("phoneNumberConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "time" => entity
            .get("timeConfig")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        _ => serde_json::json!({}),
    };
    snake_case_json_keys(&mut cfg);
    cfg
}
