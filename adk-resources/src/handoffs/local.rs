use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
    duplicate_names,
};
use adk_protobuf::handoff::{
    SipByeHandoffConfig, SipConfig as ProtoSipConfig, SipHeader, SipHeaders,
    SipInviteHandoffConfig, SipReferHandoffConfig, sip_config,
};
use serde::de::Error as DeError;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

pub(crate) const HANDOFFS_FILE_PATH: &str = "config/handoffs.yaml";
pub(crate) const HANDOFF_ITEM_PREFIX: &str = "config/handoffs.yaml/handoffs/";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandoffsFile {
    handoffs: Vec<Handoff>,
}

impl HandoffsFile {
    pub(crate) fn new(handoffs: Vec<Handoff>) -> Self {
        Self { handoffs }
    }

    fn try_from_raw(path: &str, raw: RawHandoffsFile) -> ResourceParseResult<Self> {
        let mut errors = ResourceParseErrors::new();
        for duplicate in duplicate_names(raw.handoffs.iter().map(|handoff| handoff.name())) {
            errors.push(
                &format!("{path}/handoffs/{duplicate}"),
                format!("duplicate handoff name '{duplicate}'."),
            );
        }
        let default_names = raw
            .handoffs
            .iter()
            .filter(|handoff| handoff.is_default)
            .map(|handoff| handoff.name().to_string())
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

pub(crate) fn parse_handoffs_file(path: &str, yaml: &Value) -> ResourceParseResult<HandoffsFile> {
    let raw = deserialize_yaml::<RawHandoffsFile>(path, yaml)?;
    HandoffsFile::try_from_raw(path, raw)
}

pub(crate) fn parse_handoffs_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Vec<Handoff>> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    if path == HANDOFFS_FILE_PATH {
        return deserialize_yaml::<RawHandoffsFile>(path, &yaml).map(|file| file.into_handoffs());
    }
    if path.starts_with(HANDOFF_ITEM_PREFIX) {
        return deserialize_yaml::<Handoff>(path, &yaml).map(|handoff| vec![handoff]);
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, Deserialize)]
struct RawHandoffsFile {
    #[serde(default)]
    handoffs: Vec<Handoff>,
}

impl RawHandoffsFile {
    fn into_handoffs(self) -> Vec<Handoff> {
        self.handoffs
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Handoff {
    name: NonEmptyString,
    #[serde(default, deserialize_with = "string_or_default")]
    description: String,
    #[serde(default, alias = "isDefault")]
    is_default: bool,
    #[serde(default, alias = "sipConfig", deserialize_with = "default_if_null")]
    sip_config: SipConfig,
    #[serde(default, alias = "sipHeaders", deserialize_with = "default_if_null")]
    sip_headers: Vec<LocalSipHeader>,
}

impl Handoff {
    pub(super) fn new(
        name: String,
        description: String,
        is_default: bool,
        sip_config: SipConfig,
        sip_headers: Vec<(String, String)>,
    ) -> Result<Self, String> {
        Ok(Self {
            name: NonEmptyString::new(name)?,
            description,
            is_default,
            sip_config,
            sip_headers: sip_headers
                .into_iter()
                .map(|(key, value)| LocalSipHeader {
                    key: Some(key),
                    value: Some(value),
                })
                .collect(),
        })
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn is_default(&self) -> bool {
        self.is_default
    }

    pub(crate) fn sip_config_proto(&self) -> ProtoSipConfig {
        self.sip_config.to_proto()
    }

    pub(crate) fn sip_headers_proto(&self) -> Option<SipHeaders> {
        let headers = self
            .sip_headers
            .iter()
            .filter_map(LocalSipHeader::to_proto)
            .collect::<Vec<_>>();
        (!headers.is_empty()).then_some(SipHeaders { headers })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub(super) enum SipConfig {
    Invite {
        phone_number: String,
        outbound_endpoint: String,
        outbound_encryption: String,
    },
    Refer {
        phone_number: String,
    },
    #[default]
    Bye,
}

impl<'de> Deserialize<'de> for SipConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        RawSipConfig::deserialize(deserializer)?
            .try_into()
            .map_err(D::Error::custom)
    }
}

impl SipConfig {
    fn to_proto(&self) -> ProtoSipConfig {
        let config = match self {
            Self::Invite {
                phone_number,
                outbound_endpoint,
                outbound_encryption,
            } => sip_config::Config::Invite(SipInviteHandoffConfig {
                phone_number: phone_number.clone(),
                outbound_endpoint: outbound_endpoint.clone(),
                outbound_encryption: outbound_encryption.clone(),
            }),
            Self::Refer { phone_number } => sip_config::Config::Refer(SipReferHandoffConfig {
                phone_number: phone_number.clone(),
            }),
            Self::Bye => sip_config::Config::Bye(SipByeHandoffConfig {}),
        };
        ProtoSipConfig {
            config: Some(config),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawSipConfig {
    #[serde(default)]
    method: SipMethod,
    #[serde(default, deserialize_with = "string_or_default")]
    phone_number: String,
    #[serde(default, deserialize_with = "string_or_default")]
    outbound_endpoint: String,
    #[serde(default, deserialize_with = "string_or_default")]
    outbound_encryption: String,
}

impl TryFrom<RawSipConfig> for SipConfig {
    type Error = String;

    fn try_from(raw: RawSipConfig) -> Result<Self, Self::Error> {
        match raw.method {
            SipMethod::Invite => {
                let Some(outbound_encryption) =
                    normalize_invite_encryption(&raw.outbound_encryption)
                else {
                    return Err(format!(
                        "Invalid encryption method '{}'. Must be one of: TLS/SRTP, UDP/RTP",
                        raw.outbound_encryption
                    ));
                };
                Ok(Self::Invite {
                    phone_number: raw.phone_number,
                    outbound_endpoint: raw.outbound_endpoint,
                    outbound_encryption,
                })
            }
            SipMethod::Refer => Ok(Self::Refer {
                phone_number: raw.phone_number,
            }),
            SipMethod::Bye => Ok(Self::Bye),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
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
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "invite" => Ok(Self::Invite),
            "refer" => Ok(Self::Refer),
            "bye" => Ok(Self::Bye),
            _ => Err(D::Error::custom(format!(
                "Invalid SIP method '{value}'. Must be one of: invite, refer, bye"
            ))),
        }
    }
}

pub(super) fn normalize_invite_encryption(value: &str) -> Option<String> {
    match value {
        "TLS/SRTP" | "tls" => Some("TLS/SRTP".to_string()),
        "UDP/RTP" | "udp" => Some("UDP/RTP".to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LocalSipHeader {
    #[serde(default, deserialize_with = "optional_string")]
    key: Option<String>,
    #[serde(default, deserialize_with = "optional_string")]
    value: Option<String>,
}

impl LocalSipHeader {
    fn to_proto(&self) -> Option<SipHeader> {
        let key = self.key.as_ref()?;
        if key.is_empty() {
            return None;
        }
        Some(SipHeader {
            key: key.clone(),
            value: self.value.clone()?,
        })
    }
}

impl Serialize for LocalSipHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("LocalSipHeader", 2)?;
        state.serialize_field("key", self.key.as_deref().unwrap_or(""))?;
        state.serialize_field("value", self.value.as_deref().unwrap_or(""))?;
        state.end()
    }
}

fn string_or_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(optional_string(deserializer)?.unwrap_or_default())
}

fn optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_str().map(ToOwned::to_owned))
}

fn python_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}
