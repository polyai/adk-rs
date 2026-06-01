use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct CredentialsFile {
    #[serde(flatten)]
    api_keys: IndexMap<String, String>,
}

impl CredentialsFile {
    fn api_key(&self, region: &str) -> Option<String> {
        self.api_keys
            .get(region)
            .filter(|key| !key.trim().is_empty())
            .cloned()
    }

    fn any_api_keys(&self) -> bool {
        self.api_keys.values().any(|key| !key.trim().is_empty())
    }

    fn insert_api_key(&mut self, region: &str, api_key: &str) {
        self.api_keys
            .insert(region.to_string(), api_key.to_string());
    }
}

pub(crate) fn api_key_for_region(region: &str) -> Result<String, String> {
    api_key_for_region_from(
        region,
        || credentials_file_path().ok(),
        |name| env::var(name).ok(),
    )
}

pub(crate) fn any_credentials_exist() -> bool {
    if all_api_key_env_names()
        .iter()
        .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
    {
        return true;
    }
    credentials_file_path()
        .ok()
        .and_then(|path| read_credentials_file(&path).ok())
        .is_some_and(|credentials| credentials.any_api_keys())
}

pub(crate) fn save_api_key_credential_file(
    api_key: &str,
    region: &str,
) -> Result<PathBuf, String> {
    let path = credentials_file_path()?;
    save_api_key_credential_file_at(&path, api_key, region)?;
    Ok(path)
}

pub(crate) fn mask_api_key(api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.len() <= 8 {
        return "****".to_string();
    }
    format!("{}****{}", &api_key[..4], &api_key[api_key.len() - 4..])
}

fn api_key_for_region_from(
    region: &str,
    credentials_file_path: impl Fn() -> Option<PathBuf>,
    env_var: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    if let Some(path) = credentials_file_path()
        && let Some(value) = read_api_key_from_credential_file_at(&path, region)
    {
        return Ok(value);
    }

    for name in api_key_env_names(region) {
        if let Some(value) = env_var(name)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }
    }

    Err(format!(
        "No API key found for region {region}. Run poly start or poly login, or set POLY_ADK_KEY."
    ))
}

fn read_api_key_from_credential_file_at(path: &Path, region: &str) -> Option<String> {
    let credentials = read_credentials_file(path).ok()?;
    credentials.api_key(region)
}

fn save_api_key_credential_file_at(
    path: &Path,
    api_key: &str,
    region: &str,
) -> Result<(), String> {
    let mut credentials = read_credentials_file_for_update(path)?;
    credentials.insert_api_key(region, api_key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create credentials directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let contents = serde_json::to_string_pretty(&credentials)
        .map_err(|error| format!("Failed to serialize credentials: {error}"))?;
    fs::write(path, contents).map_err(|error| {
        format!(
            "Failed to write credentials file {}: {error}",
            path.display()
        )
    })?;
    restrict_credentials_permissions(path)?;
    Ok(())
}

fn read_credentials_file(path: &Path) -> Result<CredentialsFile, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read credentials file {}: {error}", path.display()))?;
    parse_credentials_file(path, &contents)
}

fn read_credentials_file_for_update(path: &Path) -> Result<CredentialsFile, String> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_credentials_file(path, &contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CredentialsFile::default()),
        Err(error) => Err(format!(
            "Failed to read credentials file {}: {error}",
            path.display()
        )),
    }
}

fn parse_credentials_file(path: &Path, contents: &str) -> Result<CredentialsFile, String> {
    serde_json::from_str(contents).map_err(|error| {
        format!(
            "Failed to parse credentials file {}: {error}",
            path.display()
        )
    })
}

fn credentials_file_path() -> Result<PathBuf, String> {
    home_dir()
        .map(|home| home.join(".poly").join("credentials.json"))
        .ok_or_else(|| "Unable to find home directory for credential storage.".to_string())
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

fn api_key_env_names(region: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    match region {
        "us-1" => names.push("POLY_ADK_KEY_US"),
        "euw-1" => names.push("POLY_ADK_KEY_EUW"),
        "uk-1" => names.push("POLY_ADK_KEY_UK"),
        "studio" => names.push("POLY_ADK_KEY_STUDIO"),
        "staging" => names.push("POLY_ADK_KEY_STAGING"),
        "dev" => names.push("POLY_ADK_KEY_DEV"),
        _ => {}
    }
    names.push("POLY_ADK_KEY");
    names
}

fn all_api_key_env_names() -> Vec<&'static str> {
    let mut names = vec!["POLY_ADK_KEY"];
    for region in ["us-1", "euw-1", "uk-1", "studio", "staging", "dev"] {
        for name in api_key_env_names(region) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(unix)]
fn restrict_credentials_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|error| {
        format!(
            "Failed to restrict credentials file permissions {}: {error}",
            path.display()
        )
    })
}

#[cfg(not(unix))]
fn restrict_credentials_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn save_api_key_merges_regions_and_restricts_permissions() {
        let path = temp_credentials_file();
        save_api_key_credential_file_at(&path, "studio-key", "studio").expect("save studio key");
        save_api_key_credential_file_at(&path, "us-key", "us-1").expect("save us key");

        let credentials = read_credentials_file(&path).expect("credentials");

        assert_eq!(credentials.api_key("studio").as_deref(), Some("studio-key"));
        assert_eq!(credentials.api_key("us-1").as_deref(), Some("us-key"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn save_api_key_refuses_to_overwrite_malformed_credentials() {
        let path = temp_credentials_file();
        fs::create_dir_all(path.parent().unwrap()).expect("credentials dir");
        fs::write(&path, "{not-json").expect("write malformed credentials");

        let error = save_api_key_credential_file_at(&path, "studio-key", "studio")
            .expect_err("malformed credentials should not be overwritten");

        assert!(error.contains("Failed to parse credentials file"));
        assert_eq!(
            fs::read_to_string(&path).expect("credentials still exist"),
            "{not-json"
        );

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn save_api_key_rejects_non_string_credential_values() {
        let path = temp_credentials_file();
        fs::create_dir_all(path.parent().unwrap()).expect("credentials dir");
        fs::write(&path, r#"{"us-1":123}"#).expect("write typed-invalid credentials");

        let error = save_api_key_credential_file_at(&path, "studio-key", "studio")
            .expect_err("non-string credentials should not be overwritten");

        assert!(error.contains("Failed to parse credentials file"));
        assert_eq!(
            fs::read_to_string(&path).expect("credentials still exist"),
            r#"{"us-1":123}"#
        );

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn credential_file_has_priority_over_environment_values() {
        let path = temp_credentials_file();
        save_api_key_credential_file_at(&path, "file-key", "studio").expect("save key");

        let key = api_key_for_region_from(
            "studio",
            || Some(path.clone()),
            |name| (name == "POLY_ADK_KEY_STUDIO").then(|| "env-key".to_string()),
        )
        .expect("resolved key");

        assert_eq!(key, "file-key");

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn region_environment_has_priority_over_global_environment() {
        let key = api_key_for_region_from(
            "uk-1",
            || None,
            |name| match name {
                "POLY_ADK_KEY_UK" => Some("region-key".to_string()),
                "POLY_ADK_KEY" => Some("global-key".to_string()),
                _ => None,
            },
        )
        .expect("resolved key");

        assert_eq!(key, "region-key");
    }

    #[test]
    fn mask_api_key_keeps_a_small_identifier_without_leaking_the_secret() {
        assert_eq!(mask_api_key("short"), "****");
        assert_eq!(mask_api_key("abcd1234wxyz"), "abcd****wxyz");
    }

    #[test]
    fn all_api_key_env_names_include_region_specific_and_global_names() {
        assert_eq!(
            all_api_key_env_names(),
            vec![
                "POLY_ADK_KEY",
                "POLY_ADK_KEY_US",
                "POLY_ADK_KEY_EUW",
                "POLY_ADK_KEY_UK",
                "POLY_ADK_KEY_STUDIO",
                "POLY_ADK_KEY_STAGING",
                "POLY_ADK_KEY_DEV",
            ]
        );
    }

    fn temp_credentials_file() -> PathBuf {
        let index = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!(
                "adk-rs-credentials-test-{}-{index}",
                std::process::id()
            ))
            .join(".poly")
            .join("credentials.json")
    }
}
