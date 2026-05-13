//! Typed discovery matching `poly/resources/**/*.py` `discover_resources` methods.

use crate::discover::DiscoverResources;
use crate::discover::resource_utils::{
    clean_name, extract_variable_names_from_code, join_under_root, rel_under_root,
};
use serde_yaml::Value;
use std::fs::{self, ReadDir};
use std::path::{Path, PathBuf};

fn read_yaml_mapping(path: &Path) -> Option<serde_yaml::Mapping> {
    let raw = fs::read_to_string(path).ok()?;
    let v: Value = serde_yaml::from_str(&raw).ok()?;
    match v {
        Value::Mapping(m) => Some(m),
        _ => None,
    }
}

fn sorted_read_dir(dir: &Path) -> Option<Vec<PathBuf>> {
    let rd: ReadDir = fs::read_dir(dir).ok()?;
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    v.sort();
    Some(v)
}

// --- ApiIntegration (poly/resources/api_integration.py) ---
pub struct ApiIntegration;
impl DiscoverResources for ApiIntegration {
    const TYPE_NAME: &'static str = "ApiIntegration";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/api_integrations.yaml");
        if !path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
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
}

// --- Function (poly/resources/function.py) ---
pub struct Function;
impl DiscoverResources for Function {
    const TYPE_NAME: &'static str = "Function";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let flows = base_path.join("flows");
        if flows.is_dir() {
            if let Some(flow_dirs) = sorted_read_dir(&flows) {
                for flow_dir in flow_dirs {
                    if !flow_dir.is_dir() {
                        continue;
                    }
                    let flow_functions = flow_dir.join("functions");
                    if let Some(files) = sorted_read_dir(&flow_functions) {
                        for f in files {
                            if f.extension().and_then(|e| e.to_str()) == Some("py") {
                                out.push(rel_under_root(base_path, &f));
                            }
                        }
                    }
                }
            }
        }
        let global_functions = base_path.join("functions");
        if global_functions.is_dir() {
            if let Some(files) = sorted_read_dir(&global_functions) {
                for f in files {
                    if f.extension().and_then(|e| e.to_str()) == Some("py") {
                        out.push(rel_under_root(base_path, &f));
                    }
                }
            }
        }
        out
    }
}

// --- Topic (poly/resources/topic.py) ---
pub struct Topic;
impl DiscoverResources for Topic {
    const TYPE_NAME: &'static str = "Topic";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let topics = base_path.join("topics");
        if !topics.is_dir() {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(files) = sorted_read_dir(&topics) {
            for f in files {
                if let Some(ext) = f.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        out.push(rel_under_root(base_path, &f));
                    }
                }
            }
        }
        out
    }
}

// --- SettingsPersonality, SettingsRole, SettingsRules (poly/resources/agent_settings.py) ---
pub struct SettingsPersonality;
impl DiscoverResources for SettingsPersonality {
    const TYPE_NAME: &'static str = "SettingsPersonality";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/personality.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub struct SettingsRole;
impl DiscoverResources for SettingsRole {
    const TYPE_NAME: &'static str = "SettingsRole";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/role.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub struct SettingsRules;
impl DiscoverResources for SettingsRules {
    const TYPE_NAME: &'static str = "SettingsRules";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/rules.txt");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

fn flow_start_step_name(flow_dir: &Path) -> Option<String> {
    let yaml = read_yaml_mapping(&flow_dir.join("flow_config.yaml"))?;
    yaml.get("start_step")
        .and_then(|value| value.as_str())
        .map(|value| value.strip_prefix("STEP-").unwrap_or(value).to_string())
}

fn step_file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
}

// --- FlowStep, FunctionStep, FlowConfig (poly/resources/flows.py) ---
pub struct FlowStep;
impl DiscoverResources for FlowStep {
    const TYPE_NAME: &'static str = "FlowStep";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !flows_path.is_dir() {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !flow_dir.is_dir() {
                    continue;
                }
                let steps_path = flow_dir.join("steps");
                if let Some(files) = sorted_read_dir(&steps_path) {
                    let start_step = flow_start_step_name(&flow_dir);
                    let mut step_files = files
                        .into_iter()
                        .filter(|f| f.extension().and_then(|e| e.to_str()) == Some("yaml"))
                        .collect::<Vec<_>>();
                    step_files.sort_by(|left, right| {
                        let left_is_start = step_file_stem(left)
                            .as_deref()
                            .is_some_and(|name| Some(name) == start_step.as_deref());
                        let right_is_start = step_file_stem(right)
                            .as_deref()
                            .is_some_and(|name| Some(name) == start_step.as_deref());
                        right_is_start
                            .cmp(&left_is_start)
                            .then_with(|| left.cmp(right))
                    });
                    for f in step_files {
                        out.push(rel_under_root(base_path, &f));
                    }
                }
            }
        }
        out
    }
}

pub struct FunctionStep;
impl DiscoverResources for FunctionStep {
    const TYPE_NAME: &'static str = "FunctionStep";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !flows_path.is_dir() {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !flow_dir.is_dir() {
                    continue;
                }
                let function_steps_path = flow_dir.join("function_steps");
                if let Some(files) = sorted_read_dir(&function_steps_path) {
                    for f in files {
                        if f.extension().and_then(|e| e.to_str()) == Some("py") {
                            out.push(rel_under_root(base_path, &f));
                        }
                    }
                }
            }
        }
        out
    }
}

pub struct FlowConfig;
impl DiscoverResources for FlowConfig {
    const TYPE_NAME: &'static str = "FlowConfig";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let flows_path = base_path.join("flows");
        if !flows_path.is_dir() {
            return vec![];
        }
        let mut out = Vec::new();
        if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
            for flow_dir in flow_dirs {
                if !flow_dir.is_dir() {
                    continue;
                }
                let cfg = flow_dir.join("flow_config.yaml");
                if cfg.is_file() {
                    out.push(rel_under_root(base_path, &cfg));
                }
            }
        }
        out
    }
}

// --- Entity (poly/resources/entities.py) ---
pub struct Entity;
impl DiscoverResources for Entity {
    const TYPE_NAME: &'static str = "Entity";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let entities_path = base_path.join("config/entities.yaml");
        if !entities_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&entities_path) else {
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
}

// --- ExperimentalConfig (poly/resources/experimental_config.py) ---
pub struct ExperimentalConfig;
impl DiscoverResources for ExperimentalConfig {
    const TYPE_NAME: &'static str = "ExperimentalConfig";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/experimental_config.json");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

// --- GeneralSafetyFilters (poly/resources/safety_filters.py) ---
pub struct GeneralSafetyFilters;
impl DiscoverResources for GeneralSafetyFilters {
    const TYPE_NAME: &'static str = "GeneralSafetyFilters";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("agent_settings/safety_filters.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

// --- SMSTemplate (poly/resources/sms.py) ---
pub struct SMSTemplate;
impl DiscoverResources for SMSTemplate {
    const TYPE_NAME: &'static str = "SMSTemplate";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/sms_templates.yaml");
        if !path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("sms_templates") else {
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
                &path.join("sms_templates").join(&safe),
            ));
        }
        out
    }
}

// --- Handoff (poly/resources/handoff.py) ---
pub struct Handoff;
impl DiscoverResources for Handoff {
    const TYPE_NAME: &'static str = "Handoff";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/handoffs.yaml");
        if !path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
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
}

// --- Variant, VariantAttribute (poly/resources/variant_attributes.py) ---
pub struct Variant;
impl DiscoverResources for Variant {
    const TYPE_NAME: &'static str = "Variant";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/variant_attributes.yaml");
        if !path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("variants") else {
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
                &path.join("variants").join(&safe),
            ));
        }
        out
    }
}

pub struct VariantAttribute;
impl DiscoverResources for VariantAttribute {
    const TYPE_NAME: &'static str = "VariantAttribute";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let path = base_path.join("config/variant_attributes.yaml");
        if !path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("attributes") else {
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
                &path.join("attributes").join(&safe),
            ));
        }
        out
    }
}

// --- Variable (poly/resources/variable.py) ---
pub struct Variable;
impl DiscoverResources for Variable {
    const TYPE_NAME: &'static str = "Variable";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let mut function_files: Vec<PathBuf> = Vec::new();
        let global_functions = base_path.join("functions");
        if global_functions.is_dir() {
            if let Some(files) = sorted_read_dir(&global_functions) {
                for f in files {
                    if f.extension().and_then(|e| e.to_str()) == Some("py") {
                        function_files.push(f);
                    }
                }
            }
        }
        let flows_path = base_path.join("flows");
        if flows_path.is_dir() {
            if let Some(flow_dirs) = sorted_read_dir(&flows_path) {
                for flow_dir in flow_dirs {
                    if !flow_dir.is_dir() {
                        continue;
                    }
                    for sub in ["functions", "function_steps"] {
                        let d = flow_dir.join(sub);
                        if let Some(files) = sorted_read_dir(&d) {
                            for f in files {
                                if f.extension().and_then(|e| e.to_str()) == Some("py") {
                                    function_files.push(f);
                                }
                            }
                        }
                    }
                }
            }
        }
        if function_files.is_empty() {
            return vec![];
        }
        let mut names = std::collections::HashSet::new();
        for function_file in function_files {
            let Ok(code) = fs::read_to_string(&function_file) else {
                continue;
            };
            for v in extract_variable_names_from_code(&code) {
                names.insert(v);
            }
        }
        let mut out: Vec<String> = names
            .into_iter()
            .map(|n| rel_under_root(base_path, &join_under_root(base_path, &["variables", &n])))
            .collect();
        out.sort_unstable();
        out
    }
}

// --- VoiceGreeting, VoiceSafetyFilters, VoiceStylePrompt, VoiceDisclaimerMessage (poly/resources/channel_settings.py) ---
pub struct VoiceGreeting;
impl DiscoverResources for VoiceGreeting {
    const TYPE_NAME: &'static str = "VoiceGreeting";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !file_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let greeting = m.get("greeting");
        if greeting.is_none()
            || greeting.is_some_and(|g| matches!(g, Value::Null))
            || greeting.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("greeting"))]
    }
}

pub struct VoiceSafetyFilters;
impl DiscoverResources for VoiceSafetyFilters {
    const TYPE_NAME: &'static str = "VoiceSafetyFilters";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("voice/safety_filters.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub struct VoiceStylePrompt;
impl DiscoverResources for VoiceStylePrompt {
    const TYPE_NAME: &'static str = "VoiceStylePrompt";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !file_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let style = m.get("style_prompt");
        if style.is_none()
            || style.is_some_and(|g| matches!(g, Value::Null))
            || style.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("style_prompt"))]
    }
}

pub struct VoiceDisclaimerMessage;
impl DiscoverResources for VoiceDisclaimerMessage {
    const TYPE_NAME: &'static str = "VoiceDisclaimerMessage";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("voice/configuration.yaml");
        if !file_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let disclaimers = m.get("disclaimer_messages");
        let has_list = disclaimers
            .and_then(|v| v.as_sequence())
            .is_some_and(|s| !s.is_empty());
        let has_dict = disclaimers
            .and_then(|v| v.as_mapping())
            .is_some_and(|d| !d.is_empty());
        if !has_list && !has_dict {
            return vec![];
        }
        vec![rel_under_root(
            base_path,
            &file_path.join("disclaimer_messages"),
        )]
    }
}

// --- ChatGreeting, ChatSafetyFilters, ChatStylePrompt ---
pub struct ChatGreeting;
impl DiscoverResources for ChatGreeting {
    const TYPE_NAME: &'static str = "ChatGreeting";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("chat/configuration.yaml");
        if !file_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let greeting = m.get("greeting");
        if greeting.is_none()
            || greeting.is_some_and(|g| matches!(g, Value::Null))
            || greeting.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("greeting"))]
    }
}

pub struct ChatSafetyFilters;
impl DiscoverResources for ChatSafetyFilters {
    const TYPE_NAME: &'static str = "ChatSafetyFilters";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("chat/safety_filters.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

pub struct ChatStylePrompt;
impl DiscoverResources for ChatStylePrompt {
    const TYPE_NAME: &'static str = "ChatStylePrompt";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let file_path = base_path.join("chat/configuration.yaml");
        if !file_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&file_path) else {
            return vec![];
        };
        let style = m.get("style_prompt");
        if style.is_none()
            || style.is_some_and(|g| matches!(g, Value::Null))
            || style.is_some_and(|g| matches!(g, Value::Mapping(mp) if mp.is_empty()))
        {
            return vec![];
        }
        vec![rel_under_root(base_path, &file_path.join("style_prompt"))]
    }
}

// --- KeyphraseBoosting (poly/resources/keyphrase_boosting.py) ---
pub struct KeyphraseBoosting;
impl DiscoverResources for KeyphraseBoosting {
    const TYPE_NAME: &'static str = "KeyphraseBoosting";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let candidates = [
            base_path.join("voice/speech_recognition/keyphrase_boosting.yaml"),
            base_path.join("speech_recognition/keyphrase_boosting.yaml"),
        ];
        let yaml_path = candidates.into_iter().find(|p| p.is_file());
        let Some(yaml_path) = yaml_path else {
            return vec![];
        };
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("keyphrases") else {
            return vec![];
        };
        let mut out = Vec::new();
        for item in list {
            let Value::Mapping(map) = item else { continue };
            let Some(name) = map.get("keyphrase").and_then(|v| v.as_str()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let safe = clean_name(name, false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("keyphrases").join(&safe),
            ));
        }
        out
    }
}

// --- TranscriptCorrection (poly/resources/transcript_correction.py) ---
pub struct TranscriptCorrection;
impl DiscoverResources for TranscriptCorrection {
    const TYPE_NAME: &'static str = "TranscriptCorrection";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/speech_recognition/transcript_corrections.yaml");
        if !yaml_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("corrections") else {
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
                &yaml_path.join("corrections").join(&safe),
            ));
        }
        out
    }
}

// --- AsrSettings (poly/resources/asr_settings.py) ---
pub struct AsrSettings;
impl DiscoverResources for AsrSettings {
    const TYPE_NAME: &'static str = "AsrSettings";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let p = base_path.join("voice/speech_recognition/asr_settings.yaml");
        if p.is_file() {
            vec![rel_under_root(base_path, &p)]
        } else {
            vec![]
        }
    }
}

// --- PhraseFilter (poly/resources/phrase_filter.py) ---
pub struct PhraseFilter;
impl DiscoverResources for PhraseFilter {
    const TYPE_NAME: &'static str = "PhraseFilter";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/response_control/phrase_filtering.yaml");
        if !yaml_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(list)) = m.get("phrase_filtering") else {
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
                &yaml_path.join("phrase_filtering").join(&safe),
            ));
        }
        out
    }
}

// --- Pronunciation (poly/resources/pronunciation.py) ---
pub struct Pronunciation;
impl DiscoverResources for Pronunciation {
    const TYPE_NAME: &'static str = "Pronunciation";

    fn discover_resources(base_path: &Path) -> Vec<String> {
        let yaml_path = base_path.join("voice/response_control/pronunciations.yaml");
        if !yaml_path.is_file() {
            return vec![];
        }
        let Some(m) = read_yaml_mapping(&yaml_path) else {
            return vec![];
        };
        let Some(Value::Sequence(items)) = m.get("pronunciations") else {
            return vec![];
        };
        let mut out = Vec::new();
        for (i, _item) in items.iter().enumerate() {
            let safe = clean_name(&i.to_string(), false);
            out.push(rel_under_root(
                base_path,
                &yaml_path.join("pronunciations").join(&safe),
            ));
        }
        out
    }
}
