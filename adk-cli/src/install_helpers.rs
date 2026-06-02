use crate::console;
use axoupdater::AxoUpdater;

pub(crate) const CARGO_DIST_APP_NAME: &str = "poly-adk";

pub(crate) fn check_release_installer_preflight(
    updater: &mut AxoUpdater,
    operation: &str,
    verbose: bool,
) -> bool {
    if let Err(error) = updater.load_receipt() {
        emit_release_installer_preflight_error(
            operation,
            verbose,
            format!(
                "Could not load the cargo-dist install receipt for {CARGO_DIST_APP_NAME}: {error}."
            ),
        );
        return false;
    }

    match updater.check_receipt_is_for_this_executable() {
        Ok(true) => true,
        Ok(false) => {
            emit_release_installer_preflight_error(
                operation,
                verbose,
                "Found a release-installer receipt, but this `poly` executable is not the one that receipt installed."
                    .to_string(),
            );
            false
        }
        Err(error) => {
            emit_release_installer_preflight_error(
                operation,
                verbose,
                format!(
                    "Could not verify that this `poly` executable came from the release shell installer: {error}."
                ),
            );
            false
        }
    }
}

pub(crate) fn emit_release_installer_preflight_error(
    operation: &str,
    verbose: bool,
    detail: String,
) {
    console::error(format!(
        "{operation} is only supported for ADK installs that were installed via shell; no shell install receipt was found."
    ));
    if verbose {
        console::plain_stderr(format!("[muted]Details: {detail}[/muted]"));
    }
}
