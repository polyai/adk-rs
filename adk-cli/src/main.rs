use adk_api_client::{AccountSummary, HttpPlatformClient, InMemoryPlatformClient, PlatformClient};
use adk_core::{AdkService, ProjectWorkspace};
use adk_push_pull::projection_to_resource_map;
use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod args;
mod branch;
mod chat;
mod completion;
mod console;
mod deployments;
mod diff;
mod docs;
mod format;
mod init;
mod output;
mod project;
mod pull;
mod push;
mod revert;
mod review;
mod self_update;
mod status;
mod validate;

pub(crate) use args::*;
use branch::cmd_branch;
use chat::cmd_chat;
use completion::cmd_completion;
use deployments::cmd_deployments;
use diff::cmd_diff;
use docs::cmd_docs;
use format::cmd_format;
use init::cmd_init;
pub(crate) use output::{clean_error_message, emit_error, print_payload};
use project::{cmd_project, project_debug, project_verbose};
use pull::cmd_pull;
use push::cmd_push;
use revert::cmd_revert;
use review::cmd_review;
use self_update::cmd_self_update;
use status::cmd_status;
use validate::cmd_validate;

macro_rules! with_remote_service {
    ($workspace:expr, $path:expr, $json_mode:expr, |$service:ident| $body:expr) => {{
        match http_service_for_path($workspace, $path) {
            Ok($service) => $body,
            Err(error) if allow_inmemory_fallback() => {
                if should_warn_inmemory_fallback(&error) {
                    emit_inmemory_fallback_warning($json_mode, &error);
                }
                let $service = local_service();
                $body
            }
            Err(error) => {
                emit_remote_service_error($json_mode, &error);
                ExitCode::from(1)
            }
        }
    }};
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            console::exception(error.to_string());
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    install_ctrlc_handler()?;
    let first_arg = std::env::args().nth(1);
    if first_arg
        .as_deref()
        .is_some_and(|arg| arg == "-h" || arg == "--help")
    {
        print_top_level_help();
        return Ok(ExitCode::SUCCESS);
    }
    if first_arg
        .as_deref()
        .is_some_and(|arg| arg == "-v" || arg == "--version")
    {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    let cli = Cli::parse();
    console::configure(command_verbose(&cli.command), command_debug(&cli.command));
    tracing::debug!("debug logging enabled");
    let workspace = ProjectWorkspace::new();

    let result = match cli.command {
        Commands::Help => {
            print_top_level_help();
            ExitCode::SUCCESS
        }
        Commands::Docs(args) => cmd_docs(args),
        Commands::Init(args) => cmd_init(&workspace, args),
        Commands::Project(args) => cmd_project(&workspace, args),
        Commands::Pull(args) => {
            if args.from_projection.is_some() {
                return Ok(cmd_pull(&local_service(), args));
            }
            with_remote_service!(&workspace, &args.path, args.json, |service| {
                cmd_pull(&service, args)
            })
        }
        Commands::Push(args) => with_remote_service!(&workspace, &args.path, args.json, |service| {
            cmd_push(&service, args)
        }),
        Commands::Status(args) => cmd_status(&workspace, args),
        Commands::Revert(args) => with_remote_service!(&workspace, &args.path, args.json, |service| {
            cmd_revert(&service, args)
        }),
        Commands::Diff(args) => with_remote_service!(&workspace, &args.path, args.json, |service| {
            cmd_diff(&service, args)
        }),
        Commands::Review(args) => cmd_review(args),
        Commands::Branch(args) => cmd_branch(args),
        Commands::Format(args) => cmd_format(args),
        Commands::Validate(args) => cmd_validate(args),
        Commands::Chat(args) => cmd_chat(args),
        Commands::SelfUpdate(args) => cmd_self_update(args),
        Commands::Completion(args) => cmd_completion(args),
        Commands::Deployments(args) => {
            let path = deployments_path(&args);
            let json_mode = deployments_json(&args);
            with_remote_service!(&workspace, path, json_mode, |service| {
                cmd_deployments(&service, args)
            })
        }
    };
    Ok(result)
}

fn install_ctrlc_handler() -> Result<()> {
    ctrlc::set_handler(|| {
        restore_terminal_state_after_interrupt();
        let _ = writeln!(io::stdout());
        console::plain("Cancelled by user");
        std::process::exit(130);
    })
    .map_err(|error| anyhow::anyhow!("failed to install Ctrl+C handler: {error}"))
}

fn restore_terminal_state_after_interrupt() {
    let _ = dialoguer::console::Term::stderr().show_cursor();
    let _ = dialoguer::console::Term::stdout().show_cursor();
}

fn print_top_level_help() {
    let mut output = String::from(
        concat!(
            "usage: poly [-h] [-v]\n",
            "            {help,docs,init,project,pull,push,status,revert,diff,review,branch,format,validate,chat,self-update,completion,deployments} ...\n\n",
            "positional arguments:\n",
            "  {help,docs,init,project,pull,push,status,revert,diff,review,branch,format,validate,chat,self-update,completion,deployments}\n",
        ),
    );
    for (name, description) in [
        ("help", "Show this help message and exit."),
        ("docs", "Outputs documentation for a given topic."),
        ("init", "Initialize a new Agent Studio project."),
        ("project", "Manage Agent Studio projects."),
        (
            "pull",
            "Pull the latest project configuration from Agent Studio.",
        ),
        ("push", "Push the project configuration to Agent Studio."),
        ("status", "Check the changed files of the project."),
        ("revert", "Revert changes in the project."),
        ("diff", "Show the changes made to the project."),
        ("review", "Incomplete: review Agent Studio project changes."),
        ("branch", "Manage branches in the Agent Studio project."),
        (
            "format",
            "Run ruff and YAML/JSON formatting on the project (optional ty with --ty).",
        ),
        ("validate", "Validate the project configuration locally."),
        ("chat", "Start an interactive chat session with the agent."),
        (
            "self-update",
            "Update the ADK CLI installed by the release shell installer.",
        ),
        ("completion", "Generate shell completion scripts"),
        ("deployments", "Manage deployments for the project."),
    ] {
        output.push_str(&format!(
            "    {}{description}\n",
            help_label(name, 20)
        ));
    }
    output.push_str("\noptions:\n");
    for (name, description) in [
        ("-h, --help", "show this help message and exit"),
        ("-v, --version", "show the version and exit"),
    ] {
        output.push_str(&format!(
            "  {}{description}\n",
            help_label(name, 22)
        ));
    }
    console::plain(output);
}

fn help_label(label: &str, width: usize) -> String {
    let padded = format!("{label:<width$}");
    format!("[label]{padded}[/label]")
}

fn deployments_path(args: &DeploymentsArgs) -> &str {
    match &args.command {
        DeploymentsCommands::List(args) => args.path.as_str(),
        DeploymentsCommands::Show(args) => args.path.as_str(),
        DeploymentsCommands::Promote(args) => args.path.as_str(),
        DeploymentsCommands::Rollback(args) => args.path.as_str(),
    }
}

fn deployments_json(args: &DeploymentsArgs) -> bool {
    match &args.command {
        DeploymentsCommands::List(args) => args.json,
        DeploymentsCommands::Show(args) => args.json,
        DeploymentsCommands::Promote(args) => args.json,
        DeploymentsCommands::Rollback(args) => args.json,
    }
}

fn command_verbose(command: &Commands) -> bool {
    match command {
        Commands::Help => false,
        Commands::Docs(args) => args.verbose,
        Commands::Init(args) => args.verbose,
        Commands::Project(args) => project_verbose(args),
        Commands::Pull(args) => args.verbose,
        Commands::Push(args) => args.verbose,
        Commands::Status(args) => args.verbose,
        Commands::Revert(args) => args.verbose,
        Commands::Diff(args) => args.verbose,
        Commands::Review(args) => {
            args.verbose
                || matches!(
                    &args.command,
                    Some(ReviewCommands::Create(create)) if create.verbose
                )
        }
        Commands::Branch(args) => branch_verbose(args),
        Commands::Format(args) => args.verbose,
        Commands::Validate(args) => args.verbose,
        Commands::Chat(args) => args.verbose,
        Commands::SelfUpdate(args) => args.verbose,
        Commands::Completion(_) => false,
        Commands::Deployments(args) => deployments_verbose(args),
    }
}

fn command_debug(command: &Commands) -> bool {
    match command {
        Commands::Init(args) => args.debug,
        Commands::Project(args) => project_debug(args),
        Commands::Pull(args) => args.debug,
        Commands::Push(args) => args.debug,
        Commands::Branch(args) => branch_debug(args),
        Commands::Chat(args) => args.debug,
        Commands::Deployments(args) => deployments_debug(args),
        _ => false,
    }
}

fn branch_verbose(args: &BranchArgs) -> bool {
    match &args.command {
        BranchCommands::List(args) | BranchCommands::Current(args) => args.verbose,
        BranchCommands::Create(args) => args.verbose,
        BranchCommands::Switch(args) => args.verbose,
        BranchCommands::Delete(args) => args.verbose,
        BranchCommands::Merge(args) => args.verbose,
    }
}

fn branch_debug(args: &BranchArgs) -> bool {
    match &args.command {
        BranchCommands::List(args) | BranchCommands::Current(args) => args.debug,
        BranchCommands::Create(args) => args.debug,
        BranchCommands::Switch(args) => args.debug,
        BranchCommands::Delete(args) => args.debug,
        BranchCommands::Merge(args) => args.debug,
    }
}

fn deployments_verbose(args: &DeploymentsArgs) -> bool {
    args.verbose
        || match &args.command {
            DeploymentsCommands::List(_) | DeploymentsCommands::Show(_) => false,
            DeploymentsCommands::Promote(args) => args.verbose,
            DeploymentsCommands::Rollback(args) => args.verbose,
        }
}

fn deployments_debug(args: &DeploymentsArgs) -> bool {
    match &args.command {
        DeploymentsCommands::Promote(args) => args.debug,
        DeploymentsCommands::Rollback(args) => args.debug,
        DeploymentsCommands::List(_) | DeploymentsCommands::Show(_) => false,
    }
}

fn account_choice(account: &AccountSummary) -> (String, String) {
    (
        account.id.clone(),
        format!("{} ({})", account.name, account.id),
    )
}

fn prompt_select(label: &str, choices: &[(String, String)]) -> Result<Option<String>, String> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        let labels = choices
            .iter()
            .map(|(_, title)| title.as_str())
            .collect::<Vec<_>>();
        let selection = dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(label)
            .items(&labels)
            .default(0)
            .interact_opt()
            .map_err(|error| format!("Failed to read selection: {error}"))?;
        return Ok(selection.map(|index| choices[index].0.clone()));
    }

    console::plain(format!("[label]{label}[/label]"));
    for (index, (_, title)) in choices.iter().enumerate() {
        console::plain(format!("  {}. {}", index + 1, title));
    }
    console::prompt("Enter selection: ")
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read selection: {error}"))?;
    if bytes == 0 || input.trim().is_empty() {
        return Ok(None);
    }
    let selected = input
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|index| choices.get(index.saturating_sub(1)))
        .map(|(value, _)| value.clone());
    if selected.is_none() {
        console::warning("Invalid selection. Exiting.");
    }
    Ok(selected)
}

fn prompt_text(label: &str, default: Option<&str>) -> Result<Option<String>, String> {
    match default {
        Some(default) if !default.is_empty() => console::prompt(format!("{label} [{default}] ")),
        _ => console::prompt(format!("{label} ")),
    }
    .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read input: {error}"))?;
    if bytes == 0 {
        return Ok(None);
    }
    let value = input.trim();
    if value.is_empty() {
        return Ok(default.map(ToString::to_string));
    }
    Ok(Some(value.to_string()))
}

fn prompt_multi_select(
    label: &str,
    choices: &[(String, String)],
) -> Result<Option<Vec<String>>, String> {
    console::plain(format!("[label]{label}[/label]"));
    for (index, (_, title)) in choices.iter().enumerate() {
        console::plain(format!("  {}. {}", index + 1, title));
    }
    console::prompt("Enter selections: ")
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read selections: {error}"))?;
    if bytes == 0 || input.trim().is_empty() {
        return Ok(None);
    }

    let mut selected = Vec::new();
    for token in input
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let value = if let Ok(index) = token.parse::<usize>() {
            choices
                .get(index.saturating_sub(1))
                .map(|(value, _)| value.clone())
        } else {
            choices
                .iter()
                .find(|(value, title)| value == token || title == token)
                .map(|(value, _)| value.clone())
        };
        let Some(value) = value else {
            console::warning("Invalid selection. Exiting.");
            return Ok(None);
        };
        if !selected.contains(&value) {
            selected.push(value);
        }
    }

    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(selected))
    }
}

fn prompt_confirm(message: &str) -> Result<bool, String> {
    console::prompt(format!("{message} [y/N] "))
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read confirmation: {error}"))?;
    if bytes == 0 {
        return Ok(false);
    }
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn prompt_branch_switch<C: PlatformClient>(
    service: &AdkService<C>,
    path: &Path,
) -> Result<Option<String>, String> {
    let current_branch = service
        .current_branch_name(path)
        .map_err(|error| error.to_string())?;
    let branches = service
        .list_branch_map(path)
        .map_err(|error| error.to_string())?;
    if branches.is_empty() {
        console::plain("[muted]No branches found.[/muted]");
        return Ok(None);
    }
    let choices = branches
        .keys()
        .map(|name| {
            let title = if name == &current_branch {
                format!("{name} (current)")
            } else {
                name.clone()
            };
            (name.clone(), title)
        })
        .collect::<Vec<_>>();
    let selected = prompt_select("Select Branch", &choices)?;
    if selected.is_none() {
        console::warning("No branch selected. Exiting.");
    }
    Ok(selected)
}

fn resolve_base_path(base_path: &str) -> PathBuf {
    let base_arg = PathBuf::from(base_path);
    if base_arg.is_absolute() {
        base_arg
    } else if base_arg == Path::new(".") {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(base_arg)
    }
}

fn normalize_cli_file_args(root: &Path, files: &[String]) -> Vec<String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_abs = if root.is_absolute() {
        root.to_path_buf()
    } else {
        cwd.join(root)
    };
    files
        .iter()
        .map(|file| {
            let path = PathBuf::from(file);
            if path.is_absolute() {
                if let Ok(relative) = path.strip_prefix(&root_abs) {
                    relative.to_string_lossy().replace('\\', "/")
                } else {
                    path.to_string_lossy().to_string()
                }
            } else {
                cwd.join(path).to_string_lossy().to_string()
            }
        })
        .collect()
}

fn read_stdin_line() -> String {
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
    input
}


fn ensure_project_loaded<C: PlatformClient>(
    service: &AdkService<C>,
    path: &str,
    json_mode: bool,
) -> bool {
    match service.load_project_config(PathBuf::from(path).as_path()) {
        Ok(_) => true,
        Err(_) => {
            emit_error(
                json_mode,
                "No project configuration found. Run poly init to initialize a project.",
            );
            false
        }
    }
}


fn parse_optional_json_arg(raw: Option<&str>) -> Result<Option<serde_json::Value>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut content = raw.trim().to_string();
    if content.is_empty() {
        return Ok(None);
    }
    if content == "-" {
        content.clear();
        std::io::stdin()
            .read_to_string(&mut content)
            .map_err(|e| e.to_string())?;
    }
    let mut parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON in --from-projection: {e}"))?;
    if parsed
        .as_object()
        .is_some_and(|obj| obj.contains_key("projection"))
    {
        parsed = parsed
            .get("projection")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
    }
    if !parsed.is_object() {
        return Err("--from-projection must be a JSON object (dictionary).".to_string());
    }
    Ok(Some(parsed))
}

fn pull_projection_into_path(
    path: &std::path::Path,
    projection: &serde_json::Value,
    force: bool,
    format: bool,
) -> Result<Vec<String>, String> {
    let resources = projection_to_resource_map(projection).map_err(|e| e.to_string())?;
    local_service()
        .pull_resource_map_with_format(path, resources, force, format)
        .map_err(|error| error.to_string())
}

fn local_service() -> AdkService<InMemoryPlatformClient> {
    AdkService::new(InMemoryPlatformClient::default())
}

/// Builds the service used for commands that require the remote platform.
///
/// Local project config is read by `ProjectWorkspace`, then the CLI constructs
/// the real HTTP client for the configured project and branch.
fn http_service_for_path(
    workspace: &ProjectWorkspace,
    path: &str,
) -> Result<AdkService<HttpPlatformClient>, String> {
    let cfg = workspace
        .load_project_config(PathBuf::from(path).as_path())
        .map_err(|_| {
            "No project configuration found. Run poly init to initialize a project.".to_string()
        })?;
    HttpPlatformClient::new(
        &cfg.region,
        &cfg.account_id,
        &cfg.project_id,
        Some(&cfg.branch_id),
    )
    .map(AdkService::new)
    .map_err(|error| format!("remote platform client unavailable: {error}"))
}

fn allow_inmemory_fallback() -> bool {
    std::env::var("POLY_ADK_ALLOW_INMEMORY_FALLBACK").unwrap_or_default() == "1"
}

fn should_warn_inmemory_fallback(error: &str) -> bool {
    error.starts_with("remote platform client unavailable")
}

fn emit_inmemory_fallback_warning(json_mode: bool, error: &str) {
    if json_mode {
        eprintln!(
            "{}",
            json!({"warning": format!("{error}, using in-memory fallback")})
        );
    } else {
        console::warning(format!("{error}, using in-memory fallback"));
    }
}

fn emit_remote_service_error(json_mode: bool, error: &str) {
    if error.starts_with("remote platform client unavailable") {
        emit_error(
            json_mode,
            &format!(
                "{error} (set POLY_ADK_ALLOW_INMEMORY_FALLBACK=1 to opt into local fallback)"
            ),
        );
    } else {
        emit_error(json_mode, error);
    }
}
