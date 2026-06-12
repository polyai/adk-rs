use crate::discover::{DiscoverResources, LocalResourcePath};
use crate::local_parse::{
    NonEmptyString, ParseLocalResource, ResourceParseErrors, ResourceParseResult, deserialize_yaml,
    duplicate_names,
};
use crate::local_resources::{is_file, read_yaml_mapping};
use crate::resource_utils::{clean_name, rel_under_root};
use serde::Deserialize;
use serde::de::{Error as DeError, Visitor};
use serde_yaml_ng::Value;
use std::fmt;
use std::path::Path;

// poly/resources/handoff.py
/// Validation parity: implemented against Python Handoff.validate() and Handoff.validate_collection().
pub(crate) struct Handoff;
impl DiscoverResources for Handoff {
    const LOCAL_PATH: LocalResourcePath = LocalResourcePath::InFile {
        path: "config/handoffs.yaml",
        yaml_path: &["handoffs"],
    };

    fn discover_resources<Fs: adk_io::FileSystem>(fs: &Fs, base_path: &Path) -> Vec<String> {
        let path = base_path.join(Self::LOCAL_PATH.primary_path().expect("local file path"));
        if !is_file(fs, &path) {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(fs, &path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("handoffs") else {
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
                &path.join("handoffs").join(&safe),
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
    let path = Handoff::LOCAL_PATH.primary_path().expect("local file path");
    <Handoff as ParseLocalResource>::validate_local_yaml(path, yaml, errors);
}

impl ParseLocalResource for Handoff {
    type Parsed = HandoffsFile;

    fn parse_local_yaml(path: &str, yaml: &Value) -> ResourceParseResult<Self::Parsed> {
        let raw = deserialize_yaml::<HandoffsFileUnchecked>(path, yaml)?;
        HandoffsFile::try_from_unchecked(path, raw)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct HandoffsFile {
    handoffs: Vec<HandoffItem>,
}

impl HandoffsFile {
    fn try_from_unchecked(path: &str, raw: HandoffsFileUnchecked) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.handoffs.iter().map(|handoff| handoff.name.as_str())) {
            errors.push(
                &format!("{path}/handoffs/{duplicate}"),
                format!("duplicate handoff name '{duplicate}'."),
            );
        }
        let default_names = raw
            .handoffs
            .iter()
            .filter(|handoff| handoff.is_default)
            .map(|handoff| handoff.name.as_str().to_string())
            .collect::<Vec<_>>();
        if default_names.len() != 1 {
            errors.push(
                path,
                format!(
                    "Multiple or zero default handoffs detected: [{}]. One handoff must be set as default.",
                    python_string_list(&default_names)
                ),
            );
        }
        if errors.is_empty() {
            Ok(Self {
                handoffs: raw.handoffs,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize)]
struct HandoffsFileUnchecked {
    #[serde(default)]
    handoffs: Vec<HandoffItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HandoffItem {
    name: NonEmptyString,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    sip_config: Option<SipConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "SipConfigUnchecked")]
#[allow(dead_code)]
struct SipConfig {
    method: SipMethod,
    outbound_encryption: Option<InviteEncryption>,
}

#[derive(Debug, Deserialize)]
struct SipConfigUnchecked {
    #[serde(default)]
    method: SipMethod,
    outbound_encryption: Option<InviteEncryption>,
}

impl TryFrom<SipConfigUnchecked> for SipConfig {
    type Error = String;

    fn try_from(raw: SipConfigUnchecked) -> Result<Self, Self::Error> {
        if matches!(raw.method, SipMethod::Invite) && raw.outbound_encryption.is_none() {
            return Err(
                "Invalid encryption method ''. Must be one of: TLS/SRTP, UDP/RTP".to_string(),
            );
        }
        Ok(Self {
            method: raw.method,
            outbound_encryption: raw.outbound_encryption,
        })
    }
}

#[derive(Debug, Default)]
enum SipMethod {
    Invite,
    Refer,
    #[default]
    Bye,
}

impl<'de> Deserialize<'de> for SipMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SipMethodVisitor;

        impl Visitor<'_> for SipMethodVisitor {
            type Value = SipMethod;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("invite, refer, or bye")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "invite" => Ok(SipMethod::Invite),
                    "refer" => Ok(SipMethod::Refer),
                    "bye" => Ok(SipMethod::Bye),
                    _ => Err(E::custom(format!(
                        "Invalid SIP method '{value}'. Must be one of: invite, refer, bye"
                    ))),
                }
            }
        }

        deserializer.deserialize_str(SipMethodVisitor)
    }
}

#[derive(Debug)]
enum InviteEncryption {
    TlsSrtp,
    UdpRtp,
}

impl<'de> Deserialize<'de> for InviteEncryption {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct InviteEncryptionVisitor;

        impl Visitor<'_> for InviteEncryptionVisitor {
            type Value = InviteEncryption;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("TLS/SRTP or UDP/RTP")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "TLS/SRTP" => Ok(InviteEncryption::TlsSrtp),
                    "UDP/RTP" => Ok(InviteEncryption::UdpRtp),
                    _ => Err(E::custom(format!(
                        "Invalid encryption method '{value}'. Must be one of: TLS/SRTP, UDP/RTP"
                    ))),
                }
            }
        }

        deserializer.deserialize_str(InviteEncryptionVisitor)
    }
}

fn python_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml_ng::from_str;

    fn validation_errors(yaml: &str) -> Vec<String> {
        let yaml = from_str::<Value>(yaml).expect("handoffs YAML");
        let mut errors = Vec::new();
        validate_local_yaml(&yaml, &mut errors);
        errors
    }

    #[test]
    fn validates_python_handoff_sip_rules_and_default_collection() {
        let invalid_method = validation_errors(
            r#"
handoffs:
  - name: Primary
    is_default: true
    sip_config:
      method: transfer
"#,
        );
        assert!(
            invalid_method
                .iter()
                .any(|error| error.contains("Invalid SIP method 'transfer'"))
        );

        let invalid_encryption = validation_errors(
            r#"
handoffs:
  - name: Backup
    is_default: true
    sip_config:
      method: invite
      outbound_encryption: plaintext
"#,
        );
        assert!(
            invalid_encryption
                .iter()
                .any(|error| error.contains("Invalid encryption method 'plaintext'"))
        );

        let missing_default = validation_errors(
            r#"
handoffs:
  - name: Primary
    is_default: false
"#,
        );
        assert!(
            missing_default
                .iter()
                .any(|error| error.contains("Multiple or zero default handoffs detected: []"))
        );
    }
}
