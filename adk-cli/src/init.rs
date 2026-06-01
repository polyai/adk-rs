use crate::{
    AdkService, HttpPlatformClient, InitArgs, ProjectWorkspace, account_choice, console,
    credentials, emit_error, parse_optional_json_arg, prompt_select, pull_projection_into_path,
    resolve_base_path,
};
use adk_api_client::{AccountSummary, ProjectSummary};
use serde_json::json;
use std::process::ExitCode;

pub(crate) const INIT_REGIONS: &[&str] = &["us-1", "euw-1", "uk-1", "studio", "staging", "dev"];

pub(crate) fn cmd_init(workspace: &ProjectWorkspace, args: InitArgs) -> ExitCode {
    let output_json = args.json || args.output_json_projection;
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(output_json, &error);
            return ExitCode::from(1);
        }
    };
    let selection =
        match resolve_init_selection(args.region, args.account_id, args.project_id, args.json) {
            Ok(Some(selection)) => selection,
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                emit_error(args.json, &error);
                return ExitCode::from(1);
            }
        };
    let base = resolve_base_path(&args.base_path);
    match workspace.init_project_with_name(
        &base,
        selection.region.clone(),
        selection.account_id.clone(),
        selection.project_id.clone(),
        selection.project_name.clone(),
    ) {
        Ok(project) => {
            let root_path = base.join(&project.account_id).join(&project.project_id);
            let mut output_projection = projection_json.clone();
            if let Some(projection) = &projection_json
                && let Err(error) =
                    pull_projection_into_path(&root_path, projection, true, args.format)
            {
                emit_error(args.json, &error);
                return ExitCode::from(1);
            } else if projection_json.is_none()
                && let Ok(api_key) = credentials::api_key_for_region(&selection.region)
                && let Ok(http_client) = HttpPlatformClient::new_with_api_key(
                    &selection.region,
                    &selection.account_id,
                    &selection.project_id,
                    Some("main"),
                    api_key,
                )
            {
                let remote_service = AdkService::new(http_client);
                if args.output_json_projection {
                    match remote_service.pull_projection_json() {
                        Ok(projection) => output_projection = Some(projection),
                        Err(error) => {
                            emit_error(args.json, &error.to_string());
                            return ExitCode::from(1);
                        }
                    }
                }
                if let Err(error) =
                    remote_service.pull_with_format(root_path.as_path(), true, args.format)
                {
                    emit_error(args.json, &error.to_string());
                    return ExitCode::from(1);
                }
            }
            if output_json {
                let mut payload = json!({"success": true, "root_path": root_path});
                if args.output_json_projection {
                    payload["projection"] = output_projection.unwrap_or(serde_json::Value::Null);
                }
                println!("{}", payload);
            } else {
                console::success(format!("Project initialized at {}", root_path.display()));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

struct InitSelection {
    region: String,
    account_id: String,
    project_id: String,
    project_name: Option<String>,
}

fn resolve_init_selection(
    region: Option<String>,
    account_id: Option<String>,
    project_id: Option<String>,
    json_mode: bool,
) -> Result<Option<InitSelection>, String> {
    if json_mode {
        return match (region, account_id, project_id) {
            (Some(region), Some(account_id), Some(project_id)) => Ok(Some(InitSelection {
                region,
                account_id,
                project_id,
                project_name: None,
            })),
            _ => Err("init with --json requires --region, --account_id, and --project_id."
                .to_string()),
        };
    }

    console::info("Initialising project...");
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

    let (account_id, _account_name) = match account_id {
        Some(account_id) => (account_id, None),
        None => {
            let accounts = list_accounts_for_region(&region)?;
            if accounts.is_empty() {
                return Err("No accounts found in the selected region.".to_string());
            }
            if accounts.len() == 1 {
                let account = &accounts[0];
                console::info(format!("Auto-selected account {}.", account.name));
                (account.id.clone(), Some(account.name.clone()))
            } else {
                let choices = accounts.iter().map(account_choice).collect::<Vec<_>>();
                let Some(account_id) = prompt_select("Select Account", &choices)? else {
                    console::warning("No account selected. Exiting.");
                    return Ok(None);
                };
                let account_name = accounts
                    .iter()
                    .find(|account| account.id == account_id)
                    .map(|account| account.name.clone());
                (account_id, account_name)
            }
        }
    };

    let projects = list_projects_for_account(&region, &account_id)?;
    let (project_id, project_name) = match project_id {
        Some(project_id) => {
            let project_name = projects
                .iter()
                .find(|project| project.id == project_id)
                .map(|project| project.name.clone());
            (project_id, project_name)
        }
        None => {
            if projects.is_empty() {
                return Err("No projects found in the selected account.".to_string());
            }
            let choices = projects.iter().map(project_choice).collect::<Vec<_>>();
            let Some(project_id) = prompt_select("Select Project", &choices)? else {
                console::warning("No project selected. Exiting.");
                return Ok(None);
            };
            let project_name = projects
                .iter()
                .find(|project| project.id == project_id)
                .map(|project| project.name.clone());
            (project_id, project_name)
        }
    };

    console::info(format!("Initializing project {account_id}/{project_id}..."));
    Ok(Some(InitSelection {
        region,
        account_id,
        project_id,
        project_name,
    }))
}

pub(crate) fn project_choice(project: &ProjectSummary) -> (String, String) {
    (
        project.id.clone(),
        format!("{} ({})", project.name, project.id),
    )
}

pub(crate) fn accessible_regions_with_credentials(regions: &[&str]) -> Vec<String> {
    regions
        .iter()
        .filter_map(|region| {
            let api_key = credentials::api_key_for_region(region).ok()?;
            HttpPlatformClient::list_accounts_with_api_key(region, &api_key)
                .ok()
                .filter(|accounts| !accounts.is_empty())
                .map(|_| (*region).to_string())
        })
        .collect()
}

pub(crate) fn list_accounts_for_region(region: &str) -> Result<Vec<AccountSummary>, String> {
    let api_key = credentials::api_key_for_region(region)?;
    HttpPlatformClient::list_accounts_with_api_key(region, &api_key)
        .map_err(|error| error.to_string())
}

pub(crate) fn list_projects_for_account(
    region: &str,
    account_id: &str,
) -> Result<Vec<ProjectSummary>, String> {
    let api_key = credentials::api_key_for_region(region)?;
    HttpPlatformClient::list_projects_with_api_key(region, account_id, &api_key)
        .map_err(|error| error.to_string())
}
