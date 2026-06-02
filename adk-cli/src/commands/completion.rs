use crate::{Cli, CompletionArgs, CompletionShell};
use clap::CommandFactory;
use clap_complete::{Generator, Shell, generate};
use std::process::ExitCode;

pub(crate) fn cmd_completion(args: CompletionArgs) -> ExitCode {
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

fn emit_completion<G: Generator>(generator: G, binary_name: &str) {
    let mut command = Cli::command();
    generate(generator, &mut command, binary_name, &mut std::io::stdout());
}
