use crate::{
    HttpPlatformClient, InitArgs, ProjectArgs, ProjectCommands, ProjectCreateArgs,
    ProjectWorkspace, account_choice, console, credentials, emit_error, prompt_select, prompt_text,
};
use super::init::{
    INIT_REGIONS, accessible_regions_with_credentials, cmd_init, list_accounts_for_region,
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
    cmd_project_create_with_backend(args, &RealProjectCreateBackend { workspace })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectCreateRequest {
    region: String,
    account_id: String,
    project_name: String,
    project_id: Option<String>,
    greeting: String,
    voice_id: Option<String>,
}

trait ProjectCreateBackend {
    fn api_key_for_region(&self, region: &str) -> Result<String, String>;
    fn create_project(
        &self,
        request: &ProjectCreateRequest,
        api_key: &str,
    ) -> Result<adk_api_client::ProjectSummary, String>;
    fn init_project(&self, args: InitArgs) -> ExitCode;
}

struct RealProjectCreateBackend<'a> {
    workspace: &'a ProjectWorkspace,
}

impl ProjectCreateBackend for RealProjectCreateBackend<'_> {
    fn api_key_for_region(&self, region: &str) -> Result<String, String> {
        credentials::api_key_for_region(region)
    }

    fn create_project(
        &self,
        request: &ProjectCreateRequest,
        api_key: &str,
    ) -> Result<adk_api_client::ProjectSummary, String> {
        HttpPlatformClient::create_project_with_api_key(
            &request.region,
            &request.account_id,
            &request.project_name,
            request.project_id.as_deref(),
            &request.greeting,
            request.voice_id.as_deref(),
            api_key,
        )
        .map_err(|error| error.to_string())
    }

    fn init_project(&self, args: InitArgs) -> ExitCode {
        cmd_init(self.workspace, args)
    }
}

fn cmd_project_create_with_backend(
    args: ProjectCreateArgs,
    backend: &impl ProjectCreateBackend,
) -> ExitCode {
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
    let api_key = match backend.api_key_for_region(&selection.region) {
        Ok(api_key) => api_key,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };
    let request = ProjectCreateRequest {
        region: selection.region,
        account_id: selection.account_id,
        project_name: selection.project_name,
        project_id: selection.project_id,
        greeting: args.greeting.clone(),
        voice_id: args.voice_id.clone(),
    };
    let created = match backend.create_project(&request, &api_key) {
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
            request.project_name, created.id
        ));
    }

    backend.init_project(InitArgs {
        base_path: args.base_path,
        region: Some(request.region),
        account_id: Some(request.account_id),
        project_id: Some(created.id),
        format: false,
        from_projection: None,
        output_json_projection: false,
        json: args.json,
        debug: args.debug,
        verbose: args.verbose,
    })
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
    let project_id = resolve_project_create_project_id(&region, project_id)?;

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
    region: &str,
    project_id: Option<String>,
) -> Result<Option<String>, String> {
    match project_id {
        Some(project_id) => Ok(Some(project_id)),
        None if region == "studio" => Ok(None),
        None => {
            let Some(project_id) = prompt_text(
                "Enter project ID (leave empty to let the platform generate one):",
                None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use adk_api_client::ProjectSummary;
    use std::cell::RefCell;

    struct FakeProjectCreateBackend {
        api_key: Result<String, String>,
        created: Result<ProjectSummary, String>,
        init_code: ExitCode,
        requests: RefCell<Vec<(ProjectCreateRequest, String)>>,
        init_args: RefCell<Vec<InitArgs>>,
    }

    impl FakeProjectCreateBackend {
        fn success() -> Self {
            Self {
                api_key: Ok("test-key".to_string()),
                created: Ok(ProjectSummary {
                    id: "created-project".to_string(),
                    name: "Created Project".to_string(),
                }),
                init_code: ExitCode::SUCCESS,
                requests: RefCell::new(Vec::new()),
                init_args: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProjectCreateBackend for FakeProjectCreateBackend {
        fn api_key_for_region(&self, _region: &str) -> Result<String, String> {
            self.api_key.clone()
        }

        fn create_project(
            &self,
            request: &ProjectCreateRequest,
            api_key: &str,
        ) -> Result<ProjectSummary, String> {
            self.requests
                .borrow_mut()
                .push((request.clone(), api_key.to_string()));
            self.created.clone()
        }

        fn init_project(&self, args: InitArgs) -> ExitCode {
            self.init_args.borrow_mut().push(args);
            self.init_code
        }
    }

    fn project_create_args() -> ProjectCreateArgs {
        ProjectCreateArgs {
            base_path: "/tmp/adk-rs-project-create-test".to_string(),
            region: Some("studio".to_string()),
            account_id: Some("acct-1".to_string()),
            project_name: Some(" Test Project ".to_string()),
            project_id: Some("project-id".to_string()),
            greeting: "Hello from tests".to_string(),
            voice_id: Some("VOICE-test".to_string()),
            json: true,
            debug: true,
            verbose: true,
        }
    }

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
    fn studio_project_create_selection_leaves_project_id_for_platform_generation() {
        let selection = resolve_project_create_selection(
            Some("studio".to_string()),
            Some("acct-1".to_string()),
            Some(" Test Project ".to_string()),
            None,
            false,
        )
        .expect("selection")
        .expect("selection");

        assert_eq!(
            selection,
            ProjectCreateSelection {
                region: "studio".to_string(),
                account_id: "acct-1".to_string(),
                project_name: "Test Project".to_string(),
                project_id: None,
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
    fn project_create_command_creates_remote_project_then_initializes_it() {
        let backend = FakeProjectCreateBackend::success();

        let code = cmd_project_create_with_backend(project_create_args(), &backend);

        assert_eq!(code, ExitCode::SUCCESS);
        assert_eq!(
            backend.requests.borrow().as_slice(),
            &[(
                ProjectCreateRequest {
                    region: "studio".to_string(),
                    account_id: "acct-1".to_string(),
                    project_name: " Test Project ".to_string(),
                    project_id: Some("project-id".to_string()),
                    greeting: "Hello from tests".to_string(),
                    voice_id: Some("VOICE-test".to_string()),
                },
                "test-key".to_string(),
            )]
        );
        let init_args = backend.init_args.borrow();
        assert_eq!(init_args.len(), 1);
        assert_eq!(init_args[0].base_path, "/tmp/adk-rs-project-create-test");
        assert_eq!(init_args[0].region.as_deref(), Some("studio"));
        assert_eq!(init_args[0].account_id.as_deref(), Some("acct-1"));
        assert_eq!(init_args[0].project_id.as_deref(), Some("created-project"));
        assert!(init_args[0].json);
        assert!(init_args[0].debug);
        assert!(init_args[0].verbose);
    }

    #[test]
    fn project_create_command_stops_before_network_when_api_key_is_missing() {
        let backend = FakeProjectCreateBackend {
            api_key: Err("missing key".to_string()),
            ..FakeProjectCreateBackend::success()
        };

        let code = cmd_project_create_with_backend(project_create_args(), &backend);

        assert_eq!(code, ExitCode::from(1));
        assert!(backend.requests.borrow().is_empty());
        assert!(backend.init_args.borrow().is_empty());
    }

    #[test]
    fn project_create_command_stops_when_platform_returns_no_project_id() {
        let backend = FakeProjectCreateBackend {
            created: Ok(ProjectSummary {
                id: String::new(),
                name: "Created Project".to_string(),
            }),
            ..FakeProjectCreateBackend::success()
        };

        let code = cmd_project_create_with_backend(project_create_args(), &backend);

        assert_eq!(code, ExitCode::from(1));
        assert_eq!(backend.requests.borrow().len(), 1);
        assert!(backend.init_args.borrow().is_empty());
    }
}
