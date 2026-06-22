use crate::{
    HttpPlatformClient, InitArgs, ProjectArgs, ProjectCommands, ProjectCreateArgs,
    ProjectDeleteArgs, ProjectDuplicateArgs, ProjectListArgs, ProjectWorkspace, account_choice,
    console, credentials, emit_error, print_payload, prompt_confirm, prompt_select, prompt_text,
};
use super::init::{
    INIT_REGIONS, accessible_regions_with_credentials, cmd_init, list_accounts_for_region,
};
use std::process::ExitCode;

pub(crate) fn cmd_project(workspace: &ProjectWorkspace, args: ProjectArgs) -> ExitCode {
    match args.command {
        ProjectCommands::Create(args) => cmd_project_create(workspace, args),
        ProjectCommands::List(args) => cmd_project_list(args),
        ProjectCommands::Delete(args) => cmd_project_delete(args),
        ProjectCommands::Duplicate(args) => cmd_project_duplicate(args),
    }
}

pub(crate) fn project_verbose(args: &ProjectArgs) -> bool {
    match &args.command {
        ProjectCommands::Create(args) => args.verbose,
        ProjectCommands::List(args) => args.verbose,
        ProjectCommands::Delete(args) => args.verbose,
        ProjectCommands::Duplicate(args) => args.verbose,
    }
}

pub(crate) fn project_debug(args: &ProjectArgs) -> bool {
    match &args.command {
        ProjectCommands::Create(args) => args.debug,
        ProjectCommands::List(args) => args.debug,
        ProjectCommands::Delete(args) => args.debug,
        ProjectCommands::Duplicate(args) => args.debug,
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

// ── project list ─────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
struct ProjectListSelection {
    region: String,
    account_id: String,
}

fn cmd_project_list(args: ProjectListArgs) -> ExitCode {
    cmd_project_list_with_backend(args, &RealProjectBackend)
}

fn cmd_project_list_with_backend(
    args: ProjectListArgs,
    backend: &impl ProjectBackend,
) -> ExitCode {
    let selection = match resolve_region_account_selection(
        args.region.clone(),
        args.account_id.clone(),
        args.json,
    ) {
        Ok(Some(s)) => s,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };

    let api_key = match backend.api_key_for_region(&selection.region) {
        Ok(key) => key,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };

    if !args.json {
        console::info("Fetching agents...");
    }
    let agents = match backend.list_agents(&selection.region, &selection.account_id, &api_key) {
        Ok(agents) => agents,
        Err(error) => {
            emit_error(args.json, &format!("Failed to list agents: {error}"));
            return ExitCode::from(1);
        }
    };

    if args.json {
        let payload = serde_json::json!({
            "success": true,
            "agents": agents.iter().map(|a| serde_json::json!({
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "updated_at": a.updated_at,
                "branch_count": a.branch_count,
            })).collect::<Vec<_>>(),
        });
        return print_payload(true, payload);
    } else if agents.is_empty() {
        console::warning("No agents found in this account.");
    } else {
        print_agents_table(&agents);
    }
    ExitCode::SUCCESS
}

fn print_agents_table(agents: &[adk_api_client::AgentSummary]) {
    console::plain(format!(
        "  [label]{:<30}[/label] [label]{:<30}[/label] [label]{:<28}[/label] [label]{}[/label]",
        "Agent ID", "Name", "Updated", "Branches"
    ));
    for agent in agents {
        let updated = agent.updated_at.as_deref().unwrap_or("—");
        let branches = agent
            .branch_count
            .map(|c| c.to_string())
            .unwrap_or_else(|| "0".to_string());
        console::plain(format!(
            "  {:<30} {:<30} {:<28} {}",
            agent.agent_id, agent.agent_name, updated, branches
        ));
    }
}

// ── project delete ───────────────────────────────────────────────────

fn cmd_project_delete(args: ProjectDeleteArgs) -> ExitCode {
    cmd_project_delete_with_backend(args, &RealProjectBackend)
}

fn cmd_project_delete_with_backend(
    args: ProjectDeleteArgs,
    backend: &impl ProjectBackend,
) -> ExitCode {
    let selection = match resolve_region_account_project_selection(
        args.region.clone(),
        args.account_id.clone(),
        args.project_id.clone(),
        args.json,
        backend,
    ) {
        Ok(Some(s)) => s,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };

    let display_name = selection.display_name();

    if !args.force && !args.json {
        console::warning(format!(
            "You are about to delete project '{display_name}'. This action cannot be undone."
        ));
        let confirmed = match prompt_confirm("Are you sure you want to continue?") {
            Ok(c) => c,
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        };
        if !confirmed {
            console::warning("Aborted.");
            return ExitCode::SUCCESS;
        }
    }

    if !args.json {
        console::info(format!("Deleting project {display_name}..."));
    }
    if let Err(error) =
        backend.delete_project(&selection.region, &selection.project_id, &selection.api_key)
    {
        emit_error(args.json, &format!("Failed to delete project: {error}"));
        return ExitCode::from(1);
    }

    if args.json {
        let payload = serde_json::json!({
            "success": true,
            "agent_id": selection.project_id,
        });
        return print_payload(true, payload);
    } else {
        console::success(format!("Deleted project {display_name}."));
    }
    ExitCode::SUCCESS
}

// ── project duplicate ────────────────────────────────────────────────

fn cmd_project_duplicate(args: ProjectDuplicateArgs) -> ExitCode {
    cmd_project_duplicate_with_backend(args, &RealProjectBackend)
}

fn cmd_project_duplicate_with_backend(
    args: ProjectDuplicateArgs,
    backend: &impl ProjectBackend,
) -> ExitCode {
    if args.json && args.new_name.is_none() {
        emit_error(true, "project duplicate with --json requires --name.");
        return ExitCode::from(1);
    }

    let selection = match resolve_region_account_project_selection(
        args.region.clone(),
        args.account_id.clone(),
        args.project_id.clone(),
        args.json,
        backend,
    ) {
        Ok(Some(s)) => s,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(args.json, &error);
            return ExitCode::from(1);
        }
    };

    let display_name = selection.display_name();

    let new_name = match resolve_duplicate_name(args.new_name, &selection.project_name) {
        Ok(Some(name)) => name,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(false, &error);
            return ExitCode::from(1);
        }
    };

    let new_project_id = match args.new_project_id {
        Some(id) => {
            let id = id.trim().to_string();
            if id.is_empty() { None } else { Some(id) }
        }
        None if args.json => None,
        None => match prompt_text(
            "Enter project ID for the duplicate (leave empty to auto-generate):",
            None,
        ) {
            Ok(Some(id)) => match project_id_from_prompt_input(&id) {
                Ok(v) => v,
                Err(error) => {
                    emit_error(false, &error);
                    return ExitCode::from(1);
                }
            },
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        },
    };

    if !args.json {
        console::info(format!("Duplicating project {display_name}..."));
    }
    let result = match backend.duplicate_project(
        &selection.region,
        &selection.project_id,
        &new_name,
        new_project_id.as_deref(),
        &selection.api_key,
    ) {
        Ok(r) => r,
        Err(error) => {
            emit_error(args.json, &format!("Failed to duplicate project: {error}"));
            return ExitCode::from(1);
        }
    };

    if args.json {
        let payload = serde_json::json!({
            "success": true,
            "agent_id": result.id,
            "agent_name": result.name,
        });
        return print_payload(true, payload);
    } else {
        console::success(format!(
            "Duplicated {display_name} → {} ({})",
            result.name, result.id
        ));
    }
    ExitCode::SUCCESS
}

fn resolve_duplicate_name(
    new_name: Option<String>,
    source_name: &str,
) -> Result<Option<String>, String> {
    if let Some(name) = new_name {
        let name = name.trim();
        if !name.is_empty() {
            return Ok(Some(name.to_string()));
        }
    }
    let default_name = if source_name.is_empty() {
        None
    } else {
        Some(format!("{source_name} (copy)"))
    };
    let prompt = match &default_name {
        Some(name) => format!("Enter name for the duplicated project ({name}):"),
        None => "Enter name for the duplicated project:".to_string(),
    };
    let Some(input) = prompt_text(&prompt, None)? else {
        console::warning("No project name provided. Exiting.");
        return Ok(None);
    };
    let input = input.trim();
    if input.is_empty() {
        match default_name {
            Some(name) => Ok(Some(name)),
            None => {
                console::warning("No project name provided. Exiting.");
                Ok(None)
            }
        }
    } else {
        Ok(Some(input.to_string()))
    }
}

// ── shared resolution helpers ────────────────────────────────────────

fn resolve_region_account_selection(
    region: Option<String>,
    account_id: Option<String>,
    json_mode: bool,
) -> Result<Option<ProjectListSelection>, String> {
    if json_mode {
        return match (region, account_id) {
            (Some(region), Some(account_id)) => {
                Ok(Some(ProjectListSelection { region, account_id }))
            }
            _ => Err("--json requires --region and --account_id.".to_string()),
        };
    }
    let Some(region) = resolve_project_create_region(region)? else {
        return Ok(None);
    };
    let Some(account_id) = resolve_project_create_account_id(&region, account_id)? else {
        return Ok(None);
    };
    Ok(Some(ProjectListSelection { region, account_id }))
}

#[derive(Debug)]
struct ProjectSelection {
    region: String,
    #[allow(dead_code)]
    account_id: String,
    project_id: String,
    project_name: String,
    api_key: String,
}

impl ProjectSelection {
    fn display_name(&self) -> String {
        if self.project_name.is_empty() {
            self.project_id.clone()
        } else {
            format!("{} ({})", self.project_name, self.project_id)
        }
    }
}

fn resolve_region_account_project_selection(
    region: Option<String>,
    account_id: Option<String>,
    project_id: Option<String>,
    json_mode: bool,
    backend: &impl ProjectBackend,
) -> Result<Option<ProjectSelection>, String> {
    if json_mode {
        return match (region, account_id, project_id) {
            (Some(region), Some(account_id), Some(project_id)) => {
                let api_key = backend.api_key_for_region(&region)?;
                Ok(Some(ProjectSelection {
                    region,
                    account_id,
                    project_id,
                    project_name: String::new(),
                    api_key,
                }))
            }
            _ => Err("--json requires --region, --account_id, and --project_id.".to_string()),
        };
    }

    let Some(region) = resolve_project_create_region(region)? else {
        return Ok(None);
    };
    let Some(account_id) = resolve_project_create_account_id(&region, account_id)? else {
        return Ok(None);
    };

    let api_key = backend.api_key_for_region(&region)?;
    let agents = backend
        .list_agents(&region, &account_id, &api_key)
        .map_err(|e| format!("Failed to list agents: {e}"))?;

    let (project_id, project_name) = if let Some(pid) = project_id {
        let name = agents
            .iter()
            .find(|a| a.agent_id == pid)
            .map(|a| a.agent_name.clone())
            .unwrap_or_default();
        (pid, name)
    } else {
        if agents.is_empty() {
            return Err("No agents found in the selected account.".to_string());
        }
        let choices: Vec<(String, String)> = agents
            .iter()
            .map(|a| {
                (
                    a.agent_id.clone(),
                    format!("{} ({})", a.agent_name, a.agent_id),
                )
            })
            .collect();
        let Some(selected_id) = prompt_select("Select Agent", &choices)? else {
            console::warning("No agent selected. Exiting.");
            return Ok(None);
        };
        let name = agents
            .iter()
            .find(|a| a.agent_id == selected_id)
            .map(|a| a.agent_name.clone())
            .unwrap_or_default();
        (selected_id, name)
    };

    Ok(Some(ProjectSelection {
        region,
        account_id,
        project_id,
        project_name,
        api_key,
    }))
}

// ── backend trait for testing ────────────────────────────────────────

trait ProjectBackend {
    fn api_key_for_region(&self, region: &str) -> Result<String, String>;
    fn list_agents(
        &self,
        region: &str,
        account_id: &str,
        api_key: &str,
    ) -> Result<Vec<adk_api_client::AgentSummary>, String>;
    fn delete_project(
        &self,
        region: &str,
        agent_id: &str,
        api_key: &str,
    ) -> Result<(), String>;
    fn duplicate_project(
        &self,
        region: &str,
        agent_id: &str,
        new_name: &str,
        new_id: Option<&str>,
        api_key: &str,
    ) -> Result<adk_api_client::ProjectSummary, String>;
}

struct RealProjectBackend;

impl ProjectBackend for RealProjectBackend {
    fn api_key_for_region(&self, region: &str) -> Result<String, String> {
        credentials::api_key_for_region(region)
    }

    fn list_agents(
        &self,
        region: &str,
        account_id: &str,
        api_key: &str,
    ) -> Result<Vec<adk_api_client::AgentSummary>, String> {
        HttpPlatformClient::list_agents_with_api_key(region, account_id, api_key)
            .map_err(|e| e.to_string())
    }

    fn delete_project(
        &self,
        region: &str,
        agent_id: &str,
        api_key: &str,
    ) -> Result<(), String> {
        HttpPlatformClient::delete_project_with_api_key(region, agent_id, api_key)
            .map_err(|e| e.to_string())
    }

    fn duplicate_project(
        &self,
        region: &str,
        agent_id: &str,
        new_name: &str,
        new_id: Option<&str>,
        api_key: &str,
    ) -> Result<adk_api_client::ProjectSummary, String> {
        HttpPlatformClient::duplicate_project_with_api_key(region, agent_id, new_name, new_id, api_key)
            .map_err(|e| e.to_string())
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

    // ── fake ProjectBackend for list/delete/duplicate ────────────────

    type DuplicateCall = (String, String, String, Option<String>);

    struct FakeProjectBackend {
        api_key: Result<String, String>,
        agents: Result<Vec<adk_api_client::AgentSummary>, String>,
        delete_result: Result<(), String>,
        duplicate_result: Result<ProjectSummary, String>,
        delete_calls: RefCell<Vec<(String, String)>>,
        duplicate_calls: RefCell<Vec<DuplicateCall>>,
    }

    impl FakeProjectBackend {
        fn success() -> Self {
            Self {
                api_key: Ok("test-key".to_string()),
                agents: Ok(vec![adk_api_client::AgentSummary {
                    agent_id: "agent-1".to_string(),
                    agent_name: "Agent One".to_string(),
                    updated_at: Some("2026-06-22T10:00:00Z".to_string()),
                    branch_count: Some(2),
                }]),
                delete_result: Ok(()),
                duplicate_result: Ok(ProjectSummary {
                    id: "agent-1-copy".to_string(),
                    name: "Agent One (copy)".to_string(),
                }),
                delete_calls: RefCell::new(Vec::new()),
                duplicate_calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProjectBackend for FakeProjectBackend {
        fn api_key_for_region(&self, _region: &str) -> Result<String, String> {
            self.api_key.clone()
        }

        fn list_agents(
            &self,
            _region: &str,
            _account_id: &str,
            _api_key: &str,
        ) -> Result<Vec<adk_api_client::AgentSummary>, String> {
            self.agents.clone()
        }

        fn delete_project(
            &self,
            region: &str,
            agent_id: &str,
            _api_key: &str,
        ) -> Result<(), String> {
            self.delete_calls
                .borrow_mut()
                .push((region.to_string(), agent_id.to_string()));
            self.delete_result.clone()
        }

        fn duplicate_project(
            &self,
            region: &str,
            agent_id: &str,
            new_name: &str,
            new_id: Option<&str>,
            _api_key: &str,
        ) -> Result<ProjectSummary, String> {
            self.duplicate_calls.borrow_mut().push((
                region.to_string(),
                agent_id.to_string(),
                new_name.to_string(),
                new_id.map(str::to_string),
            ));
            self.duplicate_result.clone()
        }
    }

    // ── list tests ───────────────────────────────────────────────────

    #[test]
    fn project_list_json_requires_region_and_account() {
        let backend = FakeProjectBackend::success();
        let args = ProjectListArgs {
            region: Some("us-1".to_string()),
            account_id: None,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_list_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }

    #[test]
    fn project_list_json_returns_agents() {
        let backend = FakeProjectBackend::success();
        let args = ProjectListArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_list_with_backend(args, &backend);
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn project_list_fails_when_api_key_missing() {
        let backend = FakeProjectBackend {
            api_key: Err("no key".to_string()),
            ..FakeProjectBackend::success()
        };
        let args = ProjectListArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_list_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }

    // ── delete tests ─────────────────────────────────────────────────

    #[test]
    fn project_delete_json_requires_region_account_and_project() {
        let backend = FakeProjectBackend::success();
        let args = ProjectDeleteArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: None,
            force: false,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_delete_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }

    #[test]
    fn project_delete_json_succeeds() {
        let backend = FakeProjectBackend::success();
        let args = ProjectDeleteArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: Some("agent-1".to_string()),
            force: true,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_delete_with_backend(args, &backend);
        assert_eq!(code, ExitCode::SUCCESS);
        let calls = backend.delete_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("us-1".to_string(), "agent-1".to_string()));
    }

    #[test]
    fn project_delete_propagates_api_error() {
        let backend = FakeProjectBackend {
            delete_result: Err("server error".to_string()),
            ..FakeProjectBackend::success()
        };
        let args = ProjectDeleteArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: Some("agent-1".to_string()),
            force: true,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_delete_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }

    // ── duplicate tests ──────────────────────────────────────────────

    #[test]
    fn project_duplicate_json_requires_all_fields() {
        let backend = FakeProjectBackend::success();
        let args = ProjectDuplicateArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: Some("agent-1".to_string()),
            new_name: None,
            new_project_id: None,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_duplicate_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }

    #[test]
    fn project_duplicate_json_succeeds() {
        let backend = FakeProjectBackend::success();
        let args = ProjectDuplicateArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: Some("agent-1".to_string()),
            new_name: Some("My Copy".to_string()),
            new_project_id: Some("agent-1-copy".to_string()),
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_duplicate_with_backend(args, &backend);
        assert_eq!(code, ExitCode::SUCCESS);
        let calls = backend.duplicate_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "us-1");
        assert_eq!(calls[0].1, "agent-1");
        assert_eq!(calls[0].2, "My Copy");
        assert_eq!(calls[0].3, Some("agent-1-copy".to_string()));
    }

    #[test]
    fn project_duplicate_propagates_api_error() {
        let backend = FakeProjectBackend {
            duplicate_result: Err("conflict".to_string()),
            ..FakeProjectBackend::success()
        };
        let args = ProjectDuplicateArgs {
            region: Some("us-1".to_string()),
            account_id: Some("acct-1".to_string()),
            project_id: Some("agent-1".to_string()),
            new_name: Some("My Copy".to_string()),
            new_project_id: None,
            json: true,
            debug: false,
            verbose: false,
        };

        let code = cmd_project_duplicate_with_backend(args, &backend);
        assert_eq!(code, ExitCode::from(1));
    }
}
