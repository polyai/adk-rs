use crate::{
    AdkService, ChatArgs, ProjectWorkspace, console, emit_error, ensure_project_loaded,
    remote_service_for_path,
};
use adk_api_client::PlatformClient;
use serde_json::json;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn cmd_chat(args: ChatArgs) -> ExitCode {
    let workspace = ProjectWorkspace::new();
    match remote_service_for_path(&workspace, &args.path, args.json) {
        Ok(service) => cmd_chat_with_service(&service, args),
        Err(code) => code,
    }
}

fn cmd_chat_with_service<C: PlatformClient>(service: &AdkService<C>, args: ChatArgs) -> ExitCode {
    if !ensure_project_loaded(service, &args.path, args.json) {
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
            service,
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

struct ChatLoopOptions<'a, C: PlatformClient> {
    service: &'a AdkService<C>,
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

fn run_chat_loop<C: PlatformClient>(options: ChatLoopOptions<'_, C>) -> ChatLoopResult {
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


fn read_chat_messages(
    input_file: Option<&str>,
    messages: &[String],
    json_mode: bool,
) -> Result<Vec<String>, ExitCode> {
    if let Some(input_file) = input_file {
        let read_result = if input_file == "-" {
            let mut src = String::new();
            io::stdin().read_to_string(&mut src).map(|_| src)
        } else {
            fs::read_to_string(input_file)
        };
        match read_result {
            Ok(_) => emit_error(
                json_mode,
                "'str' object does not support the context manager protocol (missed __exit__ method)",
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                emit_error(json_mode, &format!("Input file not found: {input_file}"));
            }
            Err(error) => emit_error(json_mode, &error.to_string()),
        }
        return Err(ExitCode::from(1));
    }
    Ok(messages.to_vec())
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
