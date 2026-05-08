use adk_core::AdkService;
use adk_platform_api::{projection_to_resource_map, HttpPlatformClient, InMemoryPlatformClient};
use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

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
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(DOC_CHOICES))]
    documents: Vec<String>,
}

const DOC_CHOICES: &[&str] = &[
    "agent_settings",
    "api_integrations",
    "chat_settings",
    "entities",
    "experimental_config",
    "flows",
    "functions",
    "handoffs",
    "response_control",
    "safety_filters",
    "sms",
    "speech_recognition",
    "topics",
    "variables",
    "variants",
    "voice_settings",
];

#[derive(Debug, clap::Args)]
struct InitArgs {
    #[arg(long = "base-path", default_value = ".")]
    base_path: String,
    #[arg(long)]
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
    #[command(subcommand)]
    command: DeploymentsCommands,
}

#[derive(Debug, Subcommand)]
enum DeploymentsCommands {
    List(DeploymentsListArgs),
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
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
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
            let needs_remote = args.hash.is_some() || args.before.is_some() || args.after.is_some();
            if needs_remote {
                let Some(service) =
                    remote_service_for_path(&bootstrap_service, &args.path, args.json)
                else {
                    return Ok(ExitCode::from(1));
                };
                cmd_diff(&service, args)
            } else {
                cmd_diff(&local_service(), args)
            }
        }
        Commands::Review(args) => cmd_review(args),
        Commands::Branch(args) => cmd_branch(args),
        Commands::Format(args) => cmd_format(args),
        Commands::Validate(args) => cmd_validate(args),
        Commands::Chat(args) => cmd_chat(args),
        Commands::Completion(args) => cmd_completion(args),
        Commands::Deployments(args) => {
            let path = match &args.command {
                DeploymentsCommands::List(list) => list.path.as_str(),
            };
            let json_mode = matches!(&args.command, DeploymentsCommands::List(list) if list.json);
            let Some(service) = remote_service_for_path(&bootstrap_service, path, json_mode) else {
                return Ok(ExitCode::from(1));
            };
            cmd_deployments(&service, args)
        }
    };
    Ok(result)
}

fn cmd_docs(args: DocsArgs) -> ExitCode {
    let mut doc_names: Vec<&str> = Vec::new();
    if args.documents.is_empty() && !args.all {
        doc_names.push("docs");
    } else if args.all {
        doc_names.push("docs");
        doc_names.extend(DOC_CHOICES.iter().copied());
    } else {
        doc_names.extend(args.documents.iter().map(String::as_str));
    }

    let mut parts = Vec::new();
    for doc_name in doc_names {
        match load_docs(doc_name) {
            Ok(content) => parts.push(content),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
        }
    }
    let content = parts.join("\n\n");
    if let Some(output) = args.output {
        let output_arg = PathBuf::from(output);
        let output_path = if output_arg.is_absolute() {
            output_arg
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(output_arg)
        };
        if let Some(parent) = output_path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
        if let Err(error) = fs::write(&output_path, content) {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
        println!("Documentation written to {}", output_path.to_string_lossy());
        ExitCode::SUCCESS
    } else {
        println!("{content}");
        ExitCode::SUCCESS
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
    match (args.region, args.account_id, args.project_id) {
        (Some(region), Some(account_id), Some(project_id)) => {
            let base = PathBuf::from(args.base_path);
            match service.init_project(&base, region, account_id, project_id) {
                Ok(project) => {
                    let root_path = base.join(&project.account_id).join(&project.project_id);
                    if let Some(projection) = &projection_json
                        && let Err(error) = pull_projection_into_path(&root_path, projection, true)
                    {
                        emit_error(json_mode, &error);
                        return ExitCode::from(1);
                    }
                    if json_mode {
                        let mut payload = json!({"success": true, "root_path": root_path});
                        if args.output_json_projection {
                            payload["projection"] =
                                projection_json.clone().unwrap_or(serde_json::Value::Null);
                        }
                        println!(
                            "{}",
                            payload
                        );
                    } else {
                        println!("Project initialized.");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_error(json_mode, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            if json_mode {
                emit_error(
                    true,
                    "init with --json requires --region, --account_id, and --project_id.",
                );
                ExitCode::from(1)
            } else {
                eprintln!("Missing required interactive values for init.");
                ExitCode::from(1)
            }
        }
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
    let pull_result = if let Some(projection) = &projection_json {
        pull_projection_into_path(path.as_path(), projection, args.force)
    } else {
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
                    payload["projection"] =
                        projection_json.clone().unwrap_or(serde_json::Value::Null);
                }
                println!(
                    "{}",
                    payload
                );
            } else {
                println!("Pulled project.");
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
                println!(
                    "{}",
                    json!({
                        "success": push_result.success,
                        "message": push_result.message,
                        "dry_run": args.dry_run,
                        "commands": if args.output_json_commands { Some(push_result.commands) } else { None }
                    })
                );
            } else if push_result.success {
                println!("Push successful.");
            } else {
                eprintln!("{}", push_result.message);
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

fn cmd_status(service: &AdkService, args: StatusArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.status(PathBuf::from(args.path).as_path()) {
        Ok(summary) => {
            if args.json {
                println!(
                    "{}",
                    json!({
                        "files_with_conflicts": summary.files_with_conflicts,
                        "modified_files": summary.modified_files,
                        "new_files": summary.new_files,
                        "deleted_files": summary.deleted_files
                    })
                );
            } else {
                println!("{summary:#?}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
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
                println!("No changes to revert.");
            } else {
                println!("Changes reverted successfully.");
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
        eprintln!("Error: Cannot specify both hash and before/after versions.");
        return ExitCode::SUCCESS;
    }
    let named_diff = args.hash.is_some() || args.before.is_some() || args.after.is_some();
    let after = args.hash.or(args.after);
    match service.diff(
        PathBuf::from(args.path).as_path(),
        &args.files,
        args.before,
        after,
    ) {
        Ok(diffs) => {
            if diffs.is_empty() {
                if args.json {
                    println!(
                        "{}",
                        json!({"success": false, "message": "No changes detected"})
                    );
                } else {
                    println!("No changes detected.");
                }
                return ExitCode::SUCCESS;
            }
            if args.json {
                println!("{}", json!({"success": true, "diffs": diffs}));
            } else {
                for (path, diff) in diffs {
                    println!("=== {path} ===\n{diff}");
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

fn cmd_review(args: ReviewArgs) -> ExitCode {
    match args.command {
        None => ExitCode::SUCCESS,
        Some(ReviewCommands::List(list)) => match github_list_diff_gists() {
            Ok(gists) => {
                if args.json || list.json {
                    println!("{}", json!(gists));
                } else if gists.is_empty() {
                    println!("No review gists found.");
                } else if let Err(error) = prompt_open_review_gist(&gists) {
                    emit_review_message(false, &error);
                } else {
                    println!("Opened gist.");
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
                github_delete_review_gist(id).map(|deleted| usize::from(deleted))
            } else {
                prompt_delete_review_gists(json_mode)
            };
            match delete_result {
                Ok(deleted_count) => {
                    let deleted = deleted_count > 0;
                if args.json || delete.json {
                    println!("{}", json!({"success": deleted}));
                    } else if deleted_count > 1 {
                        println!("Deleted {deleted_count} gists.");
                } else if deleted {
                    println!("Deleted gist.");
                } else {
                        println!("No review gists found.");
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
                eprintln!("Error: Cannot specify both hash and before/after versions.");
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
            let diffs = match service.diff(
                PathBuf::from(&args.path).as_path(),
                &create.files,
                create.before.clone(),
                after.clone(),
            ) {
                Ok(diffs) => diffs,
                Err(_) => {
                    if json_mode {
                        println!("{}", json!({"success": false, "message": "Failed to compute diffs."}));
                    } else {
                        println!("No changes detected.");
                    }
                    return ExitCode::SUCCESS;
                }
            };
            if diffs.is_empty() {
                if json_mode {
                    println!("{}", json!({"success": false, "message": "No changes to review."}));
                } else {
                    println!("No changes detected.");
                }
                return ExitCode::SUCCESS;
            }
            let description = review_description(&args.path, create.hash.as_deref(), create.before.as_deref(), after.as_deref());
            match github_create_review_gist(diffs.iter(), &description) {
                Ok(url) => {
                    if json_mode {
                        println!("{}", json!({"success": true, "link": url}));
                    } else {
                        println!("Gist created: {url}");
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
                service.current_branch(PathBuf::from(&a.path).as_path()),
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
                        println!("{branches:#?}");
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
            let Some(branch_name) = a.branch_name.as_deref() else {
                emit_error(
                    a.json,
                    "branch create with --json requires a branch name argument.",
                );
                return ExitCode::from(1);
            };
            let path = PathBuf::from(&a.path);
            if matches!(a.environment.as_deref(), Some("pre-release" | "live")) {
                if !a.force {
                    match service.diff(path.as_path(), &[], None, None) {
                        Ok(diffs) if !diffs.is_empty() => {
                            emit_error(
                                a.json,
                                &format!(
                                    "Uncommitted changes on main branch, diffs: {:?}",
                                    diffs.keys().collect::<Vec<_>>()
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
                Ok(cfg) => print_payload(
                    a.json,
                    json!({"success": true, "branch_name": branch_name, "new_branch_id": cfg.branch_id}),
                ),
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Switch(a) => {
            let service = local_service();
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            let Some(branch_name) = a.branch_name.as_deref() else {
                emit_error(
                    a.json,
                    "branch switch with --json requires a branch name argument.",
                );
                return ExitCode::from(1);
            };
            let projection_json = match parse_optional_json_arg(a.from_projection.as_deref()) {
                Ok(value) => value,
                Err(error) => {
                    emit_error(a.json || a.output_json_projection, &error);
                    return ExitCode::from(1);
                }
            };
            match service.set_branch(PathBuf::from(&a.path).as_path(), branch_name) {
                Ok(cfg) => {
                    if let Some(projection) = &projection_json
                        && let Err(error) = pull_projection_into_path(
                            PathBuf::from(&a.path).as_path(),
                            projection,
                            a.force,
                        )
                    {
                        emit_error(a.json || a.output_json_projection, &error);
                        return ExitCode::from(1);
                    }
                    let mut payload = json!({"success": true, "branch_name": cfg.branch_id});
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
            let service = local_service();
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            match service.current_branch(PathBuf::from(&a.path).as_path()) {
                Ok(branch) => {
                    if a.json {
                        println!("{}", json!({"current_branch": branch}));
                    } else {
                        println!("Current branch: {branch}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
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
            let Some(branch_name) = a.branch_name.as_deref() else {
                emit_error(a.json, "missing required branch name for delete");
                return ExitCode::from(1);
            };
            match service.delete_branch(PathBuf::from(&a.path).as_path(), branch_name) {
                Ok(deleted) => {
                    let mut payload = json!({"success": deleted});
                    if deleted {
                        payload["switched_to"] = json!("main");
                    }
                    print_payload(a.json, payload)
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Merge(a) => {
            let Some(service) = remote_service_for_path(&bootstrap, &a.path, a.json) else {
                return ExitCode::from(1);
            };
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            let message = a.message.unwrap_or_default();
            if message.trim().is_empty() {
                emit_error(a.json, "Merge message is required.");
                return ExitCode::from(1);
            }
            if a.interactive && a.json {
                emit_error(a.json, "--interactive and --json cannot be used together.");
                return ExitCode::from(1);
            }
            let resolutions = match parse_branch_merge_resolutions(a.resolutions.as_deref()) {
                Ok(value) => value,
                Err(error) => {
                    emit_error(a.json, &format!("Failed to parse resolutions: {error}"));
                    return ExitCode::from(1);
                }
            };
            match service.merge_branch(PathBuf::from(&a.path).as_path(), &message, resolutions) {
                Ok(result) => {
                    let mut payload = json!({"success": result.success});
                    if !result.conflicts.is_empty() || !result.errors.is_empty() {
                        payload["conflicts"] = json!(result.conflicts);
                        payload["errors"] = json!(result.errors);
                    }
                    let code = print_payload(a.json, payload);
                    if result.success {
                        code
                    } else {
                        ExitCode::from(1)
                    }
                }
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
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
            let ty_returncode = if ty_ran {
                Some(run_ty_check(PathBuf::from(&args.path).as_path()))
            } else {
                None
            };
            let success = format_success && ty_returncode.is_none_or(|code| code == 0);
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
                        "ty_timed_out": false,
                    })
                );
            } else if args.check {
                if success {
                    println!("Formatting check passed.");
                } else {
                    eprintln!("Formatting check failed.");
                }
            } else {
                println!("Formatting completed.");
            }
            if ty_ran && ty_returncode.is_some_and(|code| code != 0) && !args.json {
                eprintln!("Type checking failed.");
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

fn run_ty_check(path: &std::path::Path) -> i32 {
    let status = std::process::Command::new("ty")
        .arg("check")
        .current_dir(path)
        .status()
        .or_else(|_| {
            std::process::Command::new("python3")
                .args(["-m", "ty", "check"])
                .current_dir(path)
                .status()
        });
    status.ok().and_then(|s| s.code()).unwrap_or(1)
}

fn cmd_validate(args: ValidateArgs) -> ExitCode {
    let service = local_service();
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.validate_local_resources(PathBuf::from(args.path).as_path()) {
        Ok(errors) => {
            if args.json {
                println!("{}", json!({"valid": errors.is_empty(), "errors": errors}));
            } else if errors.is_empty() {
                println!("Project configuration is valid.");
            } else {
                for e in &errors {
                    eprintln!("{e}");
                }
            }
            if args.json || errors.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
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
                    eprintln!("Failed to push project before chat session.");
                    eprintln!("{}", push.message);
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

    let Some(conversation_id) = conversation_id else {
        let message = "No conversation_id in response";
        if args.json {
            println!(
                "{}",
                json!({"success": false, "error": message, "response": initial_response})
            );
        } else {
            eprintln!("{message}");
        }
        return ExitCode::from(1);
    };
    let url = service
        .conversation_url(path.as_path(), &conversation_id)
        .unwrap_or_default();
    let mut turns = Vec::new();
    if let Some(response) = &initial_response {
        turns.push(chat_turn(
            serde_json::Value::Null,
            response,
            show_functions,
            show_flows,
            show_state,
        ));
    }

    let mut end_call = false;
    let mut conversation_ended = initial_response
        .as_ref()
        .and_then(|r| r.get("conversation_ended"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    for raw in messages {
        let message = raw.trim().to_string();
        if message.eq_ignore_ascii_case("/exit") {
            end_call = true;
            break;
        }
        if message.eq_ignore_ascii_case("/restart") {
            end_call = true;
            break;
        }
        match service.send_chat_message(json!({
            "conversation_id": conversation_id,
            "message": message,
            "environment": environment,
            "input_lang": input_lang,
            "output_lang": output_lang,
        })) {
            Ok(reply) => {
                conversation_ended = reply
                    .get("conversation_ended")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if args.json {
                    turns.push(chat_turn(
                        json!(message),
                        &reply,
                        show_functions,
                        show_flows,
                        show_state,
                    ));
                } else if let Some(text) = reply.get("response").and_then(serde_json::Value::as_str)
                {
                    println!("{text}");
                }
                if conversation_ended {
                    break;
                }
            }
            Err(error) => {
                if args.json {
                    turns.push(json!({"input": message, "error": error.to_string()}));
                } else {
                    eprintln!("Failed to send message: {error}");
                }
            }
        }
    }
    if end_call {
        let _ = service.end_chat_session(json!({
            "conversation_id": conversation_id,
            "environment": environment,
        }));
    }

    if args.json {
        json_output.insert(
            "conversations".to_string(),
            json!([{"conversation_id": conversation_id, "url": url, "turns": turns}]),
        );
        println!("{}", serde_json::Value::Object(json_output));
    } else if !conversation_ended && initial_response.is_some() {
        println!("{}", initial_response.unwrap());
    }
    ExitCode::SUCCESS
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
                            eprintln!("No versions found.");
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
                            eprintln!("Version hash '{prefix}' not found.");
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
                        println!("{versions:#?}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_error(list_args.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
    }
}

fn print_payload(json_mode: bool, payload: serde_json::Value) -> ExitCode {
    if json_mode {
        println!("{payload}");
    } else {
        println!("Command completed.");
    }
    ExitCode::SUCCESS
}

fn emit_error(json_mode: bool, message: &str) {
    if json_mode {
        println!("{}", json!({"success": false, "error": message}));
    } else {
        eprintln!("{message}");
    }
}

fn emit_review_message(json_mode: bool, message: &str) {
    if json_mode {
        println!("{}", json!({"success": false, "message": message}));
    } else {
        eprintln!("{message}");
    }
}

fn github_headers() -> Result<reqwest::header::HeaderMap, String> {
    let token = std::env::var("GITHUB_ACCESS_TOKEN").map_err(|_| {
        "GITHUB_ACCESS_TOKEN environment variable not set. Please set it to your GitHub personal access token with gist scope.".to_string()
    })?;
    let mut headers = reqwest::header::HeaderMap::new();
    let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| e.to_string())?;
    headers.insert(reqwest::header::AUTHORIZATION, auth_value);
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );
    Ok(headers)
}

fn github_client() -> Result<reqwest::blocking::Client, String> {
    let headers = github_headers()?;
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

fn github_list_diff_gists() -> Result<Vec<serde_json::Value>, String> {
    let client = github_client()?;
    let response = client
        .get("https://api.github.com/gists")
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    let gists: Vec<serde_json::Value> = response.json().map_err(|e| e.to_string())?;
    Ok(gists
        .into_iter()
        .filter(|gist| {
            gist.get("files")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|files| files.keys().all(|name| name.ends_with(".diff")))
        })
        .map(|gist| {
            json!({
                "id": gist.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "description": gist.get("description").cloned().unwrap_or_else(|| gist.get("id").cloned().unwrap_or(serde_json::Value::Null)),
                "created_at": gist.get("created_at").cloned().unwrap_or(serde_json::Value::Null),
                "html_url": gist.get("html_url").cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect())
}

fn github_create_review_gist<'a, I>(diffs: I, description: &str) -> Result<String, String>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let client = github_client()?;
    let files = diffs
        .into_iter()
        .filter(|(_, diff)| !diff.is_empty())
        .map(|(path, diff)| {
            (
                format!("{}.diff", path.replace(std::path::MAIN_SEPARATOR, "_")),
                json!({"content": diff}),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    let response = client
        .post("https://api.github.com/gists")
        .json(&json!({"description": description, "public": false, "files": files}))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    let payload: serde_json::Value = response.json().map_err(|e| e.to_string())?;
    payload
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "missing html_url in GitHub response".to_string())
}

fn prompt_open_review_gist(gists: &[serde_json::Value]) -> Result<(), String> {
    print_review_gist_choices(gists);
    print!("Select a gist to open: ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut selection = String::new();
    std::io::stdin()
        .read_line(&mut selection)
        .map_err(|e| e.to_string())?;
    let selection = selection.trim();
    if selection.is_empty() {
        return Ok(());
    }
    let gist = select_gist(gists, selection)?;
    let url = gist
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "selected gist did not include an html_url".to_string())?;
    open_url(url)
}

fn prompt_delete_review_gists(json_mode: bool) -> Result<usize, String> {
    let gists = github_list_diff_gists()?;
    if gists.is_empty() {
        return Ok(0);
    }
    if json_mode {
        return Err("review delete requires --id when --json is used".to_string());
    }
    print_review_gist_choices(&gists);
    print!("Select gists to delete (comma-separated numbers or id prefixes): ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut selection = String::new();
    std::io::stdin()
        .read_line(&mut selection)
        .map_err(|e| e.to_string())?;
    let selection = selection.trim();
    if selection.is_empty() {
        return Ok(0);
    }

    let mut ids = Vec::new();
    for token in selection
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        let gist = select_gist(&gists, token)?;
        let id = gist
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "selected gist did not include an id".to_string())?
            .to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    for id in &ids {
        github_delete_review_gist_by_id(id)?;
    }
    Ok(ids.len())
}

fn print_review_gist_choices(gists: &[serde_json::Value]) {
    for (idx, gist) in gists.iter().enumerate() {
        println!(
            "{}. {}  {}",
            idx + 1,
            gist.get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            gist.get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        );
    }
}

fn select_gist<'a>(
    gists: &'a [serde_json::Value],
    selection: &str,
) -> Result<&'a serde_json::Value, String> {
    if let Ok(index) = selection.parse::<usize>()
        && (1..=gists.len()).contains(&index)
    {
        return Ok(&gists[index - 1]);
    }
    gists
        .iter()
        .find(|gist| {
            gist.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id.starts_with(selection))
        })
        .ok_or_else(|| format!("No review gist found matching '{selection}'."))
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open browser: {status}"))
    }
}

fn github_delete_review_gist(gist_id: &str) -> Result<bool, String> {
    let gists = github_list_diff_gists()?;
    let id = select_gist(&gists, gist_id)?
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "matched gist did not include an id".to_string())?
        .to_string();
    github_delete_review_gist_by_id(&id)?;
    Ok(true)
}

fn github_delete_review_gist_by_id(id: &str) -> Result<(), String> {
    let client = github_client()?;
    let response = client
        .delete(format!("https://api.github.com/gists/{id}"))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(github_error_message(response));
    }
    Ok(())
}

fn github_error_message(response: reqwest::blocking::Response) -> String {
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return "Failed to make the gist. Your token must be missing gist permissions. Update and try again!".to_string();
    }
    let status = response.status();
    let body = response.text().unwrap_or_default();
    format!("GitHub API error: status={status} body={body}")
}

fn review_description(
    base_path: &str,
    version_hash: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) -> String {
    let path = PathBuf::from(base_path);
    let pieces = path
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let project_name = pieces.into_iter().rev().collect::<Vec<_>>().join("/");
    if let Some(hash) = version_hash {
        format!("Poly ADK: {project_name}: {hash}")
    } else if before.is_none() && after.is_none() {
        format!("Poly ADK: {project_name}: local -> remote")
    } else if let (Some(before), Some(after)) = (before, after) {
        format!("Poly ADK: {project_name}: {before} -> {after}")
    } else if let Some(after) = after {
        format!("Poly ADK: {project_name}: {after}")
    } else {
        format!("Poly ADK: {project_name}: {} -> local", before.unwrap_or(""))
    }
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
    if let Some(input_file) = input_file {
        let source = if input_file == "-" {
            let mut buf = String::new();
            if let Err(error) = std::io::stdin().read_to_string(&mut buf) {
                emit_error(json_mode, &error.to_string());
                return Err(ExitCode::from(1));
            }
            buf
        } else {
            match fs::read_to_string(input_file) {
                Ok(content) => content,
                Err(_) => {
                    emit_error(json_mode, &format!("Input file not found: {input_file}"));
                    return Err(ExitCode::from(1));
                }
            }
        };
        return Ok(source.lines().map(|line| line.to_string()).collect());
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
            .cloned()
            .unwrap_or_default();
        out.insert("function_events".to_string(), json!(function_events));
    }
    if show_flow {
        let mut flow = serde_json::Map::new();
        if let Some(in_flow) = metadata.get("in_flow") {
            flow.insert("in_flow".to_string(), in_flow.clone());
        }
        if let Some(in_step) = metadata.get("in_step") {
            flow.insert("in_step".to_string(), in_step.clone());
        }
        if !flow.is_empty() {
            out.insert("flow".to_string(), serde_json::Value::Object(flow));
        }
    }
    if show_state {
        let state_changes = metadata
            .get("state_changes")
            .cloned()
            .unwrap_or_else(|| json!([]));
        out.insert("state_changes".to_string(), state_changes);
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
                    eprintln!(
                        "Warning: remote platform client unavailable, using in-memory fallback: {error}"
                    );
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

fn load_docs(document_name: &str) -> Result<String, String> {
    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join(format!("{document_name}.md"));
    if !docs_path.exists() {
        return Err(format!("Documentation file {document_name}.md not found."));
    }
    fs::read_to_string(&docs_path).map_err(|e| e.to_string())
}
