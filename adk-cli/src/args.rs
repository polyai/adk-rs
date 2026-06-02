use clap::{ArgAction, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "poly",
    version,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
pub(crate) struct Cli {
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue, help = "show the version and exit")]
    pub(crate) version: bool,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    #[command(about = "Show this help message and exit.")]
    Help,
    #[command(about = "Outputs documentation for a given topic.")]
    Docs(DocsArgs),
    #[command(about = "Sign in and save an Agent Studio API key.")]
    Login(LoginArgs),
    #[command(about = "Start using the ADK with guided account and project setup.")]
    Start(StartArgs),
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
    #[command(about = "Incomplete: review Agent Studio project changes.")]
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
    #[command(
        about = "EXPERIMENTAL: Uninstall shell-installed ADK using installation receipt file."
    )]
    Uninstall(UninstallArgs),
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
    #[command(about = "Manage deployments for the project.")]
    Deployments(DeploymentsArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct DocsArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) all: bool,
    #[arg(long, visible_alias = "write", short = 'o')]
    pub(crate) output: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(crate::commands::docs::DOC_CHOICES))]
    pub(crate) documents: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub(crate) struct LoginArgs {
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(crate::commands::init::INIT_REGIONS))]
    pub(crate) region: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct StartArgs {
    #[arg(long = "base-path", default_value = ".")]
    pub(crate) base_path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct InitArgs {
    #[arg(long = "base-path", default_value = ".")]
    pub(crate) base_path: String,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["us-1", "euw-1", "uk-1", "studio", "staging", "dev"]))]
    pub(crate) region: Option<String>,
    #[arg(long = "account_id")]
    pub(crate) account_id: Option<String>,
    #[arg(long = "project_id")]
    pub(crate) project_id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) format: bool,
    #[arg(long = "from-projection", hide = true)]
    pub(crate) from_projection: Option<String>,
    #[arg(long = "output-json-projection", hide = true, action = ArgAction::SetTrue)]
    pub(crate) output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ProjectArgs {
    #[command(subcommand)]
    pub(crate) command: ProjectCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectCommands {
    #[command(about = "Create a new Agent Studio project under an account.")]
    Create(ProjectCreateArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct ProjectCreateArgs {
    #[arg(long = "base-path", default_value = ".")]
    pub(crate) base_path: String,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(crate::commands::init::INIT_REGIONS))]
    pub(crate) region: Option<String>,
    #[arg(long = "account_id")]
    pub(crate) account_id: Option<String>,
    #[arg(long = "name")]
    pub(crate) project_name: Option<String>,
    #[arg(long = "id", visible_alias = "project_id")]
    pub(crate) project_id: Option<String>,
    #[arg(long, default_value = "Hello, how can I help you?")]
    pub(crate) greeting: String,
    #[arg(long = "voice-id")]
    pub(crate) voice_id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct PullArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) format: bool,
    #[arg(long = "from-projection", hide = true)]
    pub(crate) from_projection: Option<String>,
    #[arg(long = "output-json-projection", hide = true, action = ArgAction::SetTrue)]
    pub(crate) output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct PushArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long = "skip-validation", action = ArgAction::SetTrue)]
    pub(crate) skip_validation: bool,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    pub(crate) dry_run: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) format: bool,
    #[arg(long)]
    pub(crate) email: Option<String>,
    #[arg(long = "from-projection", hide = true)]
    pub(crate) from_projection: Option<String>,
    #[arg(long = "output-json-commands", hide = true, action = ArgAction::SetTrue)]
    pub(crate) output_json_commands: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct StatusArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct RevertArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct DiffArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) hash: Option<String>,
    #[arg(long, num_args = 0..)]
    pub(crate) files: Vec<String>,
    #[arg(long)]
    pub(crate) before: Option<String>,
    #[arg(long)]
    pub(crate) after: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ReviewArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
    #[command(subcommand)]
    pub(crate) command: Option<ReviewCommands>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCommands {
    Create(ReviewCreateArgs),
    List(ReviewListArgs),
    Delete(ReviewDeleteArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct ReviewCreateArgs {
    pub(crate) hash: Option<String>,
    #[arg(long)]
    pub(crate) before: Option<String>,
    #[arg(long)]
    pub(crate) after: Option<String>,
    #[arg(long, num_args = 0..)]
    pub(crate) files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ReviewListArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ReviewDeleteArgs {
    #[arg(long = "id")]
    pub(crate) id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct BranchArgs {
    #[command(subcommand)]
    pub(crate) command: BranchCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BranchCommands {
    List(CommonPathArgs),
    Create(BranchCreateArgs),
    Switch(BranchSwitchArgs),
    Current(CommonPathArgs),
    Delete(BranchDeleteArgs),
    Merge(BranchMergeArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct CommonPathArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct BranchCreateArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) branch_name: Option<String>,
    #[arg(
        long = "env",
        visible_alias = "environment",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    pub(crate) environment: Option<String>,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct BranchSwitchArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) branch_name: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) format: bool,
    #[arg(long, short = 'f', action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long = "from-projection", hide = true)]
    pub(crate) from_projection: Option<String>,
    #[arg(long = "output-json-projection", action = ArgAction::SetTrue)]
    pub(crate) output_json_projection: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct BranchDeleteArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) branch_name: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct BranchMergeArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    pub(crate) message: Option<String>,
    #[arg(long, short = 'i', action = ArgAction::SetTrue)]
    pub(crate) interactive: bool,
    #[arg(long)]
    pub(crate) resolutions: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct FormatArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, num_args = 0..)]
    pub(crate) files: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) check: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) ty: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ValidateArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct ChatArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "branch",
        value_parser = clap::builder::PossibleValuesParser::new(["branch", "sandbox", "pre-release", "live"])
    )]
    pub(crate) environment: String,
    #[arg(long)]
    pub(crate) variant: Option<String>,
    #[arg(long)]
    pub(crate) lang: Option<String>,
    #[arg(long = "input-lang")]
    pub(crate) input_lang: Option<String>,
    #[arg(long = "output-lang")]
    pub(crate) output_lang: Option<String>,
    #[arg(
        long,
        default_value = "voice",
        value_parser = clap::builder::PossibleValuesParser::new(["voice", "webchat"])
    )]
    pub(crate) channel: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) functions: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) flows: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) state: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) metadata: bool,
    #[arg(long = "push", action = ArgAction::SetTrue)]
    pub(crate) push_before_chat: bool,
    #[arg(long = "message", short = 'm')]
    pub(crate) messages: Vec<String>,
    #[arg(long = "input-file")]
    pub(crate) input_file: Option<String>,
    #[arg(long = "conversation-id", visible_alias = "conv-id")]
    pub(crate) conversation_id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub(crate) enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, clap::Args)]
pub(crate) struct CompletionArgs {
    pub(crate) shell: CompletionShell,
}

#[derive(Debug, clap::Args)]
pub(crate) struct SelfUpdateArgs {
    #[arg(long, action = ArgAction::SetTrue, help = "show detailed update errors")]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct UninstallArgs {
    #[arg(long, action = ArgAction::SetTrue, help = "uninstall without prompting for confirmation")]
    pub(crate) yes: bool,
    #[arg(long, action = ArgAction::SetTrue, help = "show detailed uninstall errors")]
    pub(crate) verbose: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct DeploymentsArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
    #[command(subcommand)]
    pub(crate) command: DeploymentsCommands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DeploymentsCommands {
    List(DeploymentsListArgs),
    Show(DeploymentsShowArgs),
    Promote(DeploymentsPromoteArgs),
    Rollback(DeploymentsRollbackArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct DeploymentsListArgs {
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "sandbox",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    pub(crate) env: String,
    #[arg(long, default_value_t = 10)]
    pub(crate) limit: usize,
    #[arg(long, default_value_t = 0)]
    pub(crate) offset: usize,
    #[arg(long = "hash")]
    pub(crate) version_hash: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) details: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct DeploymentsShowArgs {
    pub(crate) version_hash: String,
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(
        long,
        short = 'e',
        default_value = "sandbox",
        value_parser = clap::builder::PossibleValuesParser::new(["sandbox", "pre-release", "live"])
    )]
    pub(crate) env: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct DeploymentsPromoteArgs {
    #[arg(long = "from")]
    pub(crate) from_deployment: String,
    #[arg(
        long = "to",
        value_parser = clap::builder::PossibleValuesParser::new(["pre-release", "live"])
    )]
    pub(crate) to_env: String,
    #[arg(long, short = 'm')]
    pub(crate) message: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) dry_run: bool,
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
}

#[derive(Debug, clap::Args)]
pub(crate) struct DeploymentsRollbackArgs {
    #[arg(long = "to")]
    pub(crate) to_deployment: String,
    #[arg(long, short = 'm')]
    pub(crate) message: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) force: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) dry_run: bool,
    #[arg(long, default_value = ".")]
    pub(crate) path: String,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) json: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) verbose: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) debug: bool,
}
