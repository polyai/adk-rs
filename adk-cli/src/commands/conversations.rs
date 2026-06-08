use crate::{
    AdkService, ConversationsArgs, ConversationsCommands, ConversationsGetArgs,
    ConversationsGetAudioArgs, ConversationsListArgs, console, emit_error, ensure_project_loaded,
};
use adk_api_client::PlatformClient;
use adk_types::{
    ConversationDetail, ConversationShortSummary, ConversationSummary, ConversationTurn,
};
use chrono::{DateTime, Local};
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(crate) fn cmd_conversations<C: PlatformClient>(
    service: &AdkService<C>,
    args: ConversationsArgs,
) -> ExitCode {
    match args.command {
        ConversationsCommands::List(args) => cmd_conversations_list(service, args),
        ConversationsCommands::Get(args) => cmd_conversations_get(service, args),
        ConversationsCommands::GetAudio(args) => cmd_conversations_get_audio(service, args),
    }
}

fn cmd_conversations_list<C: PlatformClient>(
    service: &AdkService<C>,
    args: ConversationsListArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.list_conversations(args.limit, args.offset) {
        Ok(payload) => {
            if args.json {
                print_json(&payload);
                return ExitCode::SUCCESS;
            }
            if payload.conversations.is_empty() {
                console::info("No conversations found.");
            } else {
                print_conversations(service, Path::new(&args.path), &payload.conversations);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn cmd_conversations_get<C: PlatformClient>(
    service: &AdkService<C>,
    args: ConversationsGetArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    match service.get_conversation(&args.conversation_id) {
        Ok(conversation) => {
            if args.json {
                print_json(&conversation);
            } else {
                let studio_url = service
                    .conversation_url(Path::new(&args.path), &args.conversation_id)
                    .unwrap_or_default();
                print_conversation_detail(
                    &conversation,
                    (!studio_url.is_empty()).then_some(studio_url.as_str()),
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

fn cmd_conversations_get_audio<C: PlatformClient>(
    service: &AdkService<C>,
    args: ConversationsGetAudioArgs,
) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
        return ExitCode::from(1);
    }
    let output_path = args
        .output_path
        .clone()
        .unwrap_or_else(|| format!("{}.wav", args.conversation_id));
    match service.get_conversation_audio(&args.conversation_id, &args.direction, args.redacted) {
        Ok(audio) => {
            if let Err(error) = fs::write(PathBuf::from(&output_path), &audio) {
                emit_error(args.json, &format!("Failed to write audio file: {error}"));
                return ExitCode::from(1);
            }
            let size_bytes = audio.len();
            if args.json {
                println!(
                    "{}",
                    json!({
                        "success": true,
                        "conversation_id": args.conversation_id,
                        "direction": args.direction,
                        "redacted": args.redacted,
                        "output_path": output_path,
                        "size_bytes": size_bytes,
                    })
                );
            } else {
                let size_mb = size_bytes as f64 / 1_000_000.0;
                console::success(format!("Audio saved to {output_path} ({size_mb:.1} MB)"));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(args.json, &error.to_string());
            ExitCode::from(1)
        }
    }
}

fn print_conversations<C: PlatformClient>(
    service: &AdkService<C>,
    root: &Path,
    conversations: &[ConversationSummary],
) {
    let show_variant = conversations
        .iter()
        .any(|conversation| non_empty(conversation.variant_id.as_deref()).is_some());
    let mut headers = vec![
        "Conversation ID",
        "Started",
        "Duration",
        "From",
        "Channel",
    ];
    if show_variant {
        headers.push("Variant");
    }
    headers.extend(["Handoff", "Summary"]);

    let rows = conversations
        .iter()
        .map(|conversation| {
            let conversation_id = non_empty(Some(&conversation.conversation_id)).unwrap_or("-");
            let conversation_id_cell = service
                .conversation_url(root, conversation_id)
                .ok()
                .filter(|url| !url.is_empty())
                .map(|url| TableCell::link(conversation_id, &url))
                .unwrap_or_else(|| TableCell::plain(conversation_id));
            let mut row = vec![
                conversation_id_cell,
                TableCell::plain(
                    conversation
                        .started_at
                        .as_deref()
                        .map(format_iso_timestamp)
                        .unwrap_or_else(|| "-".to_string()),
                ),
                TableCell::plain(format_duration(
                    conversation.duration.or(conversation.total_duration),
                )),
                TableCell::plain(non_empty(conversation.from_number.as_deref()).unwrap_or("-")),
                TableCell::plain(non_empty(conversation.channel.as_deref()).unwrap_or("-")),
            ];
            if show_variant {
                row.push(TableCell::plain(
                    non_empty(conversation.variant_id.as_deref())
                        .unwrap_or("-")
                        .to_string(),
                ));
            }
            row.push(TableCell::plain(format_handoff(conversation)));
            row.push(TableCell::plain(extract_summary_heading(
                conversation.short_summary.as_ref(),
            )));
            row
        })
        .collect::<Vec<_>>();

    print_table(&headers, &rows);
}

fn print_conversation_detail(conversation: &ConversationDetail, studio_url: Option<&str>) {
    let summary = &conversation.summary;
    let conversation_id = non_empty(Some(&summary.conversation_id)).unwrap_or("-");
    console::plain(format!("[label]Conversation[/label] {conversation_id}"));
    if let Some(studio_url) = studio_url {
        console::plain(format!("  [label]Studio URL:[/label] {studio_url}"));
    }

    print_optional_field("Channel", summary.channel.as_deref());
    print_optional_field("Direction", summary.direction.as_deref());
    print_optional_field("Language", summary.language.as_deref());
    print_optional_field("From", summary.from_number.as_deref());
    print_optional_field("To", summary.to_number.as_deref());
    let started = summary.started_at.as_deref().map(format_iso_timestamp);
    print_optional_field("Started", started.as_deref());
    let finished = summary.finished_at.as_deref().map(format_iso_timestamp);
    print_optional_field("Finished", finished.as_deref());
    let in_progress = summary
        .in_progress
        .map(|value| if value { "yes" } else { "no" });
    print_optional_field("In Progress", in_progress);
    print_optional_field("Variant", summary.variant_id.as_deref());
    print_optional_field("Deployment", summary.deployment_id.as_deref());

    let duration = format_duration(summary.duration.or(summary.total_duration));
    if duration != "-" {
        console::plain(format!("  [label]Duration:[/label] {duration}"));
    }

    if summary.handoff.unwrap_or(false) {
        let destination = non_empty(summary.handoff_destination.as_deref()).unwrap_or("-");
        let reason = non_empty(summary.handoff_reason.as_deref()).unwrap_or("-");
        console::plain(format!(
            "  [label]Handoff:[/label] {destination} ({reason})"
        ));
    }
    if !summary.tags.is_empty() {
        let tags = summary.tags.join(", ");
        if !tags.is_empty() {
            console::plain(format!("  [label]Tags:[/label] {tags}"));
        }
    }
    if let Some(score) = summary.poly_score {
        console::plain(format!("  [label]PolyScore:[/label] {score}"));
    }
    let short_summary = extract_summary_heading(summary.short_summary.as_ref());
    if short_summary != "-" {
        console::plain(format!("\n  [label]Summary:[/label] {short_summary}"));
    }
    if let Some(note) = non_empty(summary.note.as_deref()) {
        console::plain(format!("  [label]Note:[/label] {note}"));
    }

    if conversation.turns.is_empty() {
        return;
    }
    console::plain(format!("\n[label]Turns ({}):[/label]", conversation.turns.len()));
    for turn in &conversation.turns {
        if let Some(input) = turn_text(turn.user_input.as_deref(), turn, &["userInput", "input"]) {
            console::plain(format!("  user: {input}"));
        }
        if let Some(response) =
            turn_text(turn.agent_response.as_deref(), turn, &["agentResponse", "response"])
        {
            console::plain(format!("  agent: {response}"));
        }
    }
}

#[derive(Debug, Clone)]
struct TableCell {
    display: String,
    width: usize,
}

impl TableCell {
    fn plain(text: impl Into<String>) -> Self {
        let display = text.into();
        Self {
            width: display.len(),
            display,
        }
    }

    fn link(text: &str, url: &str) -> Self {
        Self {
            display: format!("[link={url}]{text}[/link]"),
            width: text.len(),
        }
    }
}

fn print_table(headers: &[&str], rows: &[Vec<TableCell>]) {
    let mut widths = headers.iter().map(|header| header.len()).collect::<Vec<_>>();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(cell.width);
        }
    }
    console::plain(format_table_row(headers, &widths));
    for row in rows {
        console::plain(format_table_cells(row, &widths));
    }
}

fn format_table_row(cells: &[&str], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(idx, cell)| format!("{cell:<width$}", width = widths[idx]))
        .collect::<Vec<_>>()
        .join("  ")
}

fn format_table_cells(cells: &[TableCell], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(idx, cell)| {
            let padding = widths[idx].saturating_sub(cell.width);
            format!("{}{}", cell.display, " ".repeat(padding))
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn print_json<T: Serialize>(payload: &T) {
    println!(
        "{}",
        serde_json::to_string(payload).expect("serialize conversation response")
    );
}

fn print_optional_field(label: &str, value: Option<&str>) {
    if let Some(value) = non_empty(value) {
        console::plain(format!("  [label]{label}:[/label] {value}"));
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn format_handoff(conversation: &ConversationSummary) -> String {
    if !conversation.handoff.unwrap_or(false) {
        return String::new();
    }
    non_empty(conversation.handoff_destination.as_deref())
        .unwrap_or("yes")
        .to_string()
}

fn extract_summary_heading(short_summary: Option<&ConversationShortSummary>) -> String {
    let Some(short_summary) = short_summary else {
        return "-".to_string();
    };
    match short_summary {
        ConversationShortSummary::Object { heading, .. } => {
            heading.as_deref().map(non_empty_or_dash).unwrap_or_else(|| "-".to_string())
        }
        ConversationShortSummary::Text(text) => {
            let Some(text) = non_empty(Some(text)) else {
                return "-".to_string();
            };
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text)
                && let Some(heading) = parsed.get("heading").and_then(serde_json::Value::as_str)
            {
                return non_empty_or_dash(heading);
            }
            non_empty_or_dash(text)
        }
    }
}

fn non_empty_or_dash(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn format_duration(duration: Option<u64>) -> String {
    let Some(seconds) = duration else {
        return "-".to_string();
    };
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m{seconds:02}s")
    }
}

fn turn_text(
    primary: Option<&str>,
    turn: &ConversationTurn,
    fallback_keys: &[&str],
) -> Option<String> {
    if let Some(text) = non_empty(primary) {
        return Some(text.to_string());
    }
    fallback_keys
        .iter()
        .filter_map(|key| turn.extra.get(*key))
        .find_map(json_text)
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return non_empty(Some(text)).map(ToString::to_string);
    }
    Some(value.to_string())
}

fn format_iso_timestamp(timestamp: &str) -> String {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%d %b %y %H:%M %Z")
                .to_string()
        })
        .unwrap_or_else(|_| timestamp.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_heading_accepts_python_short_summary_shapes() {
        assert_eq!(
            extract_summary_heading(Some(&ConversationShortSummary::Text(
                "{\"heading\":\"Test call\",\"content\":\"x\"}".to_string()
            ))),
            "Test call"
        );
        assert_eq!(
            extract_summary_heading(Some(&ConversationShortSummary::Object {
                heading: Some("Plain object".to_string()),
                content: Some("x".to_string()),
                extra: serde_json::Map::new(),
            })),
            "Plain object"
        );
        assert_eq!(
            extract_summary_heading(Some(&ConversationShortSummary::Text(
                "raw text".to_string()
            ))),
            "raw text"
        );
        assert_eq!(extract_summary_heading(None), "-");
    }

    #[test]
    fn duration_matches_python_table_format() {
        assert_eq!(format_duration(Some(45)), "45s");
        assert_eq!(format_duration(Some(90)), "1m30s");
        assert_eq!(format_duration(None), "-");
    }
}
