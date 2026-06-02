use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::{DocsArgs, console};

pub(crate) const DOC_CHOICES: &[&str] = &[
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

pub(crate) fn cmd_docs(args: DocsArgs) -> ExitCode {
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
                console::error(error);
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
            console::error(error.to_string());
            return ExitCode::from(1);
        }
        if let Err(error) = fs::write(&output_path, content) {
            console::error(error.to_string());
            return ExitCode::from(1);
        }
        console::success(format!(
            "Documentation written to {}",
            output_path.to_string_lossy()
        ));
        ExitCode::SUCCESS
    } else {
        println!("{content}");
        ExitCode::SUCCESS
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
