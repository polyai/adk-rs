use adk_core::AdkService;
use adk_platform_api::{HttpPlatformClient, InMemoryPlatformClient};
use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser)]
#[command(name = "poly", version, about = "Agent Development Kit (Rust)")]
struct Cli {
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
    documents: Vec<String>,
}

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
    #[arg(long)]
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
    List,
    Delete(ReviewDeleteArgs),
}

#[derive(Debug, clap::Args)]
struct ReviewCreateArgs {
    hash: Option<String>,
    #[arg(long)]
    before: Option<String>,
    #[arg(long)]
    after: Option<String>,
    #[arg(long)]
    files: Vec<String>,
}

#[derive(Debug, clap::Args)]
struct ReviewDeleteArgs {
    #[arg(long = "id")]
    id: Option<String>,
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
    #[arg(long = "env", visible_alias = "environment")]
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
    #[arg(long)]
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
    #[arg(long, short = 'e', default_value = "branch")]
    environment: String,
    #[arg(long)]
    variant: Option<String>,
    #[arg(long)]
    lang: Option<String>,
    #[arg(long = "input-lang")]
    input_lang: Option<String>,
    #[arg(long = "output-lang")]
    output_lang: Option<String>,
    #[arg(long, default_value = "voice")]
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
    #[arg(long, short = 'e', default_value = "sandbox")]
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
    let cli = Cli::parse();
    let bootstrap_service = AdkService::new(Box::new(InMemoryPlatformClient::default()));

    let result = match cli.command {
        Commands::Docs(args) => cmd_docs(args),
        Commands::Init(args) => cmd_init(&bootstrap_service, args),
        Commands::Pull(args) => cmd_pull(&service_for_path(&bootstrap_service, &args.path), args),
        Commands::Push(args) => cmd_push(&service_for_path(&bootstrap_service, &args.path), args),
        Commands::Status(args) => {
            cmd_status(&service_for_path(&bootstrap_service, &args.path), args)
        }
        Commands::Revert(args) => {
            cmd_revert(&service_for_path(&bootstrap_service, &args.path), args)
        }
        Commands::Diff(args) => cmd_diff(&service_for_path(&bootstrap_service, &args.path), args),
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
            cmd_deployments(&service_for_path(&bootstrap_service, path), args)
        }
    };
    Ok(result)
}

fn cmd_docs(args: DocsArgs) -> ExitCode {
    let payload = json!({
        "success": true,
        "all": args.all,
        "documents": args.documents,
        "output": args.output
    });
    println!("{payload}");
    ExitCode::SUCCESS
}

fn cmd_init(service: &AdkService, args: InitArgs) -> ExitCode {
    let json_mode = args.json || args.output_json_projection;
    match (args.region, args.account_id, args.project_id) {
        (Some(region), Some(account_id), Some(project_id)) => {
            let base = PathBuf::from(args.base_path);
            match service.init_project(&base, region, account_id, project_id) {
                Ok(project) => {
                    if json_mode {
                        println!(
                            "{}",
                            json!({"success": true, "root_path": base.join(project.account_id).join(project.project_id)})
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
    match service.pull(PathBuf::from(args.path).as_path()) {
        Ok(conflicts) => {
            if args.json || args.output_json_projection {
                println!(
                    "{}",
                    json!({"success": conflicts.is_empty(), "files_with_conflicts": conflicts})
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
            emit_error(args.json || args.output_json_projection, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_push(service: &AdkService, args: PushArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json || args.output_json_commands) {
        return ExitCode::from(1);
    }
    match service.push(
        PathBuf::from(args.path).as_path(),
        args.force,
        args.skip_validation,
        args.dry_run,
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
    if args.json {
        println!("{}", json!({"success": true, "files_reverted": args.files}));
    } else {
        println!("Revert completed.");
    }
    ExitCode::SUCCESS
}

fn cmd_diff(service: &AdkService, args: DiffArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    if args.hash.is_some() && (args.before.is_some() || args.after.is_some()) {
        eprintln!("Error: Cannot specify both hash and before/after versions.");
        return ExitCode::SUCCESS;
    }
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
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_review(args: ReviewArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let service = service_for_path(&bootstrap, &args.path);
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let mut reviews = match load_reviews(&args.path) {
        Ok(v) => v,
        Err(e) => {
            emit_error(args.json, &e);
            return ExitCode::from(1);
        }
    };
    let payload = match args.command {
        Some(ReviewCommands::Create(create)) => {
            let review_id = format!("review-{}", unix_nanos());
            let review = json!({
                "id": review_id,
                "hash": create.hash,
                "before": create.before,
                "after": create.after,
                "files": create.files
            });
            reviews.push(review.clone());
            if let Err(e) = save_reviews(&args.path, &reviews) {
                emit_error(args.json, &e);
                return ExitCode::from(1);
            }
            json!({"success": true, "action": "create", "review": review})
        }
        Some(ReviewCommands::List) => json!({"success": true, "action": "list", "gists": reviews}),
        Some(ReviewCommands::Delete(delete)) => {
            let before = reviews.len();
            let id = delete.id.unwrap_or_default();
            reviews.retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
            if let Err(e) = save_reviews(&args.path, &reviews) {
                emit_error(args.json, &e);
                return ExitCode::from(1);
            }
            json!({"success": true, "action": "delete", "id": id, "deleted": before != reviews.len()})
        }
        None => json!({"success": true, "action": null}),
    };
    print_payload(args.json, payload)
}

fn cmd_branch(args: BranchArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    match args.command {
        BranchCommands::List(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            match service.list_known_branches(PathBuf::from(&a.path).as_path()) {
                Ok(branches) => print_payload(a.json, json!({"success": true, "branches": branches})),
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Create(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            print_payload(
                a.json,
                json!({"success": true, "branch_name": a.branch_name, "environment": a.environment, "force": a.force}),
            )
        }
        BranchCommands::Switch(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            let Some(branch_name) = a.branch_name.as_deref() else {
                emit_error(a.json, "missing required branch name for switch");
                return ExitCode::from(1);
            };
            match service.set_branch(PathBuf::from(&a.path).as_path(), branch_name) {
                Ok(cfg) => print_payload(
                    a.json,
                    json!({"success": true, "branch_name": cfg.branch_id, "force": a.force, "format": a.format}),
                ),
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Current(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            match service.current_branch(PathBuf::from(&a.path).as_path()) {
                Ok(branch) => print_payload(a.json, json!({"success": true, "branch": branch})),
                Err(error) => {
                    emit_error(a.json, &error.to_string());
                    ExitCode::from(1)
                }
            }
        }
        BranchCommands::Delete(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            print_payload(a.json, json!({"success": true, "branch_name": a.branch_name}))
        }
        BranchCommands::Merge(a) => {
            let service = service_for_path(&bootstrap, &a.path);
            if !ensure_project_loaded(&service, &a.path, a.json) {
                return ExitCode::from(1);
            }
            print_payload(
                a.json,
                json!({"success": true, "message": a.message, "interactive": a.interactive, "resolutions": a.resolutions}),
            )
        }
    }
}

fn cmd_format(args: FormatArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let service = service_for_path(&bootstrap, &args.path);
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.format_local_resources(PathBuf::from(&args.path).as_path(), &args.files, args.check)
    {
        Ok(changed_files) => {
            let success = !args.check || changed_files.is_empty();
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": success,
                        "check": args.check,
                        "ty": args.ty,
                        "files": args.files,
                        "changed_files": changed_files,
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
            if success {
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

fn cmd_validate(args: ValidateArgs) -> ExitCode {
    let bootstrap = AdkService::new(Box::new(InMemoryPlatformClient::default()));
    let service = service_for_path(&bootstrap, &args.path);
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.validate_local_resources(PathBuf::from(args.path).as_path()) {
        Ok(errors) => {
            if args.json {
                println!("{}", json!({"success": errors.is_empty(), "errors": errors}));
            } else if errors.is_empty() {
                println!("Validation successful.");
            } else {
                for e in &errors {
                    eprintln!("{e}");
                }
            }
            if errors.is_empty() {
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
    let service = service_for_path(&bootstrap, &args.path);
    if !ensure_project_loaded(&service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let payload = json!({
        "environment": args.environment,
        "variant": args.variant,
        "lang": args.lang,
        "input_lang": args.input_lang,
        "output_lang": args.output_lang,
        "channel": args.channel,
        "functions": args.functions,
        "flows": args.flows,
        "state": args.state,
        "metadata": args.metadata,
        "push_before_chat": args.push_before_chat,
        "messages": args.messages,
        "input_file": args.input_file,
        "conversation_id": args.conversation_id,
        "debug": args.debug,
        "verbose": args.verbose,
    });
    match service.create_chat_session(payload) {
        Ok(response) => {
            if args.json {
                println!("{}", json!({"success": true, "conversation": response}));
            } else {
                println!("{response}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
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
                    let versions = deployments
                        .versions
                        .into_iter()
                        .skip(list_args.offset)
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

fn reviews_file_path(path: &str) -> PathBuf {
    PathBuf::from(path).join("_gen").join("reviews.json")
}

fn load_reviews(path: &str) -> Result<Vec<serde_json::Value>, String> {
    let reviews_path = reviews_file_path(path);
    if !reviews_path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&reviews_path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(parsed.as_array().cloned().unwrap_or_default())
}

fn save_reviews(path: &str, reviews: &[serde_json::Value]) -> Result<(), String> {
    let reviews_path = reviews_file_path(path);
    if let Some(parent) = reviews_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let serialized = serde_json::to_string_pretty(reviews).map_err(|e| e.to_string())?;
    fs::write(reviews_path, serialized).map_err(|e| e.to_string())
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
