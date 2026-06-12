//! Typed local parsing helpers for ADK resource-family files.
//!
//! Discovery still owns file location and Python-compatible ordering. This
//! module defines the narrower boundary for converting a resource-owned file
//! into typed Rust values whose constructors encode resource-local invariants.

use serde::Deserialize;
use serde::de::{DeserializeOwned, Error as DeError};
use serde_yaml_ng::Value;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceParseErrors {
    errors: Vec<String>,
}

impl ResourceParseErrors {
    pub(crate) fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub(crate) fn single(path: &str, detail: impl fmt::Display) -> Self {
        Self {
            errors: vec![format!("Validation error in {path}: {detail}")],
        }
    }

    pub(crate) fn push(&mut self, path: &str, detail: impl fmt::Display) {
        self.errors
            .push(format!("Validation error in {path}: {detail}"));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub(crate) fn into_validation_errors(self) -> Vec<String> {
        self.errors
    }
}

pub(crate) type ResourceParseResult<T> = Result<T, ResourceParseErrors>;

pub(crate) trait ParseLocalResource {
    type Parsed;

    /// Parse a resource-owned YAML document into its typed local shape.
    ///
    /// Resource-scoped invariants should live in the parsed types, serde
    /// adapters, or this parse step. The discovery/CLI layer may append these
    /// failures to user-facing validation output, but new resource business
    /// rules should not be written as ad hoc YAML traversal.
    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed>;

    #[allow(dead_code)]
    fn parse_local_content(path: &str, content: &str) -> ResourceParseResult<Self::Parsed> {
        let yaml = serde_yaml_ng::from_str::<Value>(content)
            .map_err(|error| ResourceParseErrors::single(path, error))?;
        Self::parse_local_yaml(path, &yaml)
    }

    fn append_parse_errors(path: &str, yaml: &Value, errors: &mut Vec<String>) {
        if let Err(parse_errors) = Self::parse_local_yaml(path, yaml) {
            errors.extend(parse_errors.into_validation_errors());
        }
    }
}

pub(crate) fn deserialize_yaml<T>(path: &str, yaml: &Value) -> ResourceParseResult<T>
where
    T: DeserializeOwned,
{
    serde_yaml_ng::from_value(yaml.clone())
        .map_err(|error| ResourceParseErrors::single(path, error))
}

pub(crate) fn default_if_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub(crate) fn non_empty_vec<'de, D, T>(
    deserializer: D,
    message: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    if values.is_empty() {
        Err(D::Error::custom(message))
    } else {
        Ok(values)
    }
}

pub(crate) fn non_empty_map<'de, D, K, V>(
    deserializer: D,
    message: &'static str,
) -> Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    let values = BTreeMap::<K, V>::deserialize(deserializer)?;
    if values.is_empty() {
        Err(D::Error::custom(message))
    } else {
        Ok(values)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NonEmptyString(String);

impl NonEmptyString {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NonEmptyString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            Err(D::Error::custom("cannot be empty"))
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoEdgeWhitespace(String);

impl<'de> Deserialize<'de> for NoEdgeWhitespace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value != value.trim() {
            Err(D::Error::custom(
                "Description cannot contain leading or trailing whitespace.",
            ))
        } else {
            Ok(Self(value))
        }
    }
}

pub(crate) fn duplicate_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut duplicates = std::collections::BTreeSet::new();
    for name in names {
        if !seen.insert(name.to_string()) {
            duplicates.insert(name.to_string());
        }
    }
    duplicates
}
