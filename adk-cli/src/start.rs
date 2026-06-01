use crate::{
    ProjectCreateArgs, ProjectWorkspace, StartArgs, console, credentials,
    login::{print_welcome_message, sign_in_and_save_key},
    project::cmd_project_create,
    prompt_confirm, prompt_confirm_default,
};
use std::io::{self, Write};
use std::process::ExitCode;

const START_REGION: &str = "studio";

pub(crate) fn cmd_start(workspace: &ProjectWorkspace, args: StartArgs) -> ExitCode {
    print_welcome_message();
    console::plain("This command signs you in, saves an ADK API key, and can create a new project.");
    if let Err(error) = wait_for_enter("Press Enter to continue...") {
        crate::emit_error(false, &error);
        return ExitCode::from(1);
    }

    if credentials::any_credentials_exist() {
        console::warning(format!(
            "An API key already exists in {} or your environment.",
            credentials::CREDENTIALS_FILE_DISPLAY
        ));
        match prompt_confirm_default("Do you want to continue with the existing key?", true) {
            Ok(true) => return maybe_create_project(workspace, args, None),
            Ok(false) => {}
            Err(error) => {
                crate::emit_error(false, &error);
                return ExitCode::from(1);
            }
        }
    }

    if let Err(error) = sign_in_and_save_key(START_REGION) {
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
    match prompt_confirm("Do you want to create a new project?") {
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
            console::success("Setup complete.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            crate::emit_error(false, &error);
            ExitCode::from(1)
        }
    }
}

fn wait_for_enter(message: &str) -> Result<(), String> {
    console::prompt(format!("{message} "))
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read input: {error}"))?;
    Ok(())
}
