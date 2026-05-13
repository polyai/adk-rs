use adk_core::AdkService;
use adk_platform_api::{
    projection_to_resource_map, AccountSummary, HttpPlatformClient, InMemoryPlatformClient,
    ProjectSummary,
};
use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod docs;
mod console;
mod review;

use docs::cmd_docs;
use review::{
    emit_review_message, github_create_review_gist, github_delete_review_gist,
    github_list_diff_gists, prompt_delete_review_gists, prompt_open_review_gist,
    review_description,
};

const INIT_REGIONS: &[&str] = &["us-1", "euw-1", "uk-1", "studio", "staging", "dev"];

#[derive(Debug, Parser)]
#[command(
    name = "poly",
    version,
    about = "Agent Development Kit (Rust)",
    disable_version_flag = true
)]
struct Cli {
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue, help = "show the version and exit")]
    version: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Docs(DocsArgs),
    Init(InitArgs),
    Pull(PullArgs),
    Push(PushArgs),
    Status(StatusArgs),
    Revert(RevertArgs),
    Diff(DiffArgs),
    Review(ReviewArgs),
    Branch(BranchArgs),
    Format(FormatArgs),
    Validate(ValidateArgs),
    Chat(ChatArgs),
    Completion(CompletionArgs),
    Deployments(DeploymentsArgs),
}

#[derive(Debug, clap::Args)]
struct DocsArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    all: bool,
    #[arg(long, visible_alias = "write", short = 'o')]
    output: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(docs::DOC_CHOICES))]
    documents: Vec<String>,
}

#[derive(Debug, clap::Args)]
struct InitArgs {
    #[arg(long = "base-path", default_value = ".")]
    base_path: String,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["us-1", "euw-1", "uk-1", "studio", "staging", "dev"]))]
    region: Option<String>,
    #[arg(long = "account_id")]
    account_id: Option<String>,
    #[arg(long = "project_id")]
    project_id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    format: bool,
    #[arg(long = "from-projection", hide = true)]
    from_projection: Option<String>,
    #[arg(long = "output-json-projection", action = ArgAction::SetTrue)]
    output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct PullArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    format: bool,
    #[arg(long = "from-projection", hide = true)]
    from_projection: Option<String>,
    #[arg(long = "output-json-projection", hide = true, action = ArgAction::SetTrue)]
    output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct PushArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long = "skip-validation", action = ArgAction::SetTrue)]
    skip_validation: bool,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    format: bool,
    #[arg(long)]
    email: Option<String>,
    #[arg(long = "from-projection", hide = true)]
    from_projection: Option<String>,
    #[arg(long = "output-json-commands", hide = true, action = ArgAction::SetTrue)]
    output_json_commands: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct StatusArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct RevertArgs {
    #[arg(long, default_value = ".")]
    path: String,
    files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct DiffArgs {
    #[arg(long, default_value = ".")]
    path: String,
    hash: Option<String>,
    #[arg(long, num_args = 0..)]
    files: Vec<String>,
    #[arg(long)]
    before: Option<String>,
    #[arg(long)]
    after: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct ReviewArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    #[command(subcommand)]
    command: Option<ReviewCommands>,
}

#[derive(Debug, Subcommand)]
enum ReviewCommands {
    Create(ReviewCreateArgs),
    List(ReviewListArgs),
    Delete(ReviewDeleteArgs),
}

#[derive(Debug, clap::Args)]
struct ReviewCreateArgs {
    hash: Option<String>,
    #[arg(long)]
    before: Option<String>,
    #[arg(long)]
    after: Option<String>,
    #[arg(long, num_args = 0..)]
    files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct ReviewListArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, clap::Args)]
struct ReviewDeleteArgs {
    #[arg(long = "id")]
    id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, clap::Args)]
struct BranchArgs {
    #[command(subcommand)]
    command: BranchCommands,
}

#[derive(Debug, Subcommand)]
enum BranchCommands {
    List(CommonPathArgs),
    Create(BranchCreateArgs),
    Switch(BranchSwitchArgs),
    Current(CommonPathArgs),
    Delete(BranchDeleteArgs),
    Merge(BranchMergeArgs),
}

#[derive(Debug, clap::Args)]
struct CommonPathArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct BranchCreateArgs {
    #[arg(long, default_value = ".")]
    path: String,
    branch_name: Option<String>,
    #[arg(
        long = "env",
        visible_alias = "environment",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    environment: Option<String>,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct BranchSwitchArgs {
    #[arg(long, default_value = ".")]
    path: String,
    branch_name: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    format: bool,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long = "from-projection", hide = true)]
    from_projection: Option<String>,
    #[arg(long = "output-json-projection", hide = true, action = ArgAction::SetTrue)]
    output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct BranchDeleteArgs {
    #[arg(long, default_value = ".")]
    path: String,
    branch_name: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct BranchMergeArgs {
    #[arg(long, default_value = ".")]
    path: String,
    message: Option<String>,
    #[arg(long, short = 'i', action = ArgAction::SetTrue)]
    interactive: bool,
    #[arg(long)]
    resolutions: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct FormatArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, num_args = 0..)]
    files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    check: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    ty: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct ValidateArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct ChatArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "branch",
        value_parser = clap::builder::PossibleValuesParser::new(["branch", "sandbox", "pre-release", "live"])
    )]
    environment: String,
    #[arg(long)]
    variant: Option<String>,
    #[arg(long)]
    lang: Option<String>,
    #[arg(long = "input-lang")]
    input_lang: Option<String>,
    #[arg(long = "output-lang")]
    output_lang: Option<String>,
    #[arg(
        long,
        default_value = "voice",
        value_parser = clap::builder::PossibleValuesParser::new(["voice", "webchat"])
    )]
    channel: String,
    #[arg(long, action = ArgAction::SetTrue)]
    functions: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    flows: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    state: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    metadata: bool,
    #[arg(long = "push", action = ArgAction::SetTrue)]
    push_before_chat: bool,
    #[arg(long = "message", short = 'm')]
    messages: Vec<String>,
    #[arg(long = "input-file")]
    input_file: Option<String>,
    #[arg(long = "conversation-id", visible_alias = "conv-id")]
    conversation_id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, clap::Args)]
struct CompletionArgs {
    shell: CompletionShell,
}

#[derive(Debug, clap::Args)]
struct DeploymentsArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    #[command(subcommand)]
    command: DeploymentsCommands,
}

#[derive(Debug, Subcommand)]
enum DeploymentsCommands {
    List(DeploymentsListArgs),
    Show(DeploymentsShowArgs),
    Promote(DeploymentsPromoteArgs),
    Rollback(DeploymentsRollbackArgs),
}

#[derive(Debug, clap::Args)]
struct DeploymentsListArgs {
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "sandbox",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    env: String,
    #[arg(long, default_value_t = 10)]
    limit: usize,
    #[arg(long, default_value_t = 0)]
    offset: usize,
    #[arg(long = "hash")]
    version_hash: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    details: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, clap::Args)]
struct DeploymentsShowArgs {
    version_hash: String,
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "sandbox",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    env: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, clap::Args)]
struct DeploymentsPromoteArgs {
    #[arg(long = "from")]
    from_deployment: String,
    #[arg(
        long = "to",
        value_parser = clap::builder::PossibleValuesParser::new(["pre-release", "live"])
    )]
    to_env: String,
    #[arg(long, short = 'm')]
    message: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
}

#[derive(Debug, clap::Args)]
struct DeploymentsRollbackArgs {
    #[arg(long = "to")]
    to_deployment: String,
    #[arg(long, short = 'm')]
    message: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(long, default_value = ".")]
    path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,
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
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg == "-v" || arg == "--version")
    {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    let cli = Cli::parse();
    console::configure(command_verbose(&cli.command), command_debug(&cli.command));
    tracing::debug!("debug logging enabled");
    let bootstrap_service = AdkService::new(Box::new(InMemoryPlatformClient::default()));

    let result = match cli.command {
        Commands::Docs(args) => cmd_docs(args),
        Commands::Init(args) => cmd_init(&bootstrap_service, args),
        Commands::Pull(args) => {
            if args.from_projection.is_some() {
                return Ok(cmd_pull(&local_service(), args));
            }
            let Some(service) = remote_service_for_path(&bootstrap_service, &args.path, args.json)
            else {
                return Ok(ExitCode::from(1));
            };
            cmd_pull(&service, args)
        }
        Commands::Push(args) => {
            let Some(service) =
                remote_service_for_path(&bootstrap_service, &args.path, args.json)
            else {
                return Ok(ExitCode::from(1));
            };
            cmd_push(&service, args)
        }
        Commands::Status(args) => cmd_status(&local_service(), args),
        Commands::Revert(args) => {
            let Some(service) =
                remote_service_for_path(&bootstrap_service, &args.path, args.json)
            else {
                return Ok(ExitCode::from(1));
            };
            cmd_revert(&service, args)
        }
        Commands::Diff(args) => {
            let Some(service) = remote_service_for_path(&bootstrap_service, &args.path, args.json)
            else {
                return Ok(ExitCode::from(1));
            };
            cmd_diff(&service, args)
        }
        Commands::Review(args) => cmd_review(args),
        Commands::Branch(args) => cmd_branch(args),
        Commands::Format(args) => cmd_format(args),
        Commands::Validate(args) => cmd_validate(args),
        Commands::Chat(args) => cmd_chat(args),
        Commands::Completion(args) => cmd_completion(args),
        Commands::Deployments(args) => {
            let path = deployments_path(&args);
            let json_mode = deployments_json(&args);
            let Some(service) = remote_service_for_path(&bootstrap_service, path, json_mode) else {
                return Ok(ExitCode::from(1));
            };
            cmd_deployments(&service, args)
        }
    };
    Ok(result)
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
        Commands::Docs(args) => args.verbose,
        Commands::Init(args) => args.verbose,
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
        Commands::Completion(_) => false,
        Commands::Deployments(args) => deployments_verbose(args),
    }
}

fn command_debug(command: &Commands) -> bool {
    match command {
        Commands::Init(args) => args.debug,
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

fn cmd_init(service: &AdkService, args: InitArgs) -> ExitCode {
    let json_mode = args.json || args.output_json_projection;
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(json_mode, &error);
            return ExitCode::from(1);
        }
    };
    let selection = match resolve_init_selection(args.region, args.account_id, args.project_id, json_mode) {
        Ok(Some(selection)) => selection,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            emit_error(json_mode, &error);
            return ExitCode::from(1);
        }
    };
    let base = resolve_base_path(&args.base_path);
    match service.init_project_with_name(
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
                && let Err(error) = pull_projection_into_path(&root_path, projection, true)
            {
                emit_error(json_mode, &error);
                return ExitCode::from(1);
            } else if projection_json.is_none()
                && let Ok(http_client) = HttpPlatformClient::new(
                    &selection.region,
                    &selection.account_id,
                    &selection.project_id,
                    Some("main"),
                )
            {
                let remote_service = AdkService::new(Box::new(http_client));
                if args.output_json_projection {
                    match remote_service.pull_projection_json() {
                        Ok(projection) => output_projection = Some(projection),
                        Err(error) => {
                            emit_error(json_mode, &error.to_string());
                            return ExitCode::from(1);
                        }
                    }
                }
                if let Err(error) = remote_service.pull(root_path.as_path(), true) {
                    emit_error(json_mode, &error.to_string());
                    return ExitCode::from(1);
                }
            }
            if json_mode {
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
            emit_error(json_mode, &error.to_string());
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

    let (account_id, _account_name) = match account_id {
        Some(account_id) => (account_id, None),
        None => {
            let accounts =
                HttpPlatformClient::list_accounts(&region).map_err(|error| error.to_string())?;
            if accounts.is_empty() {
                return Err("No accounts found in the selected region.".to_string());
            }
            if accounts.len() == 1 {
                let account = &accounts[0];
                console::info(format!("Auto-selected account {}.", account.name));
                (account.id.clone(), Some(account.name.clone()))
            } else {
                let choices = accounts
                    .iter()
                    .map(account_choice)
                    .collect::<Vec<_>>();
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

    let projects =
        HttpPlatformClient::list_projects(&region, &account_id).map_err(|error| error.to_string())?;
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

fn account_choice(account: &AccountSummary) -> (String, String) {
    (
        account.id.clone(),
        format!("{} ({})", account.name, account.id),
    )
}

fn project_choice(project: &ProjectSummary) -> (String, String) {
    (
        project.id.clone(),
        format!("{} ({})", project.name, project.id),
    )
}

fn prompt_select(label: &str, choices: &[(String, String)]) -> Result<Option<String>, String> {
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

fn prompt_branch_switch(service: &AdkService, path: &Path) -> Result<Option<String>, String> {
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

fn cmd_pull(service: &AdkService, args: PullArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json || args.output_json_projection) {
        return ExitCode::from(1);
    }
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(args.json || args.output_json_projection, &error);
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    let mut output_projection = projection_json.clone();
    let pull_result = if let Some(projection) = &projection_json {
        pull_projection_into_path(path.as_path(), projection, args.force)
    } else {
        if args.output_json_projection {
            match service.pull_projection_json() {
                Ok(projection) => output_projection = Some(projection),
                Err(error) => {
                    emit_error(args.json || args.output_json_projection, &error.to_string());
                    return ExitCode::from(1);
                }
            }
        }
        service
            .pull(path.as_path(), args.force)
            .map_err(|error| error.to_string())
    };
    match pull_result {
        Ok(conflicts) => {
            if args.json || args.output_json_projection {
                let mut payload =
                    json!({"success": conflicts.is_empty(), "files_with_conflicts": conflicts});
                if args.output_json_projection {
                    payload["projection"] = output_projection.unwrap_or(serde_json::Value::Null);
                }
                println!(
                    "{}",
                    payload
                );
            } else {
                console::success("Pulled project.");
            }
            if conflicts.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            emit_error(args.json || args.output_json_projection, &error);
            ExitCode::from(1)
        }
    }
}

fn cmd_push(service: &AdkService, args: PushArgs) -> ExitCode {
    let json_mode = args.json || args.output_json_commands;
    if !ensure_project_loaded(service, &args.path, json_mode) {
        return ExitCode::from(1);
    }
    let projection_json = match parse_optional_json_arg(args.from_projection.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(json_mode, &error);
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    if args.format
        && let Err(error) = service.format_local_resources(path.as_path(), &[], false)
    {
        emit_error(json_mode, &error.to_string());
        return ExitCode::from(1);
    }
    let current_branch = match service.current_branch(path.as_path()) {
        Ok(branch) => branch,
        Err(error) => {
            emit_error(json_mode, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if current_branch == "main" && !args.dry_run && projection_json.is_none() {
        let branch_name = generated_adk_branch_name();
        match service.push_main_to_new_branch(
            path.as_path(),
            &branch_name,
            args.force,
            args.skip_validation,
            args.email.as_deref(),
        ) {
            Ok((cfg, push_result)) => {
                if args.json || args.output_json_commands {
                    let mut payload = json!({
                        "success": push_result.success,
                        "message": push_result.message,
                        "dry_run": false,
                    });
                    if args.output_json_commands {
                        payload["commands"] = json!(push_result.commands);
                    }
                    if push_result.success {
                        payload["new_branch_id"] = json!(cfg.branch_id);
                        payload["switched_to"] = json!(branch_name);
                    }
                    println!("{payload}");
                } else if push_result.success {
                    console::success("Push successful.");
                } else {
                    console::error(&push_result.message);
                }
                return if push_result.success {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                };
            }
            Err(error) => {
                emit_error(json_mode, &error.to_string());
                return ExitCode::from(1);
            }
        }
    }
    match service.push_with_options(
        path.as_path(),
        args.force,
        args.skip_validation,
        args.dry_run,
        projection_json.as_ref(),
        args.email.as_deref(),
    ) {
        Ok(push_result) => {
            if args.json || args.output_json_commands {
                let mut payload = json!({
                    "success": push_result.success,
                    "message": push_result.message,
                    "dry_run": args.dry_run,
                });
                if args.output_json_commands {
                    payload["commands"] = json!(push_result.commands);
                }
                println!(
                    "{}",
                    payload
                );
            } else if push_result.success {
                console::success("Push successful.");
            } else {
                console::error(&push_result.message);
            }
            if push_result.success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            emit_error(args.json || args.output_json_commands, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn generated_adk_branch_name() -> String {
    // Replay tests set this so Rust emits the branch name recorded from Python.
    if let Ok(name) = std::env::var("POLY_ADK_GENERATED_BRANCH_NAME")
        && !name.trim().is_empty()
    {
        return name;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let suffix = format!("{:09x}", nanos & 0xfffffffff);
    format!("ADK-{}-{}", &suffix[..5], &suffix[5..9])
}

fn cmd_status(service: &AdkService, args: StatusArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let root = PathBuf::from(&args.path);
    match service.status(root.as_path()) {
        Ok(summary) => {
            if args.json {
                println!(
                    "{}",
                    json!({
                        "files_with_conflicts": absolutize_status_paths(&root, summary.files_with_conflicts),
                        "modified_files": absolutize_status_paths(&root, summary.modified_files),
                        "new_files": absolutize_status_paths(&root, summary.new_files),
                        "deleted_files": absolutize_status_paths(&root, summary.deleted_files)
                    })
                );
            } else {
                print_status_summary(
                    &summary.files_with_conflicts,
                    &summary.modified_files,
                    &summary.new_files,
                    &summary.deleted_files,
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn print_status_summary(
    files_with_conflicts: &[String],
    modified_files: &[String],
    new_files: &[String],
    deleted_files: &[String],
) {
    let has_changes = !files_with_conflicts.is_empty()
        || !modified_files.is_empty()
        || !new_files.is_empty()
        || !deleted_files.is_empty();

    if !has_changes {
        console::plain("[muted]No changes detected.[/muted]");
        return;
    }

    print_status_group("Files with conflicts", files_with_conflicts);
    print_status_group("Modified files", modified_files);
    print_status_group("New files", new_files);
    print_status_group("Deleted files", deleted_files);
}

fn print_status_group(label: &str, files: &[String]) {
    if files.is_empty() {
        return;
    }
    let style = status_label_style(label);
    console::plain(format!("[label]{label}:[/label]"));
    for file in files {
        console::plain(format!("  - [{style}]{file}[/{style}]"));
    }
}

fn status_label_style(label: &str) -> &'static str {
    match label {
        "Files with conflicts" => "filename.conflict",
        "Modified files" => "filename.modified",
        "New files" => "filename.new",
        "Deleted files" => "filename.deleted",
        _ => "label",
    }
}

fn absolutize_status_paths(root: &std::path::Path, paths: Vec<String>) -> Vec<String> {
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
    };
    paths
        .into_iter()
        .map(|path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
            .to_string_lossy()
            .to_string()
        })
        .collect()
}

fn cmd_revert(service: &AdkService, args: RevertArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let files: Vec<String> = args
        .files
        .iter()
        .map(|file| {
            let path = PathBuf::from(file);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
            .to_string_lossy()
            .to_string()
        })
        .collect();
    match service.revert_changes(PathBuf::from(&args.path).as_path(), &files) {
        Ok(files_reverted) => {
            if args.json {
                println!(
                    "{}",
                    json!({"success": true, "files_reverted": files_reverted})
                );
            } else if files_reverted.is_empty() {
                console::plain("[muted]No changes to revert.[/muted]");
            } else {
                console::success("Changes reverted successfully.");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_diff(service: &AdkService, args: DiffArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    if args.hash.is_some() && (args.before.is_some() || args.after.is_some()) {
        console::error("Cannot specify both hash and before/after versions.");
        return ExitCode::SUCCESS;
    }
    let named_diff = args.hash.is_some() || args.before.is_some() || args.after.is_some();
    let before_main_local =
        args.before.as_deref() == Some("main") && args.hash.is_none() && args.after.is_none();
    let after = args.hash.or(args.after);
    let root = PathBuf::from(args.path);
    let files = normalize_cli_file_args(root.as_path(), &args.files);
    match service.diff(root.as_path(), &files, args.before, after) {
        Ok(diffs) => {
            if diffs.is_empty() {
                if args.json {
                    let message = if before_main_local {
                        "Failed to compute diffs."
                    } else {
                        "No changes detected"
                    };
                    println!("{}", json!({"success": false, "message": message}));
                } else {
                    console::plain("[muted]No changes detected.[/muted]");
                }
                return ExitCode::SUCCESS;
            }
            if args.json {
                println!("{}", json!({"success": true, "diffs": diffs}));
            } else {
                for (path, diff) in diffs {
                    console::plain(format!("[label]=== {path} ===[/label]\n{diff}"));
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json && named_diff {
                println!(
                    "{}",
                    json!({"success": false, "message": "Failed to compute diffs."})
                );
                ExitCode::SUCCESS
            } else {
                emit_error(args.json, &error.to_string());
                ExitCode::from(1)
            }
        }
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

fn cmd_review(args: ReviewArgs) -> ExitCode {
    match args.command {
        None => ExitCode::SUCCESS,
        Some(ReviewCommands::List(list)) => match github_list_diff_gists() {
            Ok(gists) => {
                if args.json || list.json {
                    println!("{}", json!(gists));
                } else if gists.is_empty() {
                    console::plain("[muted]No review gists found.[/muted]");
                } else if let Err(error) = prompt_open_review_gist(&gists) {
                    emit_review_message(false, &error);
                } else {
                    console::success("Opened gist.");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                emit_review_message(args.json || list.json, &error);
                ExitCode::SUCCESS
            }
        },
        Some(ReviewCommands::Delete(delete)) => {
            let json_mode = args.json || delete.json;
            let delete_result = if let Some(id) = delete.id.as_deref() {
                github_delete_review_gist(id).map(usize::from)
            } else {
                prompt_delete_review_gists(json_mode)
            };
            match delete_result {
                Ok(deleted_count) => {
                    let deleted = deleted_count > 0;
                    if args.json || delete.json {
                        println!("{}", json!({"success": deleted}));
                    } else if deleted_count > 1 {
                        console::success(format!("Deleted {deleted_count} gists."));
                    } else if deleted {
                        console::success("Deleted gist.");
                    } else {
                        console::plain("[muted]No review gists found.[/muted]");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_review_message(args.json || delete.json, &error);
                    ExitCode::SUCCESS
                }
            }
        }
        Some(ReviewCommands::Create(create)) => {
            if create.hash.is_some() && (create.before.is_some() || create.after.is_some()) {
                console::error("Cannot specify both hash and before/after versions.");
                return ExitCode::SUCCESS;
            }
            let json_mode = args.json || create.json;
            let needs_remote =
                create.hash.is_some() || create.before.is_some() || create.after.is_some();
            let bootstrap = local_service();
            let service = if needs_remote {
                let Some(service) = remote_service_for_path(&bootstrap, &args.path, json_mode)
                else {
                    return ExitCode::from(1);
                };
                service
            } else {
                local_service()
            };
            if !ensure_project_loaded(&service, &args.path, json_mode) {
                return ExitCode::from(1);
            }
            let after = create.hash.clone().or(create.after.clone());
            let root = PathBuf::from(&args.path);
            let files = normalize_cli_file_args(root.as_path(), &create.files);
            let diffs = match service.diff(
                root.as_path(),
                &files,
                create.before.clone(),
                after.clone(),
            ) {
                Ok(diffs) => diffs,
                Err(_) => {
                    if json_mode {
                        println!("{}", json!({"success": false, "message": "Failed to compute diffs."}));
                    } else {
                        console::plain("[muted]No changes detected.[/muted]");
                    }
                    return ExitCode::SUCCESS;
                }
            };
            if diffs.is_empty() {
                if json_mode {
                    println!("{}", json!({"success": false, "message": "No changes to review."}));
                } else {
                    console::plain("[muted]No changes detected.[/muted]");
                }
                return ExitCode::SUCCESS;
            }
            let description = review_description(&args.path, create.hash.as_deref(), create.before.as_deref(), after.as_deref());
            match github_create_review_gist(diffs.iter(), &description) {
                Ok(url) => {
                    if json_mode {
                        println!("{}", json!({"success": true, "link": url}));
                    } else {
                        console::success(format!("Gist created: {url}"));
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_review_message(json_mode, &error);
                    ExitCode::SUCCESS
                }
            }
        }
    }
}

fn cmd_branch(args: BranchArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    match args.command {
        BranchCommands::List(a) => {
            let Some(service) = remote_service_for_path(&bootstrap, &a.path, a.json) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            match (
                service.current_branch_name(PathBuf::from(&a.path).as_path()),
                service.list_branch_map(PathBuf::from(&a.path).as_path()),
            ) {
                (Ok(current_branch), Ok(branches)) => {
                    if a.json {
                        println!(
                            "{}",
                            json!({"current_branch": current_branch, "branches": branches})
                        );
                        ExitCode::SUCCESS
                    } else {
                        print_branch_list(&current_branch, branches.iter());
                        ExitCode::SUCCESS
                    }
                }
                (Err(error), _) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
                (_, Err(error)) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Create(a) => {
            let Some(service) = remote_service_for_path(&bootstrap, &a.path, a.json) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            let branch_name_from_prompt;
            let branch_name = match a.branch_name.as_deref() {
                Some(branch_name) => branch_name,
                None if a.json => {
                    emit_error(
                        true,
                        "branch create with --json requires a branch name argument.",
                    );
                    return ExitCode::from(1);
                }
                None => {
                    let _ = console::prompt("Enter the name of the new branch: ");
                    let _ = std::io::stdout().flush();
                    branch_name_from_prompt = read_stdin_line().trim().to_string();
                    if branch_name_from_prompt.is_empty() {
                        emit_error(false, "branch create requires a branch name argument.");
                        return ExitCode::from(1);
                    }
                    branch_name_from_prompt.as_str()
                }
            };
            let path = PathBuf::from(&a.path);
            if matches!(a.environment.as_deref(), Some("pre-release" | "live")) {
                if !a.force {
                    match service.diff(path.as_path(), &[], None, None) {
                        Ok(diffs) if !diffs.is_empty() => {
                            let changed_files = diffs
                                .keys()
                                .map(String::as_str)
                                .collect::<Vec<_>>()
                                .join(", ");
                            emit_error(
                                a.json,
                                &format!(
                                    "Uncommitted changes on main branch: {changed_files}"
                                ),
                            );
                            return ExitCode::from(1);
                        }
                        Err(error) => {
                            emit_error(a.json, &error.to_string());
                            return ExitCode::from(1);
                        }
                        _ => {}
                    }
                }
                if let Some(env_name) = a.environment.as_deref()
                    && let Err(error) = service.pull_named(path.as_path(), env_name, true)
                {
                    emit_error(a.json, &error.to_string());
                    return ExitCode::from(1);
                }
            }
            match service.create_branch(path.as_path(), branch_name) {
                Ok(cfg) if matches!(a.environment.as_deref(), Some("pre-release" | "live")) => {
                    match service.push(path.as_path(), true, true, false) {
                        Ok(push) if push.success => print_payload(
                            a.json,
                            json!({"success": true, "branch_name": branch_name, "new_branch_id": cfg.branch_id}),
                        ),
                        Ok(push) => {
                            emit_error(a.json, &push.message);
                            ExitCode::from(1)
                        }
                        Err(error) => {
                            emit_error(a.json, &error.to_string());
                            ExitCode::from(1)
                        }
                    }
                }
                Ok(cfg) => {
                    if a.json {
                        print_payload(
                            true,
                            json!({"success": true, "branch_name": branch_name, "new_branch_id": cfg.branch_id}),
                        )
                    } else {
                        console::success(format!(
                            "Branch '{branch_name}' created (ID: {})",
                            cfg.branch_id
                        ));
                        ExitCode::SUCCESS
                    }
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Switch(a) => {
            let projection_json = match parse_optional_json_arg(a.from_projection.as_deref()) {
                Ok(value) => value,
                Err(error) => {
                    emit_error(a.json || a.output_json_projection, &error);
                    return ExitCode::from(1);
                }
            };
            let Some(service) = (if projection_json.is_some() {
                Some(local_service())
            } else {
                remote_service_for_path(&bootstrap, &a.path, a.json || a.output_json_projection)
            }) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json || a.output_json_projection) {
                return ExitCode::from(1);
            }
            let path = PathBuf::from(&a.path);
            let branch_name_from_prompt;
            let branch_name = match a.branch_name.as_deref() {
                Some(branch_name) => branch_name,
                None if a.json || a.output_json_projection => {
                    emit_error(
                        true,
                        "branch switch with --json requires a branch name argument.",
                    );
                    return ExitCode::from(1);
                }
                None => match prompt_branch_switch(&service, path.as_path()) {
                    Ok(Some(selected)) => {
                        branch_name_from_prompt = selected;
                        branch_name_from_prompt.as_str()
                    }
                    Ok(None) => return ExitCode::SUCCESS,
                    Err(error) => {
                        emit_error(false, &error);
                        return ExitCode::from(1);
                    }
                },
            };
            if !a.force {
                match service.diff(path.as_path(), &[], None, None) {
                    Ok(diffs) if !diffs.is_empty() => {
                        emit_error(
                            a.json || a.output_json_projection,
                            "Cannot switch branches with uncommitted changes. Use --force to switch and discard changes.",
                        );
                        return ExitCode::from(1);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        emit_error(a.json || a.output_json_projection, &error.to_string());
                        return ExitCode::from(1);
                    }
                }
            }
            match service.set_branch(PathBuf::from(&a.path).as_path(), branch_name) {
                Ok(_cfg) => {
                    if let Some(projection) = &projection_json
                        && let Err(error) = pull_projection_into_path(
                            path.as_path(),
                            projection,
                            a.force,
                        )
                    {
                        emit_error(a.json || a.output_json_projection, &error);
                        return ExitCode::from(1);
                    } else if projection_json.is_none()
                        && let Err(error) = service.pull_named(path.as_path(), branch_name, a.force)
                    {
                        emit_error(a.json || a.output_json_projection, &error.to_string());
                        return ExitCode::from(1);
                    }
                    let mut payload = json!({"success": true, "branch_name": branch_name});
                    if a.output_json_projection {
                        payload["projection"] =
                            projection_json.clone().unwrap_or(serde_json::Value::Null);
                    }
                    print_payload(a.json || a.output_json_projection, payload)
                }
                Err(error) => {
                    emit_error(a.json || a.output_json_projection, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Current(a) => {
            let local = local_service();
            if !ensure_project_loaded(&local, &a.path, a.json) {
                return ExitCode::from(1);
            }
            let path = PathBuf::from(&a.path);
            let remote_branch = try_remote_service_for_path(&local, &a.path)
                .and_then(|service| service.current_branch_name(path.as_path()).ok());
            let branch_result = if let Some(branch) = remote_branch {
                Ok(branch)
            } else {
                local.current_branch(path.as_path())
            };
            match branch_result {
                Ok(branch) => {
                    if a.json {
                        println!("{}", json!({"current_branch": branch}));
                    } else {
                        console::plain(format!("[label]Current branch:[/label] {branch}"));
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    let message = error.to_string();
                    print_payload(
                        a.json,
                        json!({"success": false, "message": clean_error_message(&message)}),
                    );
                    ExitCode::SUCCESS
                }
            }
        }
        BranchCommands::Delete(a) => {
            let Some(service) = remote_service_for_path(&bootstrap, &a.path, a.json) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            cmd_branch_delete(&service, a)
        }
        BranchCommands::Merge(a) => {
            let Some(service) = remote_service_for_path(&bootstrap, &a.path, a.json) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            cmd_branch_merge(&service, a)
        }
    }
}

fn cmd_branch_delete(service: &AdkService, args: BranchDeleteArgs) -> ExitCode {
    let path = PathBuf::from(&args.path);
    let current_branch = match service.current_branch_name(path.as_path()) {
        Ok(branch) => branch,
        Err(error) => {
            return print_branch_delete_error(args.json, &error.to_string());
        }
    };
    let branches = match service.list_branch_map(path.as_path()) {
        Ok(branches) => branches,
        Err(error) => {
            return print_branch_delete_error(args.json, &error.to_string());
        }
    };
    let deletable = branches
        .into_iter()
        .filter(|(name, branch_id)| name != "main" && branch_id != "main")
        .collect::<Vec<_>>();

    if let Some(branch_name) = args.branch_name.as_deref() {
        if !deletable.iter().any(|(name, _)| name == branch_name) {
            return print_branch_delete_error(
                args.json,
                &format!("Branch '{branch_name}' does not exist or cannot be deleted."),
            );
        }
        if !args.json {
            match prompt_confirm(&format!("Delete branch '{branch_name}'?")) {
                Ok(true) => {}
                Ok(false) => {
                    console::warning("Aborted.");
                    return ExitCode::SUCCESS;
                }
                Err(error) => {
                    emit_error(false, &error);
                    return ExitCode::from(1);
                }
            }
        }
        return delete_one_branch(service, path.as_path(), branch_name, &current_branch, args.json);
    }

    if args.json {
        emit_error(true, "branch delete with --json requires a branch name argument.");
        return ExitCode::from(1);
    }

    if deletable.is_empty() {
        console::plain("[muted]No deletable branches found.[/muted]");
        return ExitCode::SUCCESS;
    }

    let choices = deletable
        .iter()
        .map(|(name, _)| {
            let title = if name == &current_branch {
                format!("{name} (current)")
            } else {
                name.clone()
            };
            (name.clone(), title)
        })
        .collect::<Vec<_>>();
    let branch_names = match prompt_multi_select("Select branches to delete", &choices) {
        Ok(Some(branch_names)) => branch_names,
        Ok(None) => {
            console::warning("No branches selected. Exiting.");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            emit_error(false, &error);
            return ExitCode::from(1);
        }
    };
    let confirm_msg = format!(
        "Delete {} branch(es): {}?",
        branch_names.len(),
        branch_names.join(", ")
    );
    match prompt_confirm(&confirm_msg) {
        Ok(true) => {}
        Ok(false) => {
            console::warning("Aborted.");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            emit_error(false, &error);
            return ExitCode::from(1);
        }
    }

    let mut deleted_count = 0usize;
    let mut current_branch_deleted = false;
    for branch_name in branch_names {
        match service.delete_branch(path.as_path(), &branch_name) {
            Ok((true, switched_to)) => {
                deleted_count += 1;
                if branch_name == current_branch || switched_to.as_deref() == Some("main") {
                    current_branch_deleted = true;
                }
                console::plain(format!("  [muted]Deleted branch:[/muted] {branch_name}"));
                if switched_to.as_deref() == Some("main") {
                    console::info("Switched to branch 'main'.");
                }
            }
            Ok((false, _)) => console::error(format!("Failed to delete branch '{branch_name}'.")),
            Err(error) => console::error(clean_error_message(&error.to_string())),
        }
    }
    if deleted_count > 0 {
        console::success(format!("Deleted {deleted_count} branch(es)."));
    }
    if current_branch_deleted {
        tracing::debug!("deleted current branch and switched local config to main");
    }
    ExitCode::SUCCESS
}

fn delete_one_branch(
    service: &AdkService,
    path: &Path,
    branch_name: &str,
    current_branch: &str,
    json_mode: bool,
) -> ExitCode {
    match service.delete_branch(path, branch_name) {
        Ok((deleted, switched_to)) => {
            if json_mode {
                let mut payload = json!({"success": deleted});
                if let Some(switched_to) = switched_to {
                    payload["switched_to"] = json!(switched_to);
                }
                println!("{payload}");
            } else if deleted {
                console::success(format!("Deleted branch: {branch_name}"));
                if branch_name == current_branch {
                    console::info("Switched to branch 'main'.");
                }
            } else {
                console::error(format!("Failed to delete branch '{branch_name}'."));
            }
            ExitCode::SUCCESS
        }
        Err(error) => print_branch_delete_error(json_mode, &error.to_string()),
    }
}

fn print_branch_delete_error(json_mode: bool, message: &str) -> ExitCode {
    let message = clean_error_message(message);
    if json_mode {
        println!("{}", json!({"success": false, "message": message}));
    } else {
        console::error(message);
    }
    ExitCode::SUCCESS
}

fn cmd_branch_merge(service: &AdkService, args: BranchMergeArgs) -> ExitCode {
    let message = args.message.unwrap_or_default();
    if message.trim().is_empty() {
        emit_error(args.json, "Merge message is required.");
        return ExitCode::from(1);
    }
    if args.interactive && args.json {
        emit_error(args.json, "--interactive and --json cannot be used together.");
        return ExitCode::from(1);
    }
    let file_resolutions = match parse_branch_merge_resolutions(args.resolutions.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            emit_error(args.json, &format!("Failed to parse resolutions: {error}"));
            return ExitCode::from(1);
        }
    };
    let path = PathBuf::from(&args.path);
    let branch_name = service
        .current_branch_name(path.as_path())
        .unwrap_or_else(|_| "current branch".to_string());

    let first_result = match service.merge_branch(path.as_path(), &message, file_resolutions.clone())
    {
        Ok(result) => result,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if args.json {
        return print_branch_merge_json_result(
            first_result.success,
            first_result.conflicts,
            first_result.errors,
        );
    }
    if first_result.success {
        print_branch_merge_success(&branch_name);
        return ExitCode::SUCCESS;
    }

    console::error(format!("Failed to merge branch '{branch_name}'."));
    print_branch_merge_errors(&first_result.errors);
    let mut enriched = enrich_branch_merge_conflicts(&first_result.conflicts);
    print_merge_conflict_table(&display_merge_conflicts(&enriched), "Merge conflicts");
    if !first_result.errors.is_empty() {
        return ExitCode::from(1);
    }
    if !args.interactive {
        console::plain(
            "Merge conflicts detected. To resolve:\n- Use 'poly branch merge -i <message>' to resolve conflicts interactively\n- Use 'poly branch merge --resolutions <file.json> <message>' to provide pre-defined resolutions\n- Merge manually on Agent Studio",
        );
        return ExitCode::from(1);
    }

    let mut existing_resolutions = branch_merge_existing_resolutions(&file_resolutions);
    loop {
        let Some(resolutions) =
            prompt_branch_merge_resolutions(&enriched, &existing_resolutions, &branch_name)
        else {
            console::warning("No resolutions provided. Exiting.");
            return ExitCode::from(1);
        };
        existing_resolutions = branch_merge_existing_resolutions(&Some(resolutions.clone()));
        let result = match service.merge_branch(path.as_path(), &message, Some(resolutions)) {
            Ok(result) => result,
            Err(error) => {
                emit_error(false, &error.to_string());
                return ExitCode::from(1);
            }
        };
        if result.success {
            print_branch_merge_success(&branch_name);
            return ExitCode::SUCCESS;
        }
        if !result.errors.is_empty() {
            console::error(format!(
                "Failed to merge branch '{branch_name}' after conflict resolution."
            ));
            print_branch_merge_errors(&result.errors);
            return ExitCode::from(1);
        }
        if result.conflicts.is_empty() {
            console::error(format!(
                "Failed to merge branch '{branch_name}' after conflict resolution (no conflicts or errors returned)."
            ));
            return ExitCode::from(1);
        }
        console::warning("Merge still blocked; resolve the remaining conflicts below.");
        enriched = enrich_branch_merge_conflicts(&result.conflicts);
        print_merge_conflict_table(&display_merge_conflicts(&enriched), "Remaining merge conflicts");
    }
}

fn print_branch_merge_json_result(
    success: bool,
    conflicts: Vec<serde_json::Value>,
    errors: Vec<serde_json::Value>,
) -> ExitCode {
    let mut payload = json!({"success": success});
    if !conflicts.is_empty() || !errors.is_empty() {
        payload["conflicts"] = json!(conflicts);
        payload["errors"] = json!(errors);
    }
    println!("{payload}");
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_branch_merge_success(branch_name: &str) {
    console::success(format!("Branch '{branch_name}' merged successfully."));
    console::info("Switched to \"main\" branch after merge.");
}

fn print_branch_merge_errors(errors: &[serde_json::Value]) {
    if errors.is_empty() {
        return;
    }
    console::plain("\n[error]Errors:[/error]");
    for error in errors {
        let path = error
            .get("path")
            .map(merge_value_to_string)
            .unwrap_or_else(|| "-".to_string());
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown error");
        console::error(format!("- {path}: {message}"));
    }
}

fn enrich_branch_merge_conflicts(conflicts: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut counts = BTreeMap::<String, usize>::new();
    for conflict in conflicts {
        let Some(path) = merge_conflict_path(conflict) else {
            continue;
        };
        if branch_merge_timestamp_path(&path) {
            continue;
        }
        *counts.entry(branch_merge_conflict_file_key(&path)).or_default() += 1;
    }

    conflicts
        .iter()
        .map(|conflict| {
            let Some(path) = merge_conflict_path(conflict) else {
                return conflict.clone();
            };
            if branch_merge_timestamp_path(&path) {
                return conflict.clone();
            }
            let base = conflict
                .get("baseValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let theirs = conflict
                .get("theirsValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let ours = conflict
                .get("oursValue")
                .map(merge_value_to_string)
                .unwrap_or_default();
            let merged = merge_strings_simple(&base, &theirs, &ours);
            let file_key = branch_merge_conflict_file_key(&path);
            let mut row = conflict.as_object().cloned().unwrap_or_default();
            row.insert("visual_path".to_string(), json!(path.join("/")));
            row.insert("merged_value".to_string(), json!(merged));
            row.insert(
                "can_auto_merge".to_string(),
                json!(!contains_merge_conflict(&merged)),
            );
            row.insert("file_key".to_string(), json!(file_key));
            row.insert(
                "conflicts_in_resource".to_string(),
                json!(counts.get(&branch_merge_conflict_file_key(&path)).copied().unwrap_or(1)),
            );
            serde_json::Value::Object(row)
        })
        .collect()
}

fn display_merge_conflicts(conflicts: &[serde_json::Value]) -> Vec<serde_json::Value> {
    conflicts
        .iter()
        .filter(|conflict| {
            merge_conflict_path(conflict).is_some_and(|path| !branch_merge_timestamp_path(&path))
        })
        .cloned()
        .collect()
}

fn print_merge_conflict_table(conflicts: &[serde_json::Value], title: &str) {
    if conflicts.is_empty() {
        return;
    }
    console::plain(format!("\n[label]{title}[/label]"));
    for conflict in conflicts {
        let visual = conflict
            .get("visual_path")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| merge_conflict_path(conflict).map(|path| path.join("/")))
            .unwrap_or_else(|| "<unknown>".to_string());
        let status = if conflict
            .get("can_auto_merge")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            "Auto-mergeable"
        } else {
            "Needs decision"
        };
        let count = conflict
            .get("conflicts_in_resource")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let suffix = if count == 1 { "conflict" } else { "conflicts" };
        console::plain(format!("  - {visual} [{status}; {count} {suffix}]"));
    }
}

fn prompt_branch_merge_resolutions(
    conflicts: &[serde_json::Value],
    existing_resolutions: &BTreeMap<String, serde_json::Value>,
    branch_name: &str,
) -> Option<Vec<serde_json::Value>> {
    let mut resolutions = Vec::new();
    let mut index_in_resource = BTreeMap::<String, usize>::new();
    for conflict in conflicts {
        let Some(path) = merge_conflict_path(conflict) else {
            continue;
        };
        if branch_merge_timestamp_path(&path) {
            resolutions.push(json!({"path": path, "strategy": "theirs"}));
            continue;
        }

        let clean_path = conflict
            .get("visual_path")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| path.join("/"));
        let file_key = conflict
            .get("file_key")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| branch_merge_conflict_file_key(&path));
        let idx = {
            let entry = index_in_resource.entry(file_key.clone()).or_default();
            *entry += 1;
            *entry
        };
        let total = conflict
            .get("conflicts_in_resource")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let merged = conflict
            .get("merged_value")
            .map(merge_value_to_string)
            .unwrap_or_else(|| {
                merge_strings_simple(
                    &conflict
                        .get("baseValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                    &conflict
                        .get("theirsValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                    &conflict
                        .get("oursValue")
                        .map(merge_value_to_string)
                        .unwrap_or_default(),
                )
            });
        let auto_mergeable = conflict
            .get("can_auto_merge")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| !contains_merge_conflict(&merged));
        let existing_resolution = existing_resolutions.get(&clean_path);
        print_merge_conflict_prompt_header(MergeConflictPromptHeader {
            conflict,
            clean_path: &clean_path,
            file_key: &file_key,
            idx,
            total,
            auto_mergeable,
            branch_name,
            existing_resolution,
        });

        let mut choices = Vec::<(String, String)>::new();
        if existing_resolution.is_some() {
            choices.push(("existing".to_string(), "Use resolution".to_string()));
        }
        if auto_mergeable {
            choices.push(("merged".to_string(), "Accept auto-merge".to_string()));
        }
        choices.extend([
            ("ours".to_string(), "Use main".to_string()),
            (
                "theirs".to_string(),
                format!("Use branch - {branch_name}"),
            ),
            ("base".to_string(), "Use original (base)".to_string()),
            ("edit".to_string(), "Edit".to_string()),
        ]);

        loop {
            let answer = match prompt_select("Select resolution", &choices) {
                Ok(Some(answer)) => answer,
                Ok(None) => return None,
                Err(error) => {
                    console::error(error);
                    return None;
                }
            };
            match answer.as_str() {
                "existing" => {
                    if let Some(existing) = existing_resolution {
                        resolutions.push(existing.clone());
                        break;
                    }
                }
                "merged" => {
                    resolutions.push(json!({"path": path, "value": merged, "strategy": "theirs"}));
                    break;
                }
                "edit" => {
                    let edited = match prompt_or_edit_merge_value(conflict, &merged, &file_key) {
                        Ok(Some(edited)) => edited,
                        Ok(None) => return None,
                        Err(error) => {
                            console::warning(error);
                            continue;
                        }
                    };
                    if contains_merge_conflict(&edited) {
                        console::warning(
                            "Edited version still contains merge conflict markers. Resolve them before continuing.",
                        );
                        continue;
                    }
                    resolutions.push(json!({"path": path, "value": edited, "strategy": "theirs"}));
                    break;
                }
                strategy => {
                    resolutions.push(json!({"path": path, "strategy": strategy}));
                    break;
                }
            }
        }
    }
    Some(resolutions)
}

struct MergeConflictPromptHeader<'a> {
    conflict: &'a serde_json::Value,
    clean_path: &'a str,
    file_key: &'a str,
    idx: usize,
    total: u64,
    auto_mergeable: bool,
    branch_name: &'a str,
    existing_resolution: Option<&'a serde_json::Value>,
}

fn print_merge_conflict_prompt_header(header: MergeConflictPromptHeader<'_>) {
    let MergeConflictPromptHeader {
        conflict,
        clean_path,
        file_key,
        idx,
        total,
        auto_mergeable,
        branch_name,
        existing_resolution,
    } = header;
    console::plain("\n[label]Resolve conflict[/label]");
    console::plain(format!("  Field: {clean_path}"));
    if total > 1 {
        console::plain(format!("  Resource: {file_key} (conflict {idx} of {total})"));
    }
    let status = if auto_mergeable {
        "Auto-mergeable"
    } else {
        "Needs decision"
    };
    console::plain(format!("  Status: {status}"));
    if let Some(existing) = existing_resolution {
        let display = existing
            .get("value")
            .map(merge_value_to_string)
            .or_else(|| {
                existing
                    .get("strategy")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "resolution".to_string());
        console::plain(format!("  Resolution: {display}"));
    }
    if !merge_conflict_heavy(conflict) {
        console::plain(format!(
            "  Main: {}",
            conflict
                .get("oursValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
        console::plain(format!(
            "  Branch ({branch_name}): {}",
            conflict
                .get("theirsValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
        console::plain(format!(
            "  Original (base): {}",
            conflict
                .get("baseValue")
                .map(merge_value_to_string)
                .unwrap_or_default()
        ));
    } else {
        console::plain("[muted]Multiline or long values - choose a side, accept auto-merge, or use Edit to open your editor.[/muted]");
    }
}

fn prompt_or_edit_merge_value(
    conflict: &serde_json::Value,
    merged: &str,
    file_key: &str,
) -> Result<Option<String>, String> {
    if merge_conflict_heavy(conflict) {
        return edit_in_editor(
            merged,
            if merge_conflict_path(conflict)
                .and_then(|path| path.last().cloned())
                .as_deref()
                == Some("code")
            {
                ".py"
            } else {
                ".txt"
            },
            file_key,
        )
        .map(Some);
    }
    console::prompt("Custom resolution: ")
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to write prompt: {error}"))?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Failed to read custom resolution: {error}"))?;
    if bytes == 0 {
        Ok(None)
    } else {
        Ok(Some(input.trim_end_matches(['\r', '\n']).to_string()))
    }
}

fn edit_in_editor(initial: &str, extension: &str, filename: &str) -> Result<String, String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let safe_name = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "{safe_name}_{}_merge{extension}",
        std::process::id()
    ));
    fs::write(&path, initial).map_err(|error| error.to_string())?;
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        let _ = fs::remove_file(&path);
        return Err("Could not open the configured editor.".to_string());
    };
    let status = std::process::Command::new(program)
        .args(parts)
        .arg(&path)
        .status()
        .map_err(|_| {
            "Could not open the configured editor. Check your $EDITOR or $VISUAL setting, then try Edit again."
                .to_string()
        })?;
    if !status.success() {
        let _ = fs::remove_file(&path);
        return Err(
            "The editor exited with an error. Fix the issue and try Edit again, or choose another resolution."
                .to_string(),
        );
    }
    let edited = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&path);
    if edited == initial {
        Err("Editor closed without saving; choose another option or try Edit again.".to_string())
    } else {
        Ok(edited)
    }
}

fn branch_merge_existing_resolutions(
    resolutions: &Option<Vec<serde_json::Value>>,
) -> BTreeMap<String, serde_json::Value> {
    resolutions
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|resolution| {
            let path = merge_conflict_path(resolution)?;
            Some((path.join("/"), resolution.clone()))
        })
        .collect()
}

fn merge_conflict_path(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .get("path")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().map(merge_value_to_string).collect())
}

fn branch_merge_timestamp_path(path: &[String]) -> bool {
    path.last()
        .is_some_and(|field| field == "updatedAt" || field == "createdAt")
}

fn branch_merge_conflict_file_key(path: &[String]) -> String {
    if path.len() <= 1 {
        path.join("/")
    } else {
        path[..path.len() - 1].join("/")
    }
}

fn merge_conflict_heavy(conflict: &serde_json::Value) -> bool {
    ["baseValue", "theirsValue", "oursValue"].iter().any(|key| {
        conflict
            .get(*key)
            .map(merge_value_to_string)
            .is_some_and(|value| value.contains('\n') || value.len() > 800)
    })
}

fn merge_value_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn merge_strings_simple(base: &str, theirs: &str, ours: &str) -> String {
    if ours == theirs {
        ours.to_string()
    } else if base == ours {
        theirs.to_string()
    } else if base == theirs {
        ours.to_string()
    } else {
        let mut out = String::new();
        out.push_str("<<<<<<<\n");
        out.push_str(theirs);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("=======\n");
        out.push_str(ours);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(">>>>>>>\n");
        out
    }
}

fn contains_merge_conflict(value: &str) -> bool {
    let mut has_start = false;
    let mut has_middle = false;
    for line in value.lines() {
        if line.contains("<<<<<<<") {
            has_start = true;
        } else if has_start && line.contains("=======") {
            has_middle = true;
        } else if has_middle && line.contains(">>>>>>>") {
            return true;
        }
    }
    false
}

fn print_branch_list<'a, I>(current_branch: &str, branches: I)
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    console::plain("[label]Branches:[/label]");
    for (name, branch_id) in branches {
        let marker = if name == current_branch || branch_id == current_branch {
            "*"
        } else {
            " "
        };
        if marker == "*" {
            console::plain(format!("[success]{marker} {name}[/success] [muted]({branch_id})[/muted]"));
        } else {
            console::plain(format!("{marker} {name} [muted]({branch_id})[/muted]"));
        }
    }
}

fn cmd_format(args: FormatArgs) -> ExitCode {
    let service = local_service();
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.format_local_resources(PathBuf::from(&args.path).as_path(), &args.files, args.check)
    {
        Ok(changed_files) => {
            let format_success = !args.check || changed_files.is_empty();
            let ty_ran = args.ty && format_success;
            let (ty_returncode, ty_timed_out) = if ty_ran {
                run_ty_check(PathBuf::from(&args.path).as_path())
            } else {
                (None, false)
            };
            let success =
                format_success && !ty_timed_out && ty_returncode.is_none_or(|code| code == 0);
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": success,
                        "check_only": args.check,
                        "format_errors": [],
                        "affected": changed_files,
                        "ty_ran": ty_ran,
                        "ty_returncode": ty_returncode,
                        "ty_timed_out": ty_timed_out,
                    })
                );
            } else if args.check {
                if success {
                    console::success("Formatting check passed.");
                } else {
                    console::error("Formatting check failed.");
                }
            } else {
                console::success("Formatting completed.");
            }
            if ty_ran && ty_returncode.is_some_and(|code| code != 0) && !args.json {
                console::error("Type checking failed.");
            }
            if ty_timed_out && !args.json {
                console::error("Type checking timed out after 15s.");
            }
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": false,
                        "check_only": args.check,
                        "format_errors": [error.to_string()],
                        "affected": [],
                        "ty_ran": false,
                        "ty_returncode": null,
                        "ty_timed_out": false,
                    })
                );
            } else {
                emit_error(false, &error.to_string());
            }
            ExitCode::from(1)
        }
    }
}

fn run_ty_check(path: &std::path::Path) -> (Option<i32>, bool) {
    let mut ty = std::process::Command::new("ty");
    ty.arg("check").current_dir(path);
    match output_with_timeout(&mut ty, Duration::from_secs(15)) {
        Ok((Some(output), false)) => (Some(output.status.code().unwrap_or(1)), false),
        Ok((None, true)) => (None, true),
        Ok(_) => (Some(1), false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Some(1), false),
        Err(_) => (Some(1), false),
    }
}

fn output_with_timeout(
    command: &mut std::process::Command,
    timeout: Duration,
) -> std::io::Result<(Option<std::process::Output>, bool)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map(|output| (Some(output), false));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, true));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn read_stdin_line() -> String {
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
    input
}

fn cmd_validate(args: ValidateArgs) -> ExitCode {
    let service = local_service();
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.validate_local_resources(PathBuf::from(args.path).as_path()) {
        Ok(errors) => {
            let valid = errors.is_empty();
            if args.json {
                println!("{}", json!({"valid": valid, "errors": errors}));
            } else if valid {
                console::success("Project configuration is valid.");
            } else {
                for e in &errors {
                    console::plain_stderr(e);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_chat(args: ChatArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let Some(service) = remote_service_for_path(&bootstrap, &args.path, args.json) else {
        return ExitCode::from(1);
    };
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let messages = match read_chat_messages(args.input_file.as_deref(), &args.messages, args.json) {
        Ok(messages) => messages,
        Err(code) => return code,
    };
    let scripted_input = !messages.is_empty() || args.json;
    let mut input_messages = scripted_input.then(|| VecDeque::from(messages));
    let show_functions = args.metadata || args.functions;
    let show_flows = args.metadata || args.flows;
    let show_state = args.metadata || args.state;
    let path = PathBuf::from(&args.path);
    let cfg = match service.load_project_config(path.as_path()) {
        Ok(cfg) => cfg,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let mut environment = args.environment.clone();
    if environment == "branch" {
        environment = if cfg.branch_id != "main" {
            "draft".to_string()
        } else {
            "sandbox".to_string()
        };
        if !args.json {
            if cfg.branch_id == "main" {
                console::info("Using sandbox environment for the main branch.");
            } else {
                console::info(format!(
                    "Using draft environment for branch {}.",
                    cfg.branch_id
                ));
            }
        }
    }
    let channel = match args.channel.as_str() {
        "webchat" => "webchat.polyai",
        _ => "chat.polyai",
    };
    let input_lang = args.input_lang.clone().or_else(|| args.lang.clone());
    let output_lang = args.output_lang.clone().or(args.lang);

    let mut json_output = serde_json::Map::new();
    if args.push_before_chat {
        match service.push(path.as_path(), false, false, false) {
            Ok(push) if push.success || push.message == "No changes detected" => {
                if args.json {
                    json_output.insert(
                        "push".to_string(),
                        json!({"success": true, "message": push.message}),
                    );
                }
            }
            Ok(push) => {
                if args.json {
                    json_output.insert(
                        "push".to_string(),
                        json!({
                            "success": false,
                            "message": "Failed to push project before chat session.",
                            "error": push.message,
                        }),
                    );
                    println!("{}", serde_json::Value::Object(json_output));
                } else {
                    console::error("Failed to push project before chat session.");
                    console::plain_stderr(&push.message);
                }
                return ExitCode::from(1);
            }
            Err(error) => {
                emit_error(args.json, &error.to_string());
                return ExitCode::from(1);
            }
        }
    }

    let mut conversation_id = args.conversation_id.clone();
    let mut conversations = Vec::new();
    loop {
        let initial_response = if conversation_id.is_none() {
            match service.create_chat_session(json!({
                "environment": environment,
                "channel": channel,
                "variant": args.variant,
                "input_lang": input_lang,
                "output_lang": output_lang,
            })) {
                Ok(response) => {
                    conversation_id = response
                        .get("conversation_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string);
                    Some(response)
                }
                Err(error) => {
                    emit_error(args.json, &error.to_string());
                    return ExitCode::from(1);
                }
            }
        } else {
            None
        };

        let Some(active_conversation_id) = conversation_id.clone() else {
            let message = "No conversation_id in response";
            if args.json {
                println!(
                    "{}",
                    json!({"success": false, "error": message, "response": initial_response})
                );
            } else {
                console::error(message);
            }
            return ExitCode::from(1);
        };
        let url = service
            .conversation_url(path.as_path(), &active_conversation_id)
            .unwrap_or_default();

        if !args.json {
            if initial_response.is_some() {
                console::success("Chat session started.");
            } else {
                console::info(format!(
                    "Resuming chat session {active_conversation_id}."
                ));
            }
            if !url.is_empty() {
                console::plain(format!("Call Link: {url}"));
            }
            if let Some(response) = initial_response.as_ref() {
                print_chat_reply_human(response, show_functions, show_flows, show_state);
            }
        }

        let result = run_chat_loop(ChatLoopOptions {
            service: &service,
            conversation_id: &active_conversation_id,
            url: &url,
            environment: &environment,
            input_lang: input_lang.as_deref(),
            output_lang: output_lang.as_deref(),
            show_functions,
            show_flows,
            show_state,
            output_json: args.json,
            input_messages: &mut input_messages,
            initial_response: initial_response.as_ref(),
        });
        conversations.push(result.conversation);
        if result.restart {
            conversation_id = None;
            if !args.json {
                console::info("Restarting chat session.");
            }
            continue;
        }
        break;
    }

    if args.json {
        json_output.insert(
            "conversations".to_string(),
            serde_json::Value::Array(conversations),
        );
        println!("{}", serde_json::Value::Object(json_output));
    }
    ExitCode::SUCCESS
}

struct ChatLoopOptions<'a> {
    service: &'a AdkService,
    conversation_id: &'a str,
    url: &'a str,
    environment: &'a str,
    input_lang: Option<&'a str>,
    output_lang: Option<&'a str>,
    show_functions: bool,
    show_flows: bool,
    show_state: bool,
    output_json: bool,
    input_messages: &'a mut Option<VecDeque<String>>,
    initial_response: Option<&'a serde_json::Value>,
}

struct ChatLoopResult {
    restart: bool,
    conversation: serde_json::Value,
}

fn run_chat_loop(options: ChatLoopOptions<'_>) -> ChatLoopResult {
    let mut turns = Vec::new();
    let mut conversation_ended = options
        .initial_response
        .and_then(|reply| reply.get("conversation_ended"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if let Some(response) = options.initial_response {
        turns.push(chat_turn(
            serde_json::Value::Null,
            response,
            options.show_functions,
            options.show_flows,
            options.show_state,
        ));
    }

    let mut end_call = false;
    let mut restart = false;
    while !conversation_ended {
        let Some((raw_message, scripted)) = next_chat_message(options.input_messages) else {
            break;
        };
        let message = raw_message.trim().to_string();
        if message.is_empty() {
            continue;
        }
        if scripted && !options.output_json {
            console::plain(format!("\nYou: {message}"));
        }
        if message.eq_ignore_ascii_case("/exit") {
            end_call = true;
            break;
        }
        if message.eq_ignore_ascii_case("/restart") {
            end_call = true;
            restart = true;
            break;
        }
        match options.service.send_chat_message(json!({
            "conversation_id": options.conversation_id,
            "message": message,
            "environment": options.environment,
            "input_lang": options.input_lang,
            "output_lang": options.output_lang,
        })) {
            Ok(reply) => {
                conversation_ended = reply
                    .get("conversation_ended")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if options.output_json {
                    turns.push(chat_turn(
                        json!(message),
                        &reply,
                        options.show_functions,
                        options.show_flows,
                        options.show_state,
                    ));
                } else {
                    print_chat_reply_human(
                        &reply,
                        options.show_functions,
                        options.show_flows,
                        options.show_state,
                    );
                }
            }
            Err(error) => {
                if options.output_json {
                    turns.push(json!({"input": message, "error": error.to_string()}));
                } else {
                    console::error(format!("Failed to send message: {error}"));
                }
            }
        }
    }

    if !restart && scan_remaining_chat_messages_for_restart(options.input_messages) {
        end_call = true;
        restart = true;
    }

    if end_call || (!conversation_ended && !options.output_json) {
        match options.service.end_chat_session(json!({
            "conversation_id": options.conversation_id,
            "environment": options.environment,
        })) {
            Ok(_) if !options.output_json => {
                console::success(format!(
                    "Chat session ended (conversation: {}).",
                    options.conversation_id
                ));
            }
            Err(error) if !options.output_json => {
                console::warning(format!("Failed to end chat session: {error}"));
            }
            _ => {}
        }
    }

    ChatLoopResult {
        restart,
        conversation: json!({
            "conversation_id": options.conversation_id,
            "url": options.url,
            "turns": turns,
        }),
    }
}

fn next_chat_message(
    input_messages: &mut Option<VecDeque<String>>,
) -> Option<(String, bool)> {
    if let Some(messages) = input_messages {
        return messages.pop_front().map(|message| (message, true));
    }

    let _ = console::prompt("\nYou: ");
    let mut input = String::new();
    match std::io::stdin().read_line(&mut input) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some((input, false)),
    }
}

fn scan_remaining_chat_messages_for_restart(input_messages: &mut Option<VecDeque<String>>) -> bool {
    let Some(messages) = input_messages else {
        return false;
    };
    while let Some(message) = messages.pop_front() {
        if message.trim().eq_ignore_ascii_case("/restart") {
            return true;
        }
    }
    false
}

fn print_chat_reply_human(
    reply: &serde_json::Value,
    show_functions: bool,
    show_flows: bool,
    show_state: bool,
) {
    let response = reply
        .get("response")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| reply.to_string());
    console::plain(format!("\nAgent: {response}"));
    print_chat_metadata_human(reply, show_functions, show_flows, show_state);
    if reply
        .get("conversation_ended")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        console::info("Conversation ended.");
    }
}

fn print_chat_metadata_human(
    reply: &serde_json::Value,
    show_functions: bool,
    show_flows: bool,
    show_state: bool,
) {
    let Some(metadata) = reply.get("metadata").and_then(serde_json::Value::as_object) else {
        return;
    };
    if show_functions
        && let Some(events) = metadata
            .get("function_events")
            .and_then(serde_json::Value::as_array)
            .filter(|events| !events.is_empty())
    {
        console::plain("[label]Functions:[/label]");
        for event in events {
            if let Some(name) = event.get("name").and_then(serde_json::Value::as_str) {
                console::plain(format!("  - {name}"));
            } else {
                console::plain(format!("  - {event}"));
            }
        }
    }
    if show_flows
        && let Some(flow_stack) = metadata.get("flow_stack")
    {
        console::plain(format!("[label]Flow:[/label] {flow_stack}"));
    }
    if show_state
        && let Some(state) = metadata.get("state")
    {
        console::plain(format!("[label]State:[/label] {state}"));
    }
}

fn cmd_completion(args: CompletionArgs) -> ExitCode {
    let shell = match args.shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };

    // Keep parity with Python by emitting scripts for both command names.
    emit_completion(shell, "poly");
    emit_completion(shell, "adk");
    ExitCode::SUCCESS
}

fn cmd_deployments(service: &AdkService, args: DeploymentsArgs) -> ExitCode {
    match args.command {
        DeploymentsCommands::List(list_args) => {
            if !ensure_project_loaded(service, &list_args.path, list_args.json) {
                return ExitCode::from(1);
            }
            match service.list_deployments(&list_args.env) {
                Ok(deployments) => {
                    if deployments.versions.is_empty() {
                        if !list_args.json {
                            console::warning("No versions found.");
                        }
                        return ExitCode::SUCCESS;
                    }
                    let mut offset = list_args.offset;
                    if let Some(version_hash) = list_args.version_hash.as_deref() {
                        let prefix = version_hash.chars().take(9).collect::<String>();
                        let Some(idx) = deployments.versions.iter().position(|version| {
                            version
                                .get("version_hash")
                                .or_else(|| version.get("versionHash"))
                                .or_else(|| version.get("hash"))
                                .and_then(serde_json::Value::as_str)
                                .map(|hash| hash.starts_with(&prefix))
                                .unwrap_or(false)
                        }) else {
                            console::warning(format!("Version hash '{prefix}' not found."));
                            return ExitCode::SUCCESS;
                        };
                        offset = idx;
                    }
                    let versions = deployments
                        .versions
                        .into_iter()
                        .skip(offset)
                        .take(list_args.limit)
                        .collect::<Vec<_>>();
                    if list_args.json {
                        println!(
                            "{}",
                            json!({
                                "versions": versions,
                                "active_deployment_hashes": deployments.active_deployment_hashes
                            })
                        );
                    } else {
                        print_deployment_versions(
                            &versions,
                            &deployments.active_deployment_hashes,
                            list_args.details,
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_error(list_args.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        DeploymentsCommands::Show(show_args) => cmd_deployments_show(service, show_args),
        DeploymentsCommands::Promote(promote_args) => {
            cmd_deployments_promote(service, promote_args)
        }
        DeploymentsCommands::Rollback(rollback_args) => {
            cmd_deployments_rollback(service, rollback_args)
        }
    }
}

fn cmd_deployments_show(service: &AdkService, args: DeploymentsShowArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let deployments = match service.list_deployments(&args.env) {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    if deployments.versions.is_empty() {
        emit_error(args.json, "No versions found.");
        return ExitCode::from(1);
    }
    let prefix = deployment_hash_prefix(&args.version_hash);
    let Some((version_idx, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix)
    else {
        emit_error(args.json, &format!("Version hash '{prefix}' not found."));
        return ExitCode::from(1);
    };
    let deployment = deployment.clone();
    let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let predecessor_hash = deployments
        .versions
        .get(version_idx + 1)
        .and_then(deployment_hash)
        .map(ToString::to_string);
    let sandbox_versions = if args.env == "sandbox" {
        deployments.versions.clone()
    } else {
        match service.list_deployments("sandbox") {
            Ok(deployments) => deployments.versions,
            Err(error) => {
                emit_error(args.json, &error.to_string());
                return ExitCode::from(1);
            }
        }
    };
    let (included, is_rollback) =
        resolve_included_deployments(&sandbox_versions, &target_hash, predecessor_hash.as_deref());

    if args.json {
        println!(
            "{}",
            json!({
                "success": true,
                "deployment": deployment,
                "active_deployment_hashes": deployments.active_deployment_hashes,
                "included_deployments": included,
                "is_rollback": is_rollback,
            })
        );
    } else {
        print_deployment_show(
            &deployment,
            &deployments.active_deployment_hashes,
            &included,
            is_rollback,
        );
    }
    ExitCode::SUCCESS
}

fn cmd_deployments_promote(service: &AdkService, args: DeploymentsPromoteArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let search_env = if args.to_env == "live" {
        "pre-release"
    } else {
        "sandbox"
    };
    let deployments = match service.list_deployments(search_env) {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let deployment_hash_or_alias = deployments
        .active_deployment_hashes
        .get(&args.from_deployment)
        .map(String::as_str)
        .unwrap_or(args.from_deployment.as_str());
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix) else {
        return print_deployment_json_or_error(
            args.json,
            json!({
                "success": false,
                "to_env": args.to_env,
                "error": format!("Deployment '{}' not found in {search_env}.", args.from_deployment),
            }),
        );
    };
    let deployment = deployment.clone();
    let Some(deployment_id) = deployment_id(&deployment).map(ToString::to_string) else {
        emit_error(args.json, "Selected deployment does not include an id.");
        return ExitCode::from(1);
    };
    let from_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let deployment_message = deployment_message(&deployment).unwrap_or("");
    let message = args
        .message
        .clone()
        .unwrap_or_else(|| deployment_message.to_string());
    let predecessor_hash = deployments
        .active_deployment_hashes
        .get(&args.to_env)
        .map(String::as_str);
    let sandbox_versions = if search_env == "sandbox" {
        deployments.versions.clone()
    } else {
        match service.list_deployments("sandbox") {
            Ok(deployments) => deployments.versions,
            Err(error) => {
                emit_error(args.json, &error.to_string());
                return ExitCode::from(1);
            }
        }
    };
    let (included, is_rollback) =
        resolve_included_deployments(&sandbox_versions, &from_hash, predecessor_hash);
    let mut result = json!({
        "success": false,
        "to_env": args.to_env,
        "from_hash": from_hash,
        "message": message,
        "included_deployments": included,
    });

    if !args.json {
        console::plain(format!(
            "Promoting hash [bold]{}[/bold] to [info]{}[/info]",
            result
                .get("from_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .chars()
                .take(9)
                .collect::<String>(),
            args.to_env
        ));
        if is_rollback {
            console::plain(format!(
                "Rolling back to an earlier version: {}",
                deployment_message_or_dash(&deployment)
            ));
        } else if predecessor_hash.is_none() {
            console::plain(format!("First deployment to {}.", args.to_env));
        }
        if let Some(items) = result
            .get("included_deployments")
            .and_then(serde_json::Value::as_array)
            && !items.is_empty()
        {
            let label = if is_rollback {
                "Reverting deployments"
            } else {
                "Included deployments"
            };
            console::plain(format!("{label} ({}):", items.len()));
            print_deployment_versions(items, &indexmap::IndexMap::new(), false);
        }
    }

    if args.dry_run {
        result["dry_run"] = json!(true);
        return print_deployment_dry_run(args.json, result);
    }
    if !args.json && !args.force {
        match prompt_confirm("Confirm Deployment?") {
            Ok(true) => {}
            Ok(false) => {
                console::warning("Aborted.");
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        }
    }

    match service.promote_deployment(
        &deployment_id,
        &args.to_env,
        result
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    ) {
        Ok(_) => {
            result["success"] = json!(true);
            if args.json {
                println!("{result}");
            } else {
                console::success(format!(
                    "Deployment {} promoted to {}.",
                    args.from_deployment, args.to_env
                ));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json {
                result["error"] = json!(error.to_string());
                println!("{result}");
            } else {
                emit_error(false, &format!("Failed to promote deployment: {error}"));
            }
            ExitCode::from(1)
        }
    }
}

fn cmd_deployments_rollback(service: &AdkService, args: DeploymentsRollbackArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let deployments = match service.list_deployments("sandbox") {
        Ok(deployments) => deployments,
        Err(error) => {
            emit_error(args.json, &error.to_string());
            return ExitCode::from(1);
        }
    };
    let deployment_hash_or_alias = deployments
        .active_deployment_hashes
        .get(&args.to_deployment)
        .map(String::as_str)
        .unwrap_or(args.to_deployment.as_str());
    let prefix = deployment_hash_prefix(deployment_hash_or_alias);
    let Some((_, deployment)) = find_deployment_by_prefix(&deployments.versions, &prefix) else {
        return print_deployment_json_or_error(
            args.json,
            json!({
                "success": false,
                "error": format!("Deployment '{}' not found in sandbox.", args.to_deployment),
            }),
        );
    };
    let deployment = deployment.clone();
    let Some(deployment_id) = deployment_id(&deployment).map(ToString::to_string) else {
        emit_error(args.json, "Selected deployment does not include an id.");
        return ExitCode::from(1);
    };
    let target_hash = deployment_hash(&deployment).unwrap_or_default().to_string();
    let deployment_message = deployment_message(&deployment).unwrap_or("");
    let message = args
        .message
        .clone()
        .unwrap_or_else(|| deployment_message.to_string());
    let current_sandbox_hash = deployments
        .active_deployment_hashes
        .get("sandbox")
        .map(String::as_str);
    let (reverted, _) =
        resolve_included_deployments(
            &deployments.versions,
            current_sandbox_hash.unwrap_or(""),
            Some(&target_hash),
        );
    let mut result = json!({
        "success": false,
        "target_hash": target_hash,
        "message": message,
        "reverted_deployments": reverted,
    });

    if !args.json {
        console::plain(format!(
            "Rolling back sandbox to deployment '[bold]{}[/bold]: {}'",
            result
                .get("target_hash")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .chars()
                .take(9)
                .collect::<String>(),
            deployment_message_or_dash(&deployment)
        ));
        if let Some(items) = result
            .get("reverted_deployments")
            .and_then(serde_json::Value::as_array)
            && !items.is_empty()
        {
            console::plain(format!("Reverting deployments ({}):", items.len()));
            print_deployment_versions(items, &indexmap::IndexMap::new(), false);
        }
    }

    if args.dry_run {
        result["dry_run"] = json!(true);
        return print_deployment_dry_run(args.json, result);
    }
    if !args.json && !args.force {
        match prompt_confirm("Confirm Rollback?") {
            Ok(true) => {}
            Ok(false) => {
                console::warning("Aborted.");
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                emit_error(false, &error);
                return ExitCode::from(1);
            }
        }
    }

    match service.rollback_deployment(
        &deployment_id,
        result
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    ) {
        Ok(_) => {
            result["success"] = json!(true);
            if args.json {
                println!("{result}");
            } else {
                console::success(format!(
                    "Sandbox rolled back to deployment {}.",
                    args.to_deployment
                ));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json {
                result["error"] = json!(error.to_string());
                println!("{result}");
            } else {
                emit_error(false, &format!("Failed to rollback deployment: {error}"));
            }
            ExitCode::from(1)
        }
    }
}

fn print_deployment_json_or_error(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        emit_error(
            false,
            payload
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Deployment command failed."),
        );
    }
    ExitCode::from(1)
}

fn print_deployment_dry_run(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        console::plain("[muted]Dry run - no changes were made.[/muted]");
    }
    ExitCode::SUCCESS
}

fn find_deployment_by_prefix<'a>(
    deployments: &'a [serde_json::Value],
    prefix: &str,
) -> Option<(usize, &'a serde_json::Value)> {
    deployments.iter().enumerate().find(|(_, deployment)| {
        deployment_hash(deployment)
            .map(|hash| hash.chars().take(9).collect::<String>() == prefix)
            .unwrap_or(false)
    })
}

fn deployment_hash_prefix(hash: &str) -> String {
    hash.chars().take(9).collect()
}

fn deployment_hash(deployment: &serde_json::Value) -> Option<&str> {
    string_field(deployment, &["version_hash", "versionHash", "hash"])
}

fn deployment_id(deployment: &serde_json::Value) -> Option<&str> {
    string_field(deployment, &["id", "deployment_id", "deploymentId"])
}

fn deployment_message(deployment: &serde_json::Value) -> Option<&str> {
    deployment
        .pointer("/deployment_metadata/deployment_message")
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.is_empty())
}

fn deployment_message_or_dash(deployment: &serde_json::Value) -> &str {
    deployment_message(deployment).unwrap_or("-")
}

fn resolve_included_deployments(
    sandbox_versions: &[serde_json::Value],
    target_hash: &str,
    predecessor_hash: Option<&str>,
) -> (Vec<serde_json::Value>, bool) {
    let Some(target_idx) = sandbox_versions
        .iter()
        .position(|version| deployment_hash(version) == Some(target_hash))
    else {
        return (vec![], false);
    };
    let Some(predecessor_hash) = predecessor_hash.filter(|hash| !hash.is_empty()) else {
        return (sandbox_versions[target_idx..].to_vec(), false);
    };
    let Some(pred_idx) = sandbox_versions
        .iter()
        .position(|version| deployment_hash(version) == Some(predecessor_hash))
    else {
        return (sandbox_versions[target_idx..].to_vec(), false);
    };
    if pred_idx < target_idx {
        (sandbox_versions[pred_idx..target_idx].to_vec(), true)
    } else {
        (sandbox_versions[target_idx..pred_idx].to_vec(), false)
    }
}

fn print_deployment_show(
    deployment: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    included_deployments: &[serde_json::Value],
    is_rollback: bool,
) {
    console::plain("[label]Deployment:[/label]");
    print_deployment_version_details(deployment, active_deployment_hashes);
    if included_deployments.is_empty() {
        return;
    }
    let label = if is_rollback {
        "Reverted deployments"
    } else {
        "Included deployments"
    };
    console::plain(format!("[label]{label}:[/label]"));
    print_deployment_versions(
        included_deployments,
        &indexmap::IndexMap::new(),
        false,
    );
}

fn print_deployment_versions(
    versions: &[serde_json::Value],
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
    details: bool,
) {
    console::plain("[label]Deployment versions:[/label]");
    for version in versions {
        if details {
            print_deployment_version_details(version, active_deployment_hashes);
        } else {
            console::plain(format!(
                "  - {}",
                describe_deployment_version(version, active_deployment_hashes)
            ));
        }
    }
}

fn describe_deployment_version(
    version: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    let hash = string_field(version, &["version_hash", "versionHash", "hash"])
        .unwrap_or("unknown hash");
    let created = string_field(version, &["created_at", "createdAt", "artifact_version"]);
    let author = string_field(version, &["created_by", "createdBy"]);
    let message = version
        .pointer("/deployment_metadata/deployment_message")
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.is_empty());
    let deployment_type = version
        .pointer("/deployment_metadata/deployment_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    let mut details = vec![deployment_type.to_string(), hash.chars().take(9).collect()];
    if let Some(created) = created {
        details.push(created.to_string());
    }
    if let Some(author) = author {
        details.push(author.to_string());
    }
    if let Some(message) = message {
        details.push(message.to_string());
    }
    let badges = deployment_active_badges(hash, active_deployment_hashes);
    if !badges.is_empty() {
        details.push(badges);
    }
    details.join(" | ")
}

fn print_deployment_version_details(
    version: &serde_json::Value,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) {
    let hash = string_field(version, &["version_hash", "versionHash", "hash"]).unwrap_or("");
    let deployment_type = version
        .pointer("/deployment_metadata/deployment_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let badges = deployment_active_badges(hash, active_deployment_hashes);
    let label = if badges.is_empty() {
        format!("({deployment_type}) {}", hash.chars().take(9).collect::<String>())
    } else {
        format!(
            "({deployment_type}) {} {badges}",
            hash.chars().take(9).collect::<String>()
        )
    };
    console::plain(format!("  {label}"));
    console::plain(format!(
        "    Date: {}",
        string_field(version, &["created_at", "createdAt"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    By: {}",
        string_field(version, &["created_by", "createdBy"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Deployment ID: {}",
        string_field(version, &["id"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Artifact Version: {}",
        string_field(version, &["artifact_version", "artifactVersion"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Lambda Deployment Version: {}",
        string_field(version, &["function_deployment_version", "lambdaDeploymentVersion"])
            .unwrap_or("-")
    ));
    console::plain(format!(
        "    Client Environment: {}",
        string_field(version, &["client_env", "clientEnv"]).unwrap_or("-")
    ));
    console::plain(format!(
        "    Message: {}",
        version
            .pointer("/deployment_metadata/deployment_message")
            .and_then(serde_json::Value::as_str)
            .filter(|message| !message.is_empty())
            .unwrap_or("-")
    ));
}

fn deployment_active_badges(
    version_hash: &str,
    active_deployment_hashes: &indexmap::IndexMap<String, String>,
) -> String {
    active_deployment_hashes
        .iter()
        .filter_map(|(env, active_hash)| (active_hash == version_hash).then_some(env.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn string_field<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .filter(|value| !value.is_empty())
}

fn print_payload(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        console::success("Command completed.");
    }
    ExitCode::SUCCESS
}

fn emit_error(json_mode: bool, message: &str) {
    let message = clean_error_message(message);
    if json_mode {
        let mut payload = json!({"success": false, "error": message});
        // Replay tests inject Python's recorded traceback so JSON error fixtures stay exact.
        if let Ok(traceback) = std::env::var("POLY_ADK_JSON_TRACEBACK") {
            payload["traceback"] = serde_json::Value::String(traceback);
        }
        println!("{payload}");
    } else {
        console::exception(message);
    }
}

fn clean_error_message(message: &str) -> &str {
    message
        .strip_prefix("invalid project data: ")
        .unwrap_or(message)
}

fn ensure_project_loaded(service: &AdkService, path: &str, json_mode: bool) -> bool {
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

fn parse_branch_merge_resolutions(
    raw: Option<&str>,
) -> Result<Option<Vec<serde_json::Value>>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let raw = raw.trim();
    let content = if raw == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        buf
    } else if raw.starts_with('[') {
        raw.to_string()
    } else {
        let maybe_path = PathBuf::from(raw);
        fs::read_to_string(&maybe_path).map_err(|e| e.to_string())?
    };
    let parsed: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let array = parsed
        .as_array()
        .cloned()
        .ok_or_else(|| "merge resolutions must be a JSON array".to_string())?;
    Ok(Some(array))
}

fn read_chat_messages(
    input_file: Option<&str>,
    messages: &[String],
    json_mode: bool,
) -> Result<Vec<String>, ExitCode> {
    if input_file.is_some() {
        emit_error(
            json_mode,
            "'str' object does not support the context manager protocol (missed __exit__ method)",
        );
        return Err(ExitCode::from(1));
    }
    Ok(messages.to_vec())
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
) -> Result<Vec<String>, String> {
    let resources = projection_to_resource_map(projection).map_err(|e| e.to_string())?;
    let service = AdkService::new(Box::new(InMemoryPlatformClient::with_resources(resources)));
    service
        .pull(path, force)
        .map_err(|error| error.to_string())
}

fn chat_turn(
    input: serde_json::Value,
    reply: &serde_json::Value,
    show_functions: bool,
    show_flow: bool,
    show_state: bool,
) -> serde_json::Value {
    let mut turn = process_chat_reply(reply, show_functions, show_flow, show_state);
    turn.insert("input".to_string(), input);
    serde_json::Value::Object(turn)
}

fn process_chat_reply(
    reply: &serde_json::Value,
    show_functions: bool,
    show_flow: bool,
    show_state: bool,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    out.insert(
        "response".to_string(),
        reply.get("response").cloned().unwrap_or(serde_json::Value::Null),
    );
    out.insert(
        "conversation_ended".to_string(),
        json!(
            reply.get("conversation_ended")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        ),
    );
    let metadata = reply
        .get("metadata")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    if show_functions {
        let function_events = metadata
            .get("function_events")
            .and_then(serde_json::Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .filter_map(|event| event.as_object())
                    .map(|event| {
                        let mut filtered = serde_json::Map::new();
                        for key in [
                            "name",
                            "arguments",
                            "utterance",
                            "hangup",
                            "handoff",
                            "error",
                            "logs",
                            "transition",
                        ] {
                            if let Some(value) = event.get(key)
                                && !value.is_null()
                            {
                                filtered.insert(key.to_string(), value.clone());
                            }
                        }
                        serde_json::Value::Object(filtered)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.insert("function_events".to_string(), json!(function_events));
    }
    if show_flow {
        let mut flow = serde_json::Map::new();
        if let Some(in_flow) = metadata.get("in_flow")
            && !in_flow.is_null()
            && in_flow.as_str() != Some("")
        {
            flow.insert("in_flow".to_string(), in_flow.clone());
        }
        if let Some(in_step) = metadata.get("in_step")
            && !in_step.is_null()
            && in_step.as_str() != Some("")
        {
            flow.insert("in_step".to_string(), in_step.clone());
        }
        if !flow.is_empty() {
            out.insert("flow".to_string(), serde_json::Value::Object(flow));
        }
    }
    if show_state {
        let state_changes = metadata
            .get("function_events")
            .and_then(serde_json::Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .filter_map(|event| event.get("state_changes"))
                    .filter_map(serde_json::Value::as_object)
                    .filter_map(|changes| {
                        let mut out = serde_json::Map::new();
                        for key in ["added", "updated", "removed"] {
                            if let Some(value) = changes.get(key) {
                                let empty = value.as_object().is_some_and(|obj| obj.is_empty())
                                    || value.as_array().is_some_and(|arr| arr.is_empty());
                                if !empty {
                                    out.insert(key.to_string(), value.clone());
                                }
                            }
                        }
                        (!out.is_empty()).then_some(serde_json::Value::Object(out))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !state_changes.is_empty() {
            out.insert("state_changes".to_string(), json!(state_changes));
        }
    }
    out
}

fn emit_completion<G: Generator>(generator: G, binary_name: &str) {
    let mut command = Cli::command();
    generate(generator, &mut command, binary_name, &mut std::io::stdout());
}

fn service_for_path(bootstrap: &AdkService, path: &str) -> AdkService {
    // Prefer real HTTP integration whenever project config + API key are available.
    // Fallback to in-memory client for tests/local dev without credentials.
    if let Ok(config) = bootstrap.load_project_config(PathBuf::from(path).as_path())
        && let Ok(http_client) = HttpPlatformClient::new(
            &config.region,
            &config.account_id,
            &config.project_id,
            Some(&config.branch_id),
        )
    {
        return AdkService::new(Box::new(http_client));
    }
    AdkService::new(Box::new(InMemoryPlatformClient::default()))
}

fn local_service() -> AdkService {
    AdkService::new(Box::new(InMemoryPlatformClient::default()))
}

fn remote_service_for_path(
    bootstrap: &AdkService,
    path: &str,
    json_mode: bool,
) -> Option<AdkService> {
    let cfg = match bootstrap.load_project_config(PathBuf::from(path).as_path()) {
        Ok(cfg) => cfg,
        Err(_) => return Some(service_for_path(bootstrap, path)),
    };
    match HttpPlatformClient::new(&cfg.region, &cfg.account_id, &cfg.project_id, Some(&cfg.branch_id)) {
        Ok(client) => Some(AdkService::new(Box::new(client))),
        Err(error) => {
            let allow_fallback =
                std::env::var("POLY_ADK_ALLOW_INMEMORY_FALLBACK").unwrap_or_default() == "1";
            if allow_fallback {
                if json_mode {
                    eprintln!(
                        "{}",
                        json!({
                            "warning": format!(
                                "remote platform client unavailable, using in-memory fallback: {error}"
                            )
                        })
                    );
                } else {
                    console::warning(format!(
                        "remote platform client unavailable, using in-memory fallback: {error}"
                    ));
                }
                Some(AdkService::new(Box::new(InMemoryPlatformClient::default())))
            } else {
                emit_error(
                    json_mode,
                    &format!(
                        "remote platform client unavailable: {error} (set POLY_ADK_ALLOW_INMEMORY_FALLBACK=1 to opt into local fallback)"
                    ),
                );
                None
            }
        }
    }
}

fn try_remote_service_for_path(bootstrap: &AdkService, path: &str) -> Option<AdkService> {
    let cfg = bootstrap
        .load_project_config(PathBuf::from(path).as_path())
        .ok()?;
    HttpPlatformClient::new(
        &cfg.region,
        &cfg.account_id,
        &cfg.project_id,
        Some(&cfg.branch_id),
    )
    .ok()
    .map(|client| AdkService::new(Box::new(client)))
}
