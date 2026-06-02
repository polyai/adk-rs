use crate::install_helpers::{
    CARGO_DIST_APP_NAME, check_release_installer_preflight, emit_release_installer_preflight_error,
};
use crate::{UninstallArgs, console, emit_error, prompt_confirm};
use axoupdater::AxoUpdater;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(crate) fn cmd_uninstall(args: UninstallArgs) -> ExitCode {
    let mut updater = AxoUpdater::new_for(CARGO_DIST_APP_NAME);

    if !check_release_installer_preflight(&mut updater, "Uninstall", args.verbose) {
        return ExitCode::from(1);
    }

    let receipt_path = match find_receipt_path(CARGO_DIST_APP_NAME) {
        Ok(path) => path,
        Err(error) => {
            emit_release_installer_preflight_error("Uninstall", args.verbose, error);
            return ExitCode::from(1);
        }
    };
    let receipt = match load_cargo_dist_install_receipt(&receipt_path) {
        Ok(receipt) => receipt,
        Err(error) => {
            emit_release_installer_preflight_error("Uninstall", args.verbose, error);
            return ExitCode::from(1);
        }
    };
    let targets = match uninstall_targets(&receipt) {
        Ok(targets) if targets.is_empty() => {
            console::error("The shell install receipt did not list any ADK binaries to remove.");
            return ExitCode::from(1);
        }
        Ok(targets) => targets,
        Err(error) => {
            console::error(error);
            return ExitCode::from(1);
        }
    };

    if let Err(error) = current_executable_matches_install(&targets) {
        console::error(error);
        return ExitCode::from(1);
    }

    console::plain(format!(
        "ADK release install: {}",
        receipt.install_prefix.display()
    ));
    console::plain("The following files will be removed:");
    for target in &targets {
        console::plain(format!("  {}", target.path.display()));
    }
    console::plain(format!("  {}", receipt_path.display()));

    if !args.yes {
        match prompt_confirm("Uninstall ADK?") {
            Ok(true) => {}
            Ok(false) => {
                console::warning("Aborted.");
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        }
    }

    match remove_uninstall_targets(&targets, &receipt_path) {
        Ok(removed) => {
            console::success(format!("Uninstalled ADK and removed {removed} file(s)."));
            ExitCode::SUCCESS
        }
        Err(error) => {
            console::exception(format!("Failed to uninstall ADK: {error}"));
            ExitCode::from(1)
        }
    }
}

// cargo-dist's shell installer writes this JSON receipt, but the file format is
// not documented as a stable public API. Keep this direct parser narrowly
// scoped to uninstall and revisit it if axoupdater exposes a stable receipt API
// with install layout and alias fields.
#[derive(Debug, Deserialize)]
struct CargoDistInstallReceipt {
    install_prefix: PathBuf,
    #[serde(default)]
    install_layout: InstallLayout,
    #[serde(default)]
    binaries: Vec<String>,
    #[serde(default)]
    binary_aliases: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum InstallLayout {
    Flat,
    Hierarchical,
    CargoHome,
    #[default]
    Unspecified,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallTarget {
    path: PathBuf,
    kind: UninstallTargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UninstallTargetKind {
    Alias { target: PathBuf },
    Binary,
}

fn load_cargo_dist_install_receipt(path: &Path) -> Result<CargoDistInstallReceipt, String> {
    let contents = fs::read_to_string(path).map_err(|error| {
        format!(
            "Could not read shell install receipt {}: {error}.",
            path.display()
        )
    })?;
    serde_json::from_str(&contents).map_err(|error| {
        format!(
            "Could not parse shell install receipt {}: {error}.",
            path.display()
        )
    })
}

fn find_receipt_path(app_name: &str) -> Result<PathBuf, String> {
    for config_dir in receipt_config_dirs(app_name)? {
        let receipt_path = config_dir.join(format!("{app_name}-receipt.json"));
        if receipt_path.exists() {
            return Ok(receipt_path);
        }
    }
    Err(format!(
        "Could not find the cargo-dist install receipt for {app_name}."
    ))
}

fn receipt_config_dirs(app_name: &str) -> Result<Vec<PathBuf>, String> {
    if std::env::var_os("AXOUPDATER_CONFIG_WORKING_DIR").is_some() {
        return std::env::current_dir().map(|path| vec![path]).map_err(|error| {
            format!("Could not inspect the current directory for the shell install receipt: {error}.")
        });
    }

    if let Some(path) = std::env::var_os("AXOUPDATER_CONFIG_PATH") {
        return Ok(vec![PathBuf::from(path)]);
    }

    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(path).join(app_name);
        if path.exists() {
            dirs.push(path);
        }
    }

    #[cfg(windows)]
    {
        if let Some(path) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(path).join(app_name));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(path) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(path).join(".config").join(app_name));
        }
    }

    if dirs.is_empty() {
        return Err(format!(
            "Could not determine where cargo-dist install receipts are stored for {app_name}."
        ));
    }
    Ok(dirs)
}

fn uninstall_targets(receipt: &CargoDistInstallReceipt) -> Result<Vec<UninstallTarget>, String> {
    let install_dir = receipt_binary_dir(receipt);
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();

    for binary in &receipt.binaries {
        let binary_path = install_dir.join(receipt_file_name(binary)?);
        if let Some(aliases) = receipt.binary_aliases.get(binary) {
            for alias in aliases {
                let alias_path = install_dir.join(receipt_file_name(alias)?);
                if seen.insert(alias_path.clone()) {
                    targets.push(UninstallTarget {
                        path: alias_path,
                        kind: UninstallTargetKind::Alias {
                            target: binary_path.clone(),
                        },
                    });
                }
            }
        }
        if seen.insert(binary_path.clone()) {
            targets.push(UninstallTarget {
                path: binary_path,
                kind: UninstallTargetKind::Binary,
            });
        }
    }

    Ok(targets)
}

fn receipt_binary_dir(receipt: &CargoDistInstallReceipt) -> PathBuf {
    match receipt.install_layout {
        InstallLayout::CargoHome | InstallLayout::Hierarchical => receipt.install_prefix.join("bin"),
        InstallLayout::Flat => receipt.install_prefix.clone(),
        InstallLayout::Unspecified | InstallLayout::Unknown => {
            let bin_dir = receipt.install_prefix.join("bin");
            if receipt
                .binaries
                .first()
                .is_some_and(|binary| bin_dir.join(binary).exists())
            {
                bin_dir
            } else {
                receipt.install_prefix.clone()
            }
        }
    }
}

fn receipt_file_name(value: &str) -> Result<&OsStr, String> {
    let path = Path::new(value);
    if path.components().count() == 1
        && path.file_name().is_some()
        && path.file_name() == Some(path.as_os_str())
        && !value.contains(['/', '\\'])
    {
        return Ok(path.as_os_str());
    }
    Err(format!(
        "Refusing to uninstall receipt entry with an unsafe path: {value}"
    ))
}

fn current_executable_matches_install(targets: &[UninstallTarget]) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("Could not inspect the running `poly` executable: {error}."))?;
    let current_exe = canonical_or_original(current_exe);

    for target in targets {
        if target.kind == UninstallTargetKind::Binary
            && target.path.exists()
            && canonical_or_original(target.path.clone()) == current_exe
        {
            return Ok(());
        }
    }

    Err(
        "Refusing to uninstall because the running `poly` executable does not match any binary listed in the shell install receipt."
            .to_string(),
    )
}

fn remove_uninstall_targets(
    targets: &[UninstallTarget],
    receipt_path: &Path,
) -> Result<usize, String> {
    let mut removed = 0;
    for target in targets {
        if !target.path.exists() && !target.path.is_symlink() {
            continue;
        }
        if let UninstallTargetKind::Alias { target: binary } = &target.kind
            && !alias_points_to_binary(&target.path, binary)
        {
            console::warning(format!(
                "Skipping alias {} because it no longer points to {}.",
                target.path.display(),
                binary.display()
            ));
            continue;
        }
        fs::remove_file(&target.path)
            .map_err(|error| format!("Could not remove {}: {error}.", target.path.display()))?;
        removed += 1;
    }

    if receipt_path.exists() {
        fs::remove_file(receipt_path)
            .map_err(|error| format!("Could not remove {}: {error}.", receipt_path.display()))?;
        removed += 1;
    }

    Ok(removed)
}

fn alias_points_to_binary(alias: &Path, binary: &Path) -> bool {
    let Ok(target) = fs::read_link(alias) else {
        return false;
    };
    let target = if target.is_absolute() {
        target
    } else {
        alias
            .parent()
            .map(|parent| parent.join(&target))
            .unwrap_or(target)
    };
    canonical_or_original(target) == canonical_or_original(binary.to_path_buf())
}

fn canonical_or_original(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard};
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        _lock: MutexGuard<'static, ()>,
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvVarGuard {
        fn new(vars: &[&'static str]) -> Self {
            let lock = ENV_LOCK.lock().expect("env lock");
            let saved = vars
                .iter()
                .map(|&name| (name, std::env::var_os(name)))
                .collect();
            Self { _lock: lock, saved }
        }

        fn set(&self, name: &'static str, value: &Path) {
            unsafe {
                std::env::set_var(name, value);
            }
        }

        fn remove(&self, name: &'static str) {
            unsafe {
                std::env::remove_var(name);
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(name, value),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    fn axoupdater_config_env() -> EnvVarGuard {
        let guard = EnvVarGuard::new(&[
            "AXOUPDATER_CONFIG_WORKING_DIR",
            "AXOUPDATER_CONFIG_PATH",
        ]);
        guard.remove("AXOUPDATER_CONFIG_WORKING_DIR");
        guard.remove("AXOUPDATER_CONFIG_PATH");
        guard
    }

    fn receipt(prefix: &str, layout: InstallLayout) -> CargoDistInstallReceipt {
        CargoDistInstallReceipt {
            install_prefix: PathBuf::from(prefix),
            install_layout: layout,
            binaries: vec!["poly".to_string()],
            binary_aliases: BTreeMap::from([("poly".to_string(), vec!["adk".to_string()])]),
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{ts}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn load_cargo_dist_install_receipt_parses_installer_json() {
        let dir = temp_dir("adk-rs-load-install-receipt");
        let receipt_path = dir.join("poly-adk-receipt.json");
        fs::write(
            &receipt_path,
            r#"{
                "install_prefix": "/opt/poly-adk",
                "install_layout": "flat",
                "binaries": ["poly"],
                "binary_aliases": { "poly": ["adk"] }
            }"#,
        )
        .expect("write receipt");

        let receipt = load_cargo_dist_install_receipt(&receipt_path).expect("parse receipt");

        assert_eq!(receipt.install_prefix, PathBuf::from("/opt/poly-adk"));
        assert_eq!(receipt.install_layout, InstallLayout::Flat);
        assert_eq!(receipt.binaries, vec!["poly"]);
        assert_eq!(
            receipt.binary_aliases,
            BTreeMap::from([("poly".to_string(), vec!["adk".to_string()])])
        );
    }

    #[test]
    fn find_receipt_path_uses_axoupdater_config_path() {
        let env = axoupdater_config_env();
        let config_dir = temp_dir("adk-rs-find-install-receipt");
        let receipt_path = config_dir.join("poly-adk-receipt.json");
        fs::write(&receipt_path, "{}").expect("write receipt marker");
        env.set("AXOUPDATER_CONFIG_PATH", &config_dir);

        assert_eq!(
            find_receipt_path("poly-adk").expect("receipt path"),
            receipt_path
        );
    }

    #[test]
    fn uninstall_targets_use_cargo_home_bin_dir_and_aliases_first() {
        let targets = uninstall_targets(&receipt("/home/test/.cargo", InstallLayout::CargoHome))
            .expect("targets");

        assert_eq!(
            targets,
            vec![
                UninstallTarget {
                    path: PathBuf::from("/home/test/.cargo/bin/adk"),
                    kind: UninstallTargetKind::Alias {
                        target: PathBuf::from("/home/test/.cargo/bin/poly")
                    },
                },
                UninstallTarget {
                    path: PathBuf::from("/home/test/.cargo/bin/poly"),
                    kind: UninstallTargetKind::Binary,
                },
            ]
        );
    }

    #[test]
    fn uninstall_targets_use_flat_install_prefix() {
        let targets =
            uninstall_targets(&receipt("/opt/poly-adk", InstallLayout::Flat)).expect("targets");

        assert_eq!(targets[0].path, PathBuf::from("/opt/poly-adk/adk"));
        assert_eq!(targets[1].path, PathBuf::from("/opt/poly-adk/poly"));
    }

    #[test]
    fn uninstall_rejects_receipt_entries_with_path_components() {
        let mut receipt = receipt("/home/test/.cargo", InstallLayout::CargoHome);
        receipt.binaries = vec!["../poly".to_string()];

        let error = uninstall_targets(&receipt).expect_err("unsafe receipt path rejected");
        assert!(error.contains("unsafe path"));
    }

    #[test]
    fn unspecified_layout_uses_bin_dir_when_binary_exists_there() {
        let dir = temp_dir("adk-rs-unspecified-install-layout");
        let bin_dir = dir.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::write(bin_dir.join("poly"), "").expect("write binary marker");
        let receipt = CargoDistInstallReceipt {
            install_prefix: dir.clone(),
            install_layout: InstallLayout::Unspecified,
            binaries: vec!["poly".to_string()],
            binary_aliases: BTreeMap::new(),
        };

        assert_eq!(receipt_binary_dir(&receipt), bin_dir);
    }

    #[test]
    fn current_executable_matches_install_accepts_running_binary() {
        let current_exe = std::env::current_exe().expect("current exe");
        let targets = vec![UninstallTarget {
            path: current_exe,
            kind: UninstallTargetKind::Binary,
        }];

        current_executable_matches_install(&targets).expect("current exe matches");
    }

    #[test]
    fn current_executable_matches_install_rejects_other_binary() {
        let dir = temp_dir("adk-rs-current-exe-mismatch");
        let other_binary = dir.join("poly");
        fs::write(&other_binary, "").expect("write other binary marker");
        let targets = vec![UninstallTarget {
            path: other_binary,
            kind: UninstallTargetKind::Binary,
        }];

        let error =
            current_executable_matches_install(&targets).expect_err("other binary rejected");
        assert!(error.contains("running `poly` executable does not match"));
    }

    #[cfg(unix)]
    #[test]
    fn remove_uninstall_targets_removes_alias_binary_and_receipt() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("adk-rs-remove-uninstall-targets");
        let binary = dir.join("poly");
        let alias = dir.join("adk");
        let receipt_path = dir.join("poly-adk-receipt.json");
        fs::write(&binary, "").expect("write binary marker");
        fs::write(&receipt_path, "{}").expect("write receipt");
        symlink(&binary, &alias).expect("link alias");
        let targets = vec![
            UninstallTarget {
                path: alias.clone(),
                kind: UninstallTargetKind::Alias {
                    target: binary.clone(),
                },
            },
            UninstallTarget {
                path: binary.clone(),
                kind: UninstallTargetKind::Binary,
            },
        ];

        let removed = remove_uninstall_targets(&targets, &receipt_path).expect("remove targets");

        assert_eq!(removed, 3);
        assert!(!alias.exists());
        assert!(!binary.exists());
        assert!(!receipt_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn remove_uninstall_targets_skips_alias_that_points_elsewhere() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("adk-rs-skip-stale-alias");
        let binary = dir.join("poly");
        let alias = dir.join("adk");
        let other_binary = dir.join("other-poly");
        let missing = dir.join("missing-poly");
        let receipt_path = dir.join("poly-adk-receipt.json");
        fs::write(&binary, "").expect("write binary marker");
        fs::write(&other_binary, "").expect("write other binary marker");
        fs::write(&receipt_path, "{}").expect("write receipt");
        symlink(&other_binary, &alias).expect("link stale alias");
        let targets = vec![
            UninstallTarget {
                path: missing,
                kind: UninstallTargetKind::Binary,
            },
            UninstallTarget {
                path: alias.clone(),
                kind: UninstallTargetKind::Alias {
                    target: binary.clone(),
                },
            },
            UninstallTarget {
                path: binary.clone(),
                kind: UninstallTargetKind::Binary,
            },
        ];

        let removed = remove_uninstall_targets(&targets, &receipt_path).expect("remove targets");

        assert_eq!(removed, 2);
        assert!(alias.exists());
        assert!(!binary.exists());
        assert!(!receipt_path.exists());
    }
}
