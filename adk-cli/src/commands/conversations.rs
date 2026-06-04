use crate::{
    AdkService, ConversationsArgs, ConversationsCommands, ConversationsGetArgs,
    ConversationsGetAudioArgs, ConversationsListArgs, console, emit_error, ensure_project_loaded,
};
use adk_api_client::PlatformClient;
use chrono::{DateTime, Local};
use serde_json::{Value, json};
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
                println!("{payload}");
                return ExitCode::SUCCESS;
            }
            let conversations = payload
                .get("conversations")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if conversations.is_empty() {
                console::info("No conversations found.");
            } else {
                print_conversations(service, Path::new(&args.path), conversations);
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
                println!("{conversation}");
            } else {
                let studio_url = service
                    .conversation_url(Path::new(&args.path), &args.conversation_id)
                    .unwrap_or_default();
                print_conversation_detail(&conversation, (!studio_url.is_empty()).then_some(studio_url.as_str()));
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
    conversations: &[Value],
) {
    let show_variant = conversations
        .iter()
        .any(|conversation| string_field(conversation, &["variantId"]).is_some());
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
            let conversation_id = string_field(conversation, &["conversationId"])
                .or_else(|| string_field(conversation, &["conversation_id"]))
                .unwrap_or("-");
            let conversation_id_cell = service
                .conversation_url(root, conversation_id)
                .ok()
                .filter(|url| !url.is_empty())
                .map(|url| TableCell::link(conversation_id, &url))
                .unwrap_or_else(|| TableCell::plain(conversation_id));
            let mut row = vec![
                conversation_id_cell,
                TableCell::plain(
                    string_field(conversation, &["startedAt"])
                    .map(format_iso_timestamp)
                    .unwrap_or_else(|| "-".to_string()),
                ),
                TableCell::plain(format_duration(conversation.get("duration"))),
                TableCell::plain(string_field(conversation, &["fromNumber"]).unwrap_or("-")),
                TableCell::plain(string_field(conversation, &["channel"]).unwrap_or("-")),
            ];
            if show_variant {
                row.push(TableCell::plain(
                    string_field(conversation, &["variantId"])
                        .unwrap_or("-")
                        .to_string(),
                ));
            }
            row.push(TableCell::plain(format_handoff(conversation)));
            row.push(TableCell::plain(extract_summary_heading(
                conversation.get("shortSummary"),
            )));
            row
        })
        .collect::<Vec<_>>();

    print_table(&headers, &rows);
}

fn print_conversation_detail(conversation: &Value, studio_url: Option<&str>) {
    let conversation_id = string_field(conversation, &["conversationId", "conversation_id"])
        .unwrap_or("-");
    console::plain(format!("[label]Conversation[/label] {conversation_id}"));
    if let Some(studio_url) = studio_url {
        console::plain(format!("  [label]Studio URL:[/label] {studio_url}"));
    }

    for (label, keys) in [
        ("Channel", &["channel"][..]),
        ("Direction", &["direction"][..]),
        ("Language", &["language"][..]),
        ("From", &["fromNumber"][..]),
        ("To", &["toNumber"][..]),
        ("Started", &["startedAt"][..]),
        ("Finished", &["finishedAt"][..]),
        ("In Progress", &["inProgress"][..]),
        ("Variant", &["variantId"][..]),
        ("Deployment", &["deploymentId"][..]),
    ] {
        let Some(value) = conversation_field(conversation, keys) else {
            continue;
        };
        console::plain(format!("  [label]{label}:[/label] {value}"));
    }
    let duration = format_duration(conversation.get("duration"));
    if duration != "-" {
        console::plain(format!("  [label]Duration:[/label] {duration}"));
    }

    if conversation
        .get("handoff")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let destination = string_field(conversation, &["handoffDestination"]).unwrap_or("-");
        let reason = string_field(conversation, &["handoffReason"]).unwrap_or("-");
        console::plain(format!(
            "  [label]Handoff:[/label] {destination} ({reason})"
        ));
    }
    if let Some(tags) = conversation.get("tags").and_then(Value::as_array) {
        let tags = tags
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        if !tags.is_empty() {
            console::plain(format!("  [label]Tags:[/label] {tags}"));
        }
    }
    if let Some(score) = conversation.get("polyScore") {
        console::plain(format!("  [label]PolyScore:[/label] {score}"));
    }
    let summary = extract_summary_heading(conversation.get("shortSummary"));
    if summary != "-" {
        console::plain(format!("\n  [label]Summary:[/label] {summary}"));
    }
    if let Some(note) = string_field(conversation, &["note"]) {
        console::plain(format!("  [label]Note:[/label] {note}"));
    }

    let Some(turns) = conversation.get("turns").and_then(Value::as_array) else {
        return;
    };
    if turns.is_empty() {
        return;
    }
    console::plain(format!("\n[label]Turns ({}):[/label]", turns.len()));
    for turn in turns {
        if let Some(input) = string_field(turn, &["user_input", "userInput", "input"]) {
            if !input.is_empty() {
                console::plain(format!("  user: {input}"));
            }
        }
        if let Some(response) = string_field(turn, &["agent_response", "agentResponse", "response"])
        {
            if !response.is_empty() {
                console::plain(format!("  agent: {response}"));
            }
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

fn conversation_field(conversation: &Value, keys: &[&str]) -> Option<String> {
    let value = keys.iter().find_map(|key| conversation.get(*key))?;
    if value.is_null() {
        return None;
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { "yes" } else { "no" }.to_string());
    }
    if keys.iter().any(|key| matches!(*key, "startedAt" | "finishedAt")) {
        if let Some(text) = value.as_str() {
            return Some(format_iso_timestamp(text));
        }
    }
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| Some(value.to_string()))
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value.get(*key)?.as_str())
}

fn format_handoff(conversation: &Value) -> String {
    if !conversation
        .get("handoff")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return String::new();
    }
    string_field(conversation, &["handoffDestination"])
        .unwrap_or("yes")
        .to_string()
}

fn extract_summary_heading(short_summary: Option<&Value>) -> String {
    let Some(short_summary) = short_summary else {
        return "-".to_string();
    };
    if let Some(heading) = short_summary.get("heading").and_then(Value::as_str) {
        return non_empty_or_dash(heading);
    }
    let Some(text) = short_summary.as_str() else {
        return non_empty_or_dash(&short_summary.to_string());
    };
    if let Ok(parsed) = serde_json::from_str::<Value>(text)
        && let Some(heading) = parsed.get("heading").and_then(Value::as_str)
    {
        return non_empty_or_dash(heading);
    }
    non_empty_or_dash(text)
}

fn non_empty_or_dash(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn format_duration(duration: Option<&Value>) -> String {
    let Some(duration) = duration else {
        return "-".to_string();
    };
    let seconds = duration
        .as_u64()
        .or_else(|| duration.as_i64().and_then(|value| u64::try_from(value).ok()))
        .or_else(|| duration.as_f64().filter(|value| *value >= 0.0).map(|value| value as u64));
    let Some(seconds) = seconds else {
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
    use serde_json::json;

    #[test]
    fn summary_heading_accepts_python_short_summary_shapes() {
        assert_eq!(
            extract_summary_heading(Some(&json!("{\"heading\":\"Test call\",\"content\":\"x\"}"))),
            "Test call"
        );
        assert_eq!(
            extract_summary_heading(Some(&json!({"heading": "Plain object"}))),
            "Plain object"
        );
        assert_eq!(extract_summary_heading(Some(&json!("raw text"))), "raw text");
        assert_eq!(extract_summary_heading(None), "-");
    }

    #[test]
    fn duration_matches_python_table_format() {
        assert_eq!(format_duration(Some(&json!(45))), "45s");
        assert_eq!(format_duration(Some(&json!(90))), "1m30s");
        assert_eq!(format_duration(None), "-");
    }
}
