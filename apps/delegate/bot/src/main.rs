mod context;
mod event;
mod logger;
mod models;
mod retriever;
mod slack;
mod triage;
mod workspace;

use anyhow::{anyhow, Result};
use tokio::sync::mpsc;
use tracing::{error, info};

use context::TaskType;
use event::DelegateEvent;
use models::{delegate_tools, CompleteOptions, ModelClient};
use slack::SlackSocket;
use triage::TriageLabel;
use workspace::Workspace;

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

    while let Some(envelope) = event_rx.recv().await {
        let evt = match event::normalize(&envelope) {
            Some(e) => e,
            None => continue,
        };

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

    // Fetch thread context if this message is part of a thread
    let thread_context = if event.thread_ts.is_some() {
        let messages = slack.get_thread(&event.channel, thread_ts).await?;
        let mut lines = Vec::new();
        for msg in &messages {
            let user = msg["user"].as_str().unwrap_or("unknown");
            let text = msg["text"].as_str().unwrap_or("");
            let ts = msg["ts"].as_str().unwrap_or("");
            if ts != event.timestamp {
                lines.push(format!("<{user}> {text}"));
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

    let compiled = context::compile(&event, ws.path(), TaskType::Respond, &recent_logs, 8000)
        .await?;

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

    let response = client
        .complete(CompleteOptions {
            system: system_prompt,
            prompt: user_prompt,
            model: model_override.map(|s| s.to_string()),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            tools: Some(delegate_tools()),
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

    // Execute tool calls
    for call in &response.tool_calls {
        info!(tool = %call.name, args = %call.arguments, "Executing tool call");

        match call.name.as_str() {
            "react" => {
                let emoji = call.arguments["emoji"].as_str().unwrap_or("eyes");
                slack
                    .add_reaction(&event.channel, &event.timestamp, emoji)
                    .await?;
            }
            "reply" => {
                let text = call.arguments["text"].as_str().unwrap_or("");
                if !text.is_empty() {
                    slack
                        .post_message(&event.channel, text, Some(thread_ts))
                        .await?;
                }
            }
            "post" => {
                let channel = call.arguments["channel"].as_str().unwrap_or("");
                let text = call.arguments["text"].as_str().unwrap_or("");
                if !channel.is_empty() && !text.is_empty() {
                    slack.post_message(channel, text, None).await?;
                }
            }
            "no_action" => {
                let reason = call.arguments["reason"].as_str().unwrap_or("no reason given");
                info!(reason = %reason, "Model chose no_action");
            }
            "create_skill" => {
                let name = call.arguments["name"].as_str().unwrap_or("");
                let description = call.arguments["description"].as_str().unwrap_or("");
                let content = call.arguments["content"].as_str().unwrap_or("");
                if !name.is_empty() && !content.is_empty() {
                    let skill_dir = ws.path().join("skills").join(name);
                    if let Err(e) = tokio::fs::create_dir_all(&skill_dir).await {
                        error!(skill = %name, error = %e, "Failed to create skill directory");
                    } else {
                        let skill_md = format!(
                            "---\nname: {name}\ndescription: {description}\n---\n\n{content}\n"
                        );
                        let skill_path = skill_dir.join("SKILL.md");
                        match tokio::fs::write(&skill_path, &skill_md).await {
                            Ok(_) => {
                                info!(skill = %name, "Created skill");
                            }
                            Err(e) => error!(skill = %name, error = %e, "Failed to write skill"),
                        }
                    }
                }
            }
            "read_file" => {
                let path = call.arguments["path"].as_str().unwrap_or("");
                if !path.is_empty() {
                    let full = ws.path().join(path);
                    // Block path traversal
                    if let Ok(canonical) = tokio::fs::canonicalize(&full).await {
                        let ws_canonical = tokio::fs::canonicalize(ws.path()).await.unwrap_or_default();
                        if canonical.starts_with(&ws_canonical) {
                            match tokio::fs::read_to_string(&canonical).await {
                                Ok(contents) => {
                                    info!(path = %path, len = contents.len(), "Read file");
                                    // File contents are available for the model in multi-turn,
                                    // but for now we just log it
                                }
                                Err(e) => info!(path = %path, error = %e, "File not found"),
                            }
                        } else {
                            info!(path = %path, "Blocked path traversal attempt");
                        }
                    } else {
                        info!(path = %path, "File does not exist");
                    }
                }
            }
            "write_file" => {
                let path = call.arguments["path"].as_str().unwrap_or("");
                let content = call.arguments["content"].as_str().unwrap_or("");
                if !path.is_empty() {
                    // Block path traversal: no ".." components allowed
                    if path.contains("..") {
                        info!(path = %path, "Blocked path traversal attempt");
                    } else {
                        let full = ws.path().join(path);
                        if let Some(parent) = full.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        match tokio::fs::write(&full, content).await {
                            Ok(_) => {
                                info!(path = %path, "Wrote file");
                            }
                            Err(e) => error!(path = %path, error = %e, "Failed to write file"),
                        }
                    }
                }
            }
            other => {
                info!(tool = %other, "Unknown tool call, skipping");
            }
        }
    }

    // If model returned text content but no reply tool call, post it
    if !response.content.is_empty()
        && !response.tool_calls.iter().any(|c| c.name == "reply")
    {
        info!("Model returned text content without reply tool, posting as reply");
        slack
            .post_message(&event.channel, &response.content, Some(thread_ts))
            .await?;
    }

    // Log the interaction
    logger::append_log(ws.path(), &event.channel, &event.user, &event.content).await?;

    let actions: Vec<String> = response
        .tool_calls
        .iter()
        .map(|c| c.name.clone())
        .collect();
    logger::append_log(
        ws.path(),
        &event.channel,
        "delegate-bot",
        &format!("[actions: {}]", actions.join(", ")),
    )
    .await?;

    Ok(())
}

