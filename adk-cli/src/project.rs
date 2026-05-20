use crate::{
    HttpPlatformClient, InitArgs, ProjectArgs, ProjectCommands, ProjectCreateArgs,
    ProjectWorkspace, account_choice, console, emit_error, init::cmd_init, init::INIT_REGIONS,
    prompt_select, prompt_text,
};
use std::process::ExitCode;

pub(crate) fn cmd_project(workspace: &ProjectWorkspace, args: ProjectArgs) -> ExitCode {
    match args.command {
        ProjectCommands::Create(args) => cmd_project_create(workspace, args),
    }
}

pub(crate) fn project_verbose(args: &ProjectArgs) -> bool {
    match &args.command {
        ProjectCommands::Create(args) => args.verbose,
    }
}

pub(crate) fn project_debug(args: &ProjectArgs) -> bool {
    match &args.command {
        ProjectCommands::Create(args) => args.debug,
    }
}

struct ProjectCreateSelection {
    region: String,
    account_id: String,
    project_name: String,
    project_id: Option<String>,
}

fn cmd_project_create(workspace: &ProjectWorkspace, args: ProjectCreateArgs) -> ExitCode {
    let selection = match resolve_project_create_selection(
        args.region.clone(),
        args.account_id.clone(),
        args.project_name.clone(),
        args.project_id.clone(),
        args.json,
    ) {
        Ok(Some(selection)) => selection,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };

    if !args.json {
        console::info(format!(
            "Creating project {} under account {}...",
            selection.project_name, selection.account_id
        ));
    }
    let created = match HttpPlatformClient::create_project(
        &selection.region,
        &selection.account_id,
        &selection.project_name,
        selection.project_id.as_deref(),
        &args.greeting,
        args.voice_id.as_deref(),
    ) {
        Ok(project) => project,
        Err(error) => {
            emit_error(args.json, &format!("Failed to create project: {error}"));
            return ExitCode::from(1);
        }
    };
    if created.id.is_empty() {
        emit_error(args.json, "No project ID returned by API.");
        return ExitCode::from(1);
    }
    if !args.json {
        console::success(format!(
            "Created project {} ({})",
            selection.project_name, created.id
        ));
    }

    cmd_init(
        workspace,
        InitArgs {
            base_path: args.base_path,
            region: Some(selection.region),
            account_id: Some(selection.account_id),
            project_id: Some(created.id),
            format: false,
            from_projection: None,
            output_json_projection: false,
            json: args.json,
            debug: args.debug,
            verbose: args.verbose,
        },
    )
}

fn resolve_project_create_selection(
    region: Option<String>,
    account_id: Option<String>,
    project_name: Option<String>,
    project_id: Option<String>,
    json_mode: bool,
) -> Result<Option<ProjectCreateSelection>, String> {
    if json_mode {
        return match (region, account_id, project_name) {
            (Some(region), Some(account_id), Some(project_name)) => {
                Ok(Some(ProjectCreateSelection {
                    region,
                    account_id,
                    project_name,
                    project_id,
                }))
            }
            _ => Err(
                "create project with --json requires --region, --account_id, and --name."
                    .to_string(),
            ),
        };
    }

    let region = match region {
        Some(region) => region,
        None => {
            console::info("Fetching available regions...");
            let regions = HttpPlatformClient::accessible_regions(INIT_REGIONS);
            if regions.is_empty() {
                return Err("No accessible regions found for your API key.".to_string());
            }
            if regions.len() == 1 {
                let region = regions[0].clone();
                console::info(format!("Auto-selected region {region}."));
                region
            } else {
                let choices = regions
                    .iter()
                    .map(|region| (region.clone(), region.clone()))
                    .collect::<Vec<_>>();
                let Some(region) = prompt_select("Select Region", &choices)? else {
                    console::warning("No region selected. Exiting.");
                    return Ok(None);
                };
                region
            }
        }
    };

    let account_id = match account_id {
        Some(account_id) => account_id,
        None => {
            let accounts =
                HttpPlatformClient::list_accounts(&region).map_err(|error| error.to_string())?;
            if accounts.is_empty() {
                return Err("No accounts found in the selected region.".to_string());
            }
            if accounts.len() == 1 {
                let account = &accounts[0];
                console::info(format!("Auto-selected account {}.", account.name));
                account.id.clone()
            } else {
                let choices = accounts.iter().map(account_choice).collect::<Vec<_>>();
                let Some(account_id) = prompt_select("Select Account", &choices)? else {
                    console::warning("No account selected. Exiting.");
                    return Ok(None);
                };
                account_id
            }
        }
    };

    let project_name = match project_name {
        Some(project_name) if !project_name.trim().is_empty() => project_name.trim().to_string(),
        _ => {
            let Some(project_name) = prompt_text("Enter project name:", None)? else {
                console::warning("No project name provided. Exiting.");
                return Ok(None);
            };
            if project_name.trim().is_empty() {
                console::warning("No project name provided. Exiting.");
                return Ok(None);
            }
            project_name.trim().to_string()
        }
    };

    let project_id = match project_id {
        Some(project_id) => Some(project_id),
        None => {
            let default_id = default_project_id_for_name(&project_name);
            let Some(project_id) = prompt_text(
                "Enter project ID (leave empty to let the platform generate one):",
                Some(&default_id),
            )?
            else {
                return Ok(None);
            };
            let project_id = project_id.trim();
            if project_id.is_empty() {
                None
            } else if project_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
            {
                Some(project_id.to_string())
            } else {
                return Err(
                    "Project ID can only contain alphanumeric characters and dashes.".to_string(),
                );
            }
        }
    };

    Ok(Some(ProjectCreateSelection {
        region,
        account_id,
        project_name,
        project_id,
    }))
}

fn default_project_id_for_name(project_name: &str) -> String {
    project_name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
