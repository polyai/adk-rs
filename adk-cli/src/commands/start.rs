use crate::{
    ProjectCreateArgs, ProjectWorkspace, StartArgs, console, credentials, prompt_confirm_default,
    wait_for_enter,
};
use super::login::{sign_in_and_save_key, wait_for_api_key_active};
use super::project::cmd_project_create;
use std::process::ExitCode;

const START_REGION: &str = "studio";

pub(crate) fn cmd_start(workspace: &ProjectWorkspace, args: StartArgs) -> ExitCode {
    console::print_welcome_message();
    console::plain(
        "This will guide you through setting up your API key and creating a new project in Agent Studio.",
    );
    if let Err(error) = wait_for_enter("Press any key to continue...") {
        crate::emit_error(false, &error);
        return ExitCode::from(1);
    }

    if credentials::any_credentials_exist() {
        console::warning("An existing API key was found in your environment.");
        match prompt_confirm_default("Do you want to continue with the existing key?", true) {
            Ok(true) => {
                console::success("Continuing with existing API key.");
                return maybe_create_project(workspace, args, None);
            }
            Ok(false) => {}
            Err(error) => {
                crate::emit_error(false, &error);
                return ExitCode::from(1);
            }
        }
    }

    let api_key = match sign_in_and_save_key(START_REGION) {
        Ok(api_key) => api_key,
        Err(error) => {
            crate::emit_error(false, &error);
            return ExitCode::from(1);
        }
    };
    if let Err(error) = wait_for_api_key_active(START_REGION, &api_key) {
        crate::emit_error(false, &error);
        return ExitCode::from(1);
    }

    maybe_create_project(workspace, args, Some(START_REGION))
}

fn maybe_create_project(
    workspace: &ProjectWorkspace,
    args: StartArgs,
    region: Option<&str>,
) -> ExitCode {
    match prompt_confirm_default(
        "Would you like to create a new project in Agent Studio now?",
        true,
    ) {
        Ok(true) => cmd_project_create(
            workspace,
            ProjectCreateArgs {
                base_path: args.base_path,
                region: region.map(ToString::to_string),
                account_id: None,
                project_name: None,
                project_id: None,
                greeting: "Hello, how can I help you?".to_string(),
                voice_id: None,
                json: false,
                debug: args.debug,
                verbose: args.verbose,
            },
        ),
        Ok(false) => {
            console::info("You can create a new project later by running 'poly project create'");
            ExitCode::SUCCESS
        }
        Err(error) => {
            crate::emit_error(false, &error);
            ExitCode::from(1)
        }
    }
}
