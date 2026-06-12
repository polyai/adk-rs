use crate::local_parse::{
    NonEmptyString, ResourceParseErrors, ResourceParseResult, default_if_null, deserialize_yaml,
    duplicate_names,
};
use adk_protobuf::handoff::{
    SipByeHandoffConfig, SipConfig as ProtoSipConfig, SipHeader, SipHeaders,
    SipInviteHandoffConfig, SipReferHandoffConfig, sip_config,
};
use serde::Deserialize;
use serde::de::Error as DeError;
use serde_yaml_ng::Value;

pub(crate) const HANDOFFS_FILE_PATH: &str = "config/handoffs.yaml";
pub(crate) const HANDOFF_ITEM_PREFIX: &str = "config/handoffs.yaml/handoffs/";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct HandoffsFile {
    handoffs: Vec<HandoffItem>,
}

impl HandoffsFile {
    fn try_from_unchecked(path: &str, raw: HandoffsFileUnchecked) -> ResourceParseResult<Self> {
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
    let raw = deserialize_yaml::<HandoffsFileUnchecked>(path, yaml)?;
    HandoffsFile::try_from_unchecked(path, raw)
}

pub(crate) fn parse_handoff_items_content(
    path: &str,
    content: &str,
) -> ResourceParseResult<Vec<HandoffItem>> {
    let yaml = serde_yaml_ng::from_str::<Value>(content)
        .map_err(|error| ResourceParseErrors::single(path, error))?;
    if path == HANDOFFS_FILE_PATH {
        return deserialize_yaml::<HandoffsFileUnchecked>(path, &yaml)
            .map(|file| file.into_handoffs());
    }
    if path.starts_with(HANDOFF_ITEM_PREFIX) {
        return deserialize_yaml::<HandoffItem>(path, &yaml).map(|handoff| vec![handoff]);
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, Deserialize)]
struct HandoffsFileUnchecked {
    #[serde(default)]
    handoffs: Vec<HandoffItem>,
}

impl HandoffsFileUnchecked {
    fn into_handoffs(self) -> Vec<HandoffItem> {
        self.handoffs
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HandoffItem {
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

impl HandoffItem {
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(try_from = "SipConfigUnchecked")]
struct SipConfig {
    method: SipMethod,
    phone_number: String,
    outbound_endpoint: String,
    outbound_encryption: Option<InviteEncryption>,
}

impl SipConfig {
    fn to_proto(&self) -> ProtoSipConfig {
        let config = match self.method {
            SipMethod::Invite => sip_config::Config::Invite(SipInviteHandoffConfig {
                phone_number: self.phone_number.clone(),
                outbound_endpoint: self.outbound_endpoint.clone(),
                outbound_encryption: self
                    .outbound_encryption
                    .as_ref()
                    .map(InviteEncryption::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            SipMethod::Refer => sip_config::Config::Refer(SipReferHandoffConfig {
                phone_number: self.phone_number.clone(),
            }),
            SipMethod::Bye => sip_config::Config::Bye(SipByeHandoffConfig {}),
        };
        ProtoSipConfig {
            config: Some(config),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SipConfigUnchecked {
    #[serde(default)]
    method: SipMethod,
    #[serde(default, deserialize_with = "string_or_default")]
    phone_number: String,
    #[serde(default, deserialize_with = "string_or_default")]
    outbound_endpoint: String,
    #[serde(default)]
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
            phone_number: raw.phone_number,
            outbound_endpoint: raw.outbound_endpoint,
            outbound_encryption: raw.outbound_encryption,
        })
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

#[derive(Debug, Clone)]
enum InviteEncryption {
    TlsSrtp,
    UdpRtp,
}

impl InviteEncryption {
    fn as_str(&self) -> &'static str {
        match self {
            Self::TlsSrtp => "TLS/SRTP",
            Self::UdpRtp => "UDP/RTP",
        }
    }
}

impl<'de> Deserialize<'de> for InviteEncryption {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "TLS/SRTP" => Ok(Self::TlsSrtp),
            "UDP/RTP" => Ok(Self::UdpRtp),
            _ => Err(D::Error::custom(format!(
                "Invalid encryption method '{value}'. Must be one of: TLS/SRTP, UDP/RTP"
            ))),
        }
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
