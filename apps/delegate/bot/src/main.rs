mod context;
mod event;
mod logger;
mod models;
mod retriever;
mod slack;
mod triage;
mod workspace;

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

use context::TaskType;
use event::DelegateEvent;
use models::{delegate_tools, ChatOptions, CompleteOptions, ModelClient, ToolCall};
use slack::SlackSocket;
use triage::TriageLabel;
use workspace::Workspace;

const MAX_TOOL_TURNS: usize = 5;

struct Config {
    slack_app_token: String,
    slack_bot_token: String,
    provider: String,
    model: Option<String>,
    workspace_path: String,
    bot_user_id: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let slack_app_token = std::env::var("SLACK_APP_TOKEN")
            .map_err(|_| anyhow!("SLACK_APP_TOKEN not set (xapp-... token)"))?;
        let slack_bot_token = std::env::var("SLACK_BOT_TOKEN")
            .map_err(|_| anyhow!("SLACK_BOT_TOKEN not set (xoxb-... token)"))?;
        let provider =
            std::env::var("DELEGATE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
        let model = std::env::var("DELEGATE_MODEL").ok();
        let workspace_path =
            std::env::var("DELEGATE_WORKSPACE").unwrap_or_else(|_| "./workspace".to_string());
        let bot_user_id =
            std::env::var("SLACK_BOT_USER_ID").unwrap_or_else(|_| String::new());

        Ok(Self {
            slack_app_token,
            slack_bot_token,
            provider,
            model,
            workspace_path,
            bot_user_id,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Delegate Bot starting...");

    let config = Config::from_env()?;
    let ws = Workspace::new(&config.workspace_path);
    let model_client = ModelClient::new(&config.provider)?;

    info!("Listening to all channels the bot is invited to");

    let (event_tx, mut event_rx) = mpsc::channel::<serde_json::Value>(100);

    let slack_handle = {
        let slack_for_listener = SlackSocket::new(
            config.slack_app_token.clone(),
            config.slack_bot_token.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = slack_for_listener.run(event_tx).await {
                error!("Socket Mode listener failed: {e}");
            }
        })
    };

    info!("Event loop running. Waiting for Slack events...");

    // Dedup cache: track recently processed (channel, timestamp) pairs
    let seen_events: Arc<Mutex<(HashSet<String>, VecDeque<String>)>> =
        Arc::new(Mutex::new((HashSet::new(), VecDeque::new())));
    const DEDUP_CACHE_SIZE: usize = 200;

    while let Some(envelope) = event_rx.recv().await {
        let evt = match event::normalize(&envelope) {
            Some(e) => e,
            None => continue,
        };

        // Dedup: skip if we've already processed this (channel, ts) pair
        let dedup_key = format!("{}:{}", evt.channel, evt.timestamp);
        {
            let mut cache = seen_events.lock().await;
            if cache.0.contains(&dedup_key) {
                info!(event_type = %evt.event_type, "Skipping duplicate event");
                continue;
            }
            cache.0.insert(dedup_key.clone());
            cache.1.push_back(dedup_key);
            while cache.1.len() > DEDUP_CACHE_SIZE {
                if let Some(old) = cache.1.pop_front() {
                    cache.0.remove(&old);
                }
            }
        }

        info!(
            event_type = %evt.event_type,
            channel = %evt.channel,
            user = %evt.user,
            "Received event"
        );

        let slack_for_event = SlackSocket::new(
            config.slack_app_token.clone(),
            config.slack_bot_token.clone(),
        );
        let client = model_client.clone();
        let ws_clone = ws.clone();
        let bot_user_id = config.bot_user_id.clone();
        let model_override = config.model.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_event(
                evt,
                &slack_for_event,
                &client,
                &ws_clone,
                &bot_user_id,
                model_override.as_deref(),
            )
            .await
            {
                error!("Error handling event: {e:#}");
            }
        });
    }

    slack_handle.await?;
    Ok(())
}

async fn handle_event(
    event: DelegateEvent,
    slack: &SlackSocket,
    client: &ModelClient,
    ws: &Workspace,
    bot_user_id: &str,
    model_override: Option<&str>,
) -> Result<()> {
    // --- Tier 0 triage ---
    if let Some(label) = triage::tier0_classify(&event, bot_user_id) {
        match label {
            TriageLabel::Ignore => {
                info!(reason = "tier0", "Ignoring event");
                return Ok(());
            }
            TriageLabel::ActNow => {
                // app_mention — fall through to full processing
            }
            TriageLabel::Queue => {
                logger::append_log(ws.path(), &event.channel, &event.user, &event.content)
                    .await?;
                info!(reason = "tier0-queue", "Queued event");
                return Ok(());
            }
        }
    } else {
        // --- Tier 1 triage (LLM-based) ---
        let intent_summary = ws.intents().await;
        let (label, reasoning) =
            triage::tier1_classify(&event, &intent_summary, client, None).await?;

        info!(label = %label, reasoning = %reasoning, "Tier 1 triage result");

        match label {
            TriageLabel::Ignore | TriageLabel::Queue => {
                logger::append_log(ws.path(), &event.channel, &event.user, &event.content)
                    .await?;
                return Ok(());
            }
            TriageLabel::ActNow => {}
        }
    }

    // --- Full context assembly + tool-use LLM call ---
    info!("Processing act-now event, assembling context...");

    let thread_ts = event.thread_ts.as_deref().unwrap_or(&event.timestamp);

    // Resolve user display name
    let user_name = slack.get_user_name(&event.user).await;

    // Fetch thread context if this message is part of a thread
    let thread_context = if event.thread_ts.is_some() {
        let messages = slack.get_thread(&event.channel, thread_ts).await?;
        let mut lines = Vec::new();
        for msg in &messages {
            let uid = msg["user"].as_str().unwrap_or("unknown");
            let name = slack.get_user_name(uid).await;
            let text = msg["text"].as_str().unwrap_or("");
            let ts = msg["ts"].as_str().unwrap_or("");
            if ts != event.timestamp {
                lines.push(format!("<{name}> {text}"));
            }
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("Thread history (oldest first):\n{}", lines.join("\n"))
        }
    } else {
        String::new()
    };

    let recent_logs = logger::read_recent_logs(ws.path()).await;

    let mut compiled =
        context::compile(&event, ws.path(), TaskType::Respond, &recent_logs, 8000).await?;

    // Override trigger with resolved display name instead of raw user ID
    compiled.trigger = format!(
        "Channel: {}\nFrom: {}\nTime: {}\n\n{}",
        event.channel, user_name, event.timestamp, event.content
    );

    let (system_prompt, mut user_prompt) = context::to_prompt(&compiled, &recent_logs);

    // Prepend thread context to user prompt so the model sees the full conversation
    if !thread_context.is_empty() {
        user_prompt = format!("{thread_context}\n\n---\nNew message:\n{user_prompt}");
    }

    info!(
        system_len = system_prompt.len(),
        user_len = user_prompt.len(),
        "Calling LLM with tools..."
    );

    let tools = delegate_tools();
    let model = model_override.map(|s| s.to_string());

    // First call uses single-shot complete
    let response = client
        .complete(CompleteOptions {
            system: system_prompt.clone(),
            prompt: user_prompt.clone(),
            model: model.clone(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            tools: Some(tools.clone()),
        })
        .await?;

    info!(
        model = %response.model,
        input_tokens = response.input_tokens,
        output_tokens = response.output_tokens,
        duration_ms = response.duration_ms,
        tool_calls = response.tool_calls.len(),
        "LLM response received"
    );

    let mut all_actions = Vec::new();
    let mut has_reply = false;
    let mut final_content = response.content.clone();

    // Execute tools and potentially loop for multi-turn
    let mut current_response = response;
    let mut turn = 0;

    loop {
        if current_response.tool_calls.is_empty() {
            break;
        }

        let mut tool_results: Vec<(String, String)> = Vec::new(); // (tool_call_id, result)
        let mut needs_followup = false;

        for call in &current_response.tool_calls {
            info!(tool = %call.name, "Executing tool call");
            all_actions.push(call.name.clone());

            let result = execute_tool(call, slack, ws, &event, thread_ts).await;

            if call.name == "reply" {
                has_reply = true;
            }

            // Tools that return data need a followup turn
            if call.name == "read_file" {
                needs_followup = true;
            }

            tool_results.push((call.id.clone(), result));
        }

        turn += 1;
        if !needs_followup || turn >= MAX_TOOL_TURNS {
            break;
        }

        // Build message history for multi-turn
        let mut messages: Vec<Value> = vec![
            serde_json::json!({"role": "user", "content": user_prompt}),
            current_response.raw_assistant_message.clone(),
        ];

        // Append tool results
        for (tool_call_id, result) in &tool_results {
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": result
            }));
        }

        info!(turn = turn, "Multi-turn: sending tool results back to LLM");

        let next_response = client
            .chat(ChatOptions {
                system: system_prompt.clone(),
                messages,
                model: model.clone(),
                max_tokens: Some(2048),
                temperature: Some(0.7),
                tools: Some(tools.clone()),
            })
            .await?;

        info!(
            tool_calls = next_response.tool_calls.len(),
            content_len = next_response.content.len(),
            "Multi-turn response received"
        );

        final_content = next_response.content.clone();
        current_response = next_response;
    }

    // If model returned text content but no reply tool call, post it
    if !final_content.is_empty() && !has_reply {
        info!("Model returned text content without reply tool, posting as reply");
        slack
            .post_message(&event.channel, &final_content, Some(thread_ts))
            .await?;
    }

    // Log the interaction
    logger::append_log(ws.path(), &event.channel, &event.user, &event.content).await?;
    if !all_actions.is_empty() {
        logger::append_log(
            ws.path(),
            &event.channel,
            "delegate-bot",
            &format!("[actions: {}]", all_actions.join(", ")),
        )
        .await?;
    }

    Ok(())
}

/// Execute a single tool call and return a result string for multi-turn.
async fn execute_tool(
    call: &ToolCall,
    slack: &SlackSocket,
    ws: &Workspace,
    event: &DelegateEvent,
    thread_ts: &str,
) -> String {
    match call.name.as_str() {
        "react" => {
            let emoji = call.arguments["emoji"].as_str().unwrap_or("eyes");
            match slack
                .add_reaction(&event.channel, &event.timestamp, emoji)
                .await
            {
                Ok(_) => format!("Reacted with :{emoji}:"),
                Err(e) => format!("Failed to react: {e}"),
            }
        }
        "reply" => {
            let text = call.arguments["text"].as_str().unwrap_or("");
            if !text.is_empty() {
                match slack
                    .post_message(&event.channel, text, Some(thread_ts))
                    .await
                {
                    Ok(_) => "Reply posted".to_string(),
                    Err(e) => format!("Failed to reply: {e}"),
                }
            } else {
                "Empty reply, skipped".to_string()
            }
        }
        "post" => {
            let channel = call.arguments["channel"].as_str().unwrap_or("");
            let text = call.arguments["text"].as_str().unwrap_or("");
            if !channel.is_empty() && !text.is_empty() {
                match slack.post_message(channel, text, None).await {
                    Ok(_) => format!("Posted to {channel}"),
                    Err(e) => format!("Failed to post: {e}"),
                }
            } else {
                "Missing channel or text, skipped".to_string()
            }
        }
        "no_action" => {
            let reason = call.arguments["reason"].as_str().unwrap_or("no reason given");
            info!(reason = %reason, "Model chose no_action");
            format!("No action taken: {reason}")
        }
        "create_skill" => {
            let name = call.arguments["name"].as_str().unwrap_or("");
            let description = call.arguments["description"].as_str().unwrap_or("");
            let content = call.arguments["content"].as_str().unwrap_or("");
            if !name.is_empty() && !content.is_empty() {
                let skill_dir = ws.path().join("skills").join(name);
                if let Err(e) = tokio::fs::create_dir_all(&skill_dir).await {
                    return format!("Failed to create skill directory: {e}");
                }
                let skill_md =
                    format!("---\nname: {name}\ndescription: {description}\n---\n\n{content}\n");
                let skill_path = skill_dir.join("SKILL.md");
                match tokio::fs::write(&skill_path, &skill_md).await {
                    Ok(_) => {
                        info!(skill = %name, "Created skill");
                        format!("Skill '{name}' created")
                    }
                    Err(e) => format!("Failed to write skill: {e}"),
                }
            } else {
                "Missing name or content for skill".to_string()
            }
        }
        "read_file" => {
            let path = call.arguments["path"].as_str().unwrap_or("");
            if path.is_empty() {
                return "No path provided".to_string();
            }
            if path.contains("..") {
                return "Path traversal blocked".to_string();
            }
            let full = ws.path().join(path);
            match tokio::fs::read_to_string(&full).await {
                Ok(contents) => {
                    info!(path = %path, len = contents.len(), "Read file");
                    // Truncate very large files to avoid blowing up context
                    if contents.len() > 10000 {
                        format!("{}\n\n[truncated, {} bytes total]", &contents[..10000], contents.len())
                    } else {
                        contents
                    }
                }
                Err(_) => format!("File not found: {path}"),
            }
        }
        "write_file" => {
            let path = call.arguments["path"].as_str().unwrap_or("");
            let content = call.arguments["content"].as_str().unwrap_or("");
            if path.is_empty() {
                return "No path provided".to_string();
            }
            if path.contains("..") {
                return "Path traversal blocked".to_string();
            }
            let full = ws.path().join(path);
            if let Some(parent) = full.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::write(&full, content).await {
                Ok(_) => {
                    info!(path = %path, "Wrote file");
                    format!("Written to {path}")
                }
                Err(e) => format!("Failed to write {path}: {e}"),
            }
        }
        other => {
            warn!(tool = %other, "Unknown tool call");
            format!("Unknown tool: {other}")
        }
    }
}

