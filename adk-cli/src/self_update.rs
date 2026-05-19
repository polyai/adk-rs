use crate::{SelfUpdateArgs, console};
use axoupdater::AxoUpdater;
use std::process::ExitCode;

const CARGO_DIST_APP_NAME: &str = "poly-adk";

pub(crate) fn cmd_self_update(args: SelfUpdateArgs) -> ExitCode {
    let mut updater = AxoUpdater::new_for(CARGO_DIST_APP_NAME);

    if let Err(error) = updater.load_receipt() {
        emit_update_preflight_error(
            args.verbose,
            format!("Could not load the cargo-dist install receipt for {CARGO_DIST_APP_NAME}: {error}."),
        );
        return ExitCode::from(1);
    }

    match updater.check_receipt_is_for_this_executable() {
        Ok(true) => {}
        Ok(false) => {
            emit_update_preflight_error(
                args.verbose,
                "Found a release-installer receipt, but this `poly` executable is not the one that receipt installed."
                    .to_string(),
            );
            return ExitCode::from(1);
        }
        Err(error) => {
            emit_update_preflight_error(
                args.verbose,
                format!(
                    "Could not verify that this `poly` executable came from the release shell installer: {error}."
                ),
            );
            return ExitCode::from(1);
        }
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

fn emit_update_preflight_error(verbose: bool, detail: String) {
    console::error(
        "Self-update is only supported for ADK installs that were installed via shell; no shell install receipt was found.",
    );
    if verbose {
        console::plain_stderr(format!("[muted]Details: {detail}[/muted]"));
    }
}
