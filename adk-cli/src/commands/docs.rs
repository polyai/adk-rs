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

const EMBEDDED_DOCS: &[(&str, &str)] = &[
    ("docs", include_str!("../../docs/docs.md")),
    (
        "agent_settings",
        include_str!("../../docs/agent_settings.md"),
    ),
    (
        "api_integrations",
        include_str!("../../docs/api_integrations.md"),
    ),
    ("chat_settings", include_str!("../../docs/chat_settings.md")),
    ("entities", include_str!("../../docs/entities.md")),
    (
        "experimental_config",
        include_str!("../../docs/experimental_config.md"),
    ),
    ("flows", include_str!("../../docs/flows.md")),
    ("functions", include_str!("../../docs/functions.md")),
    ("handoffs", include_str!("../../docs/handoffs.md")),
    (
        "response_control",
        include_str!("../../docs/response_control.md"),
    ),
    (
        "safety_filters",
        include_str!("../../docs/safety_filters.md"),
    ),
    ("sms", include_str!("../../docs/sms.md")),
    (
        "speech_recognition",
        include_str!("../../docs/speech_recognition.md"),
    ),
    ("topics", include_str!("../../docs/topics.md")),
    ("variables", include_str!("../../docs/variables.md")),
    ("variants", include_str!("../../docs/variants.md")),
    ("voice_settings", include_str!("../../docs/voice_settings.md")),
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

fn load_docs(document_name: &str) -> Result<&'static str, String> {
    for (name, content) in EMBEDDED_DOCS {
        if *name == document_name {
            return Ok(content);
        }
    }
    Err(format!("Documentation file {document_name}.md not found."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_docs_cover_root_and_all_choices() {
        assert!(load_docs("docs").expect("root docs").contains("# Poly ADK"));
        for choice in DOC_CHOICES {
            assert!(load_docs(choice).is_ok(), "missing embedded docs for {choice}");
        }
    }

    #[test]
    fn embedded_docs_report_unknown_documents() {
        assert_eq!(
            load_docs("not-a-real-doc").expect_err("unknown doc should fail"),
            "Documentation file not-a-real-doc.md not found."
        );
    }
}
