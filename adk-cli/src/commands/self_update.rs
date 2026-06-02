use crate::SelfUpdateArgs;
use crate::console;
use crate::install_helpers::{CARGO_DIST_APP_NAME, check_release_installer_preflight};
use axoupdater::AxoUpdater;
use std::process::ExitCode;

pub(crate) fn cmd_self_update(args: SelfUpdateArgs) -> ExitCode {
    let mut updater = AxoUpdater::new_for(CARGO_DIST_APP_NAME);

    if !check_release_installer_preflight(&mut updater, "Self-update", args.verbose) {
        return ExitCode::from(1);
    }

    configure_update_auth(&mut updater);
    console::info("Checking for ADK updates...");

    match updater.run_sync() {
        Ok(Some(result)) => {
            let old_version = result
                .old_version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown".to_string());
            console::success(format!(
                "Updated ADK from {old_version} to {}.",
                result.new_version
            ));
            ExitCode::SUCCESS
        }
        Ok(None) => {
            console::success(format!(
                "ADK is already up to date ({}).",
                env!("CARGO_PKG_VERSION")
            ));
            ExitCode::SUCCESS
        }
        Err(error) => {
            console::exception(format!("Failed to update ADK: {error}"));
            ExitCode::from(1)
        }
    }
}

fn configure_update_auth(updater: &mut AxoUpdater) {
    if let Ok(token) = std::env::var("POLY_ADK_GITHUB_TOKEN")
        .or_else(|_| std::env::var("AXOUPDATER_GITHUB_TOKEN"))
    {
        updater.set_github_token(&token);
    }
}
