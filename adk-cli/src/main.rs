use adk_api_client::{AccountSummary, HttpPlatformClient, InMemoryPlatformClient, PlatformClient};
use adk_core::{AdkService, ProjectWorkspace};
use adk_push_pull::projection_to_resource_map;
use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
mod review;
mod self_update;
mod status;
mod validate;

use branch::cmd_branch;
use chat::cmd_chat;
use completion::cmd_completion;
use deployments::cmd_deployments;
use diff::cmd_diff;
use docs::cmd_docs;
use format::cmd_format;
use init::{INIT_REGIONS, cmd_init};
pub(crate) use output::{clean_error_message, emit_error, print_payload};
use project::{cmd_project, project_debug, project_verbose};
use pull::cmd_pull;
use push::cmd_push;
use review::{
    emit_review_message, github_create_review_gist, github_delete_review_gist,
    github_list_diff_gists, prompt_delete_review_gists, prompt_open_review_gist, review_description,
};
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

#[derive(Debug, Parser)]
#[command(
    name = "poly",
    version,
    disable_help_subcommand = true,
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
    #[command(about = "Outputs documentation for a given topic.")]
    Docs(DocsArgs),
    #[command(about = "Initialize a new Agent Studio project.")]
    Init(InitArgs),
    #[command(about = "Manage Agent Studio projects.")]
    Project(ProjectArgs),
    #[command(about = "Pull the latest project configuration from Agent Studio.")]
    Pull(PullArgs),
    #[command(about = "Push the project configuration to Agent Studio.")]
    Push(PushArgs),
    #[command(about = "Check the changed files of the project.")]
    Status(StatusArgs),
    #[command(about = "Revert changes in the project.")]
    Revert(RevertArgs),
    #[command(about = "Show the changes made to the project.")]
    Diff(DiffArgs),
    #[command(about = "Create a GitHub Gist of Agent Studio project changes to share changes.")]
    Review(ReviewArgs),
    #[command(about = "Manage branches in the Agent Studio project.")]
    Branch(BranchArgs),
    #[command(about = "Run ruff and YAML/JSON formatting on the project (optional ty with --ty).")]
    Format(FormatArgs),
    #[command(about = "Validate the project configuration locally.")]
    Validate(ValidateArgs),
    #[command(about = "Start an interactive chat session with the agent.")]
    Chat(ChatArgs),
    #[command(about = "Update the ADK CLI installed by the release shell installer.")]
    SelfUpdate(SelfUpdateArgs),
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
    #[command(about = "Manage deployments for the project.")]
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
struct ProjectArgs {
    #[command(subcommand)]
    command: ProjectCommands,
}

#[derive(Debug, Subcommand)]
enum ProjectCommands {
    #[command(about = "Create a new Agent Studio project under an account.")]
    Create(ProjectCreateArgs),
}

#[derive(Debug, clap::Args)]
struct ProjectCreateArgs {
    #[arg(long = "base-path", default_value = ".")]
    base_path: String,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(INIT_REGIONS))]
    region: Option<String>,
    #[arg(long = "account_id")]
    account_id: Option<String>,
    #[arg(long = "name")]
    project_name: Option<String>,
    #[arg(long = "id", visible_alias = "project_id")]
    project_id: Option<String>,
    #[arg(long, default_value = "Hello, how can I help you?")]
    greeting: String,
    #[arg(long = "voice-id")]
    voice_id: Option<String>,
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
struct SelfUpdateArgs {
    #[arg(long, action = ArgAction::SetTrue, help = "show detailed update errors")]
    verbose: bool,
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
            "            {docs,init,project,pull,push,status,revert,diff,review,branch,format,validate,chat,self-update,completion,deployments} ...\n\n",
            "positional arguments:\n",
            "  {docs,init,project,pull,push,status,revert,diff,review,branch,format,validate,chat,self-update,completion,deployments}\n",
        ),
    );
    for (name, description) in [
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
        (
            "review",
            "Create a GitHub Gist of Agent Studio project changes to share changes.",
        ),
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

fn cmd_revert<C: PlatformClient>(service: &AdkService<C>, args: RevertArgs) -> ExitCode {
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
            if needs_remote {
                let workspace = ProjectWorkspace::new();
                with_remote_service!(&workspace, &args.path, json_mode, |service| {
                    cmd_review_create(&service, &args.path, args.json, create)
                })
            } else {
                let service = local_service();
                cmd_review_create(&service, &args.path, args.json, create)
            }
        }
    }
}

fn cmd_review_create<C: PlatformClient>(
    service: &AdkService<C>,
    path: &str,
    parent_json: bool,
    create: ReviewCreateArgs,
) -> ExitCode {
    let json_mode = parent_json || create.json;
    if !ensure_project_loaded(service, path, json_mode) {
        return ExitCode::from(1);
    }
    let after = create.hash.clone().or(create.after.clone());
    let root = PathBuf::from(path);
    let files = normalize_cli_file_args(root.as_path(), &create.files);
    let diffs = match service.diff(root.as_path(), &files, create.before.clone(), after.clone()) {
        Ok(diffs) => diffs,
        Err(_) => {
            if json_mode {
                println!(
                    "{}",
                    json!({"success": false, "message": "Failed to compute diffs."})
                );
            } else {
                console::plain("[muted]No changes detected.[/muted]");
            }
            return ExitCode::SUCCESS;
        }
    };
    if diffs.is_empty() {
        if json_mode {
            println!(
                "{}",
                json!({"success": false, "message": "No changes to review."})
            );
        } else {
            console::plain("[muted]No changes detected.[/muted]");
        }
        return ExitCode::SUCCESS;
    }
    let description = review_description(
        path,
        create.hash.as_deref(),
        create.before.as_deref(),
        after.as_deref(),
    );
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
