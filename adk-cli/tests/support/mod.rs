#![allow(dead_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod cli {
    use super::*;

    pub const PROJECT_CONFIG_YAML: &str =
        "region: eu-west-1\naccount_id: test\nproject_id: proj\nbranch_id: main\n";

    pub fn poly_bin() -> &'static str {
        env!("CARGO_BIN_EXE_poly")
    }

    pub fn poly_offline_command() -> Command {
        let mut command = Command::new(poly_bin());
        command.env_remove("POLY_ADK_KEY");
        command.env_remove("GITHUB_ACCESS_TOKEN");
        command.env("POLY_ADK_ALLOW_INMEMORY_FALLBACK", "1");
        command
    }

    pub fn poly_without_fallback_command() -> Command {
        let mut command = Command::new(poly_bin());
        command.env_remove("POLY_ADK_KEY");
        command.env_remove("GITHUB_ACCESS_TOKEN");
        command.env_remove("POLY_ADK_ALLOW_INMEMORY_FALLBACK");
        command
    }

    pub fn run_poly_offline(args: &[&str]) -> Output {
        poly_offline_command()
            .args(args)
            .output()
            .expect("failed to execute poly")
    }

    pub fn run_poly_without_fallback(args: &[&str]) -> Output {
        poly_without_fallback_command()
            .args(args)
            .output()
            .expect("failed to execute poly")
    }

    pub fn temp_dir(prefix: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{ts}"))
    }

    pub fn make_temp_project_dir(prefix: &str) -> String {
        let dir = temp_dir(prefix);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("project.yaml"), PROJECT_CONFIG_YAML).expect("write config");
        dir.to_string_lossy().to_string()
    }

    pub fn make_temp_invalid_yaml_project_dir(prefix: &str) -> String {
        let dir = make_temp_project_dir(prefix);
        let path = PathBuf::from(&dir);
        fs::create_dir_all(path.join("topics")).expect("mkdir topics");
        fs::write(
            path.join("topics/bad.yaml"),
            "name: bad\ncontent: [unterminated\n",
        )
        .expect("write invalid yaml");
        dir
    }

    pub fn make_temp_unformatted_json_project_dir(prefix: &str) -> String {
        let dir = make_temp_project_dir(prefix);
        fs::write(PathBuf::from(&dir).join("sample.json"), "{\"b\":2,\"a\":1}")
            .expect("write unformatted json");
        dir
    }

    pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(src_path, dst_path)?;
            }
        }
        Ok(())
    }
}

pub mod python_recordings {
    use super::*;
    use httpmock::MockServer;

    pub const PYTHON_ADK_BIN_ENV: &str = "PYTHON_ADK_BIN";
    pub const RECORDING_FIXTURE_DIR: &str = "tests/fixtures/python-adk-recordings";
    pub const TARGET_REGION: &str = "us-1";
    pub const TARGET_ACCOUNT_ID: &str = "ben-ws";
    pub const TARGET_PROJECT_ID: &str = "PROJECT-JTQKOKLM";
    pub const TARGET_PROJECT_NAME: &str = "Test";
    pub const SCENARIOS: &[&str] = &[
        "basic-readonly",
        "branch-merge-main",
        "branch-update-push",
        "create-delete-dryrun",
        "dirty-switch",
        "main-push",
        "merge-conflict-resolution",
        "pull-conflict",
        "revert-local",
        "validation-errors",
    ];

    pub fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RECORDING_FIXTURE_DIR)
    }

    pub fn httpmock_adk_base_url(server: &MockServer) -> String {
        format!("{}/adk/v1", server.base_url())
    }

    pub fn temp_recording_dir() -> PathBuf {
        temp_dir("adk-rs-python-adk-recording")
    }

    pub fn temp_replay_dir(scenario: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("adk-rs-python-adk-replay-{scenario}-{ts}"))
    }

    pub fn recording_run_id() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        format!("{:x}", ts & 0xffff_ffff)
    }

    pub fn python_adk_bin() -> String {
        std::env::var(PYTHON_ADK_BIN_ENV).unwrap_or_else(|_| "poly".to_string())
    }

    pub fn replace_all(input: &str, replacements: &[(String, String)]) -> String {
        replacements
            .iter()
            .fold(input.to_string(), |value, (from, to)| {
                value.replace(from, to)
            })
    }

    pub fn substitute_json(
        value: &Value,
        substitutions: &[(String, String)],
        actual: Option<&Value>,
    ) -> Value {
        match value {
            Value::String(text) => {
                let substituted = replace_all(text, substitutions);
                if is_recording_placeholder(&substituted)
                    && let Some(Value::String(actual_text)) = actual
                {
                    return Value::String(actual_text.clone());
                }
                Value::String(substituted)
            }
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let actual_item = actual
                            .and_then(Value::as_array)
                            .and_then(|items| items.get(idx));
                        substitute_json(item, substitutions, actual_item)
                    })
                    .collect(),
            ),
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        let substituted_key = replace_all(key, substitutions);
                        let actual_value = actual
                            .and_then(Value::as_object)
                            .and_then(|object| object.get(&substituted_key));
                        (
                            substituted_key,
                            substitute_json(value, substitutions, actual_value),
                        )
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    pub fn lookup_substitution(name: &str, substitutions: &[(String, String)]) -> String {
        substitutions
            .iter()
            .find_map(|(from, to)| (from == name).then(|| to.clone()))
            .unwrap_or_else(|| panic!("missing substitution {name}"))
    }

    pub fn maybe_lookup_substitution(
        name: &str,
        substitutions: &[(String, String)],
    ) -> Option<String> {
        substitutions
            .iter()
            .find_map(|(from, to)| (from == name).then(|| to.clone()))
    }

    fn is_recording_placeholder(text: &str) -> bool {
        matches!(text, "${TIMESTAMP}" | "${COMMAND_ID}")
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{ts}"))
    }
}
