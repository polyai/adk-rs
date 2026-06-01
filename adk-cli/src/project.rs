use crate::{
    HttpPlatformClient, InitArgs, ProjectArgs, ProjectCommands, ProjectCreateArgs,
    ProjectWorkspace, account_choice, console, credentials, emit_error, init::INIT_REGIONS,
    init::accessible_regions_with_credentials, init::cmd_init, init::list_accounts_for_region,
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

#[derive(Debug, PartialEq, Eq)]
struct ProjectCreateSelection {
    region: String,
    account_id: String,
    project_name: String,
    project_id: Option<String>,
}

pub(crate) fn cmd_project_create(workspace: &ProjectWorkspace, args: ProjectCreateArgs) -> ExitCode {
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
    let api_key = match credentials::api_key_for_region(&selection.region) {
        Ok(api_key) => api_key,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };
    let created = match HttpPlatformClient::create_project_with_api_key(
        &selection.region,
        &selection.account_id,
        &selection.project_name,
        selection.project_id.as_deref(),
        &args.greeting,
        args.voice_id.as_deref(),
        &api_key,
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
        return resolve_project_create_json_selection(region, account_id, project_name, project_id)
            .map(Some);
    }

    let Some(region) = resolve_project_create_region(region)? else {
        return Ok(None);
    };
    let Some(account_id) = resolve_project_create_account_id(&region, account_id)? else {
        return Ok(None);
    };
    let Some(project_name) = resolve_project_create_name(project_name)? else {
        return Ok(None);
    };
    let project_id = resolve_project_create_project_id(project_id, &project_name)?;

    Ok(Some(ProjectCreateSelection {
        region,
        account_id,
        project_name,
        project_id,
    }))
}

fn resolve_project_create_json_selection(
    region: Option<String>,
    account_id: Option<String>,
    project_name: Option<String>,
    project_id: Option<String>,
) -> Result<ProjectCreateSelection, String> {
    match (region, account_id, project_name) {
        (Some(region), Some(account_id), Some(project_name)) => Ok(ProjectCreateSelection {
            region,
            account_id,
            project_name,
            project_id,
        }),
        _ => Err("create project with --json requires --region, --account_id, and --name."
            .to_string()),
    }
}

fn resolve_project_create_region(region: Option<String>) -> Result<Option<String>, String> {
    let region = match region {
        Some(region) => region,
        None => {
            console::info("Fetching available regions...");
            let regions = accessible_regions_with_credentials(INIT_REGIONS);
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
    Ok(Some(region))
}

fn resolve_project_create_account_id(
    region: &str,
    account_id: Option<String>,
) -> Result<Option<String>, String> {
    let account_id = match account_id {
        Some(account_id) => account_id,
        None => {
            let accounts = list_accounts_for_region(region)?;
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
    Ok(Some(account_id))
}

fn resolve_project_create_name(project_name: Option<String>) -> Result<Option<String>, String> {
    match project_name {
        Some(project_name) if !project_name.trim().is_empty() => {
            Ok(Some(project_name.trim().to_string()))
        }
        _ => {
            let Some(project_name) = prompt_text("Enter project name:", None)? else {
                console::warning("No project name provided. Exiting.");
                return Ok(None);
            };
            let project_name = project_name.trim();
            if project_name.is_empty() {
                console::warning("No project name provided. Exiting.");
                Ok(None)
            } else {
                Ok(Some(project_name.to_string()))
            }
        }
    }
}

fn resolve_project_create_project_id(
    project_id: Option<String>,
    project_name: &str,
) -> Result<Option<String>, String> {
    match project_id {
        Some(project_id) => Ok(Some(project_id)),
        None => {
            let default_id = default_project_id_for_name(project_name);
            let Some(project_id) = prompt_text(
                "Enter project ID (leave empty to let the platform generate one):",
                Some(&default_id),
            )?
            else {
                return Ok(None);
            };
            project_id_from_prompt_input(&project_id)
        }
    }
}

fn project_id_from_prompt_input(project_id: &str) -> Result<Option<String>, String> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        Ok(None)
    } else if project_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        Ok(Some(project_id.to_string()))
    } else {
        Err("Project ID can only contain alphanumeric characters and dashes.".to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_project_create_selection_requires_region_account_and_name() {
        let error = resolve_project_create_selection(
            Some("us-1".to_string()),
            None,
            Some("Test Project".to_string()),
            None,
            true,
        )
        .expect_err("missing account should fail in json mode");

        assert_eq!(
            error,
            "create project with --json requires --region, --account_id, and --name."
        );
    }

    #[test]
    fn json_project_create_selection_preserves_supplied_values() {
        let selection = resolve_project_create_selection(
            Some("us-1".to_string()),
            Some("acct-1".to_string()),
            Some(" Test Project ".to_string()),
            Some("project-id".to_string()),
            true,
        )
        .expect("json selection")
        .expect("selection");

        assert_eq!(
            selection,
            ProjectCreateSelection {
                region: "us-1".to_string(),
                account_id: "acct-1".to_string(),
                project_name: " Test Project ".to_string(),
                project_id: Some("project-id".to_string()),
            }
        );
    }

    #[test]
    fn non_json_project_create_selection_uses_supplied_values_without_prompting() {
        let selection = resolve_project_create_selection(
            Some("us-1".to_string()),
            Some("acct-1".to_string()),
            Some(" Test Project ".to_string()),
            Some("Project_ID_From_Arg".to_string()),
            false,
        )
        .expect("selection")
        .expect("selection");

        assert_eq!(
            selection,
            ProjectCreateSelection {
                region: "us-1".to_string(),
                account_id: "acct-1".to_string(),
                project_name: "Test Project".to_string(),
                project_id: Some("Project_ID_From_Arg".to_string()),
            }
        );
    }

    #[test]
    fn project_id_prompt_input_accepts_blank_or_slug_values() {
        assert_eq!(project_id_from_prompt_input("   ").unwrap(), None);
        assert_eq!(
            project_id_from_prompt_input("  my-project-1  ").unwrap(),
            Some("my-project-1".to_string())
        );
    }

    #[test]
    fn project_id_prompt_input_rejects_non_slug_values() {
        let error = project_id_from_prompt_input("my_project")
            .expect_err("underscores are not accepted by the existing prompt path");

        assert_eq!(
            error,
            "Project ID can only contain alphanumeric characters and dashes."
        );
    }

    #[test]
    fn default_project_id_for_name_matches_existing_slug_rules() {
        assert_eq!(
            default_project_id_for_name("  My Fancy Project!  "),
            "my-fancy-project"
        );
    }
}
