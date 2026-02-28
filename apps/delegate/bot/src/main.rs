mod context;
mod event;
mod heartbeat;
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
use tracing::{debug, error, info, warn};

use context::TaskType;
use event::DelegateEvent;
use models::{delegate_tools, ChatOptions, CompleteOptions, ModelClient, ToolCall};
use slack::SlackSocket;
use triage::TriageLabel;
use workspace::Workspace;

const MAX_TOOL_TURNS: usize = 5;

/// Action autonomy tiers per the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionTier {
    /// Execute immediately, no notification needed.
    Autonomous,
    /// Execute immediately, but log/notify that it happened.
    AutonomousWithNotice,
    /// Write to pending/ audit trail before executing. (For now, treated as notice.)
    RequiresApproval,
}

/// Classify a tool call into an action tier.
fn classify_action(tool_name: &str) -> ActionTier {
    match tool_name {
        // Read-only operations: always autonomous
        "react" | "no_action" | "read_file" | "recall_memory" | "channel_history" => {
            ActionTier::Autonomous
        }
        // Visible actions: autonomous with notice
        "reply" | "post" | "save_memory" | "log_decision" => ActionTier::AutonomousWithNotice,
        // Higher-impact actions: approval-required (notice for now during dogfooding)
        "dm_user" | "update_intents" | "create_skill" | "write_file" => {
            ActionTier::RequiresApproval
        }
        _ => ActionTier::AutonomousWithNotice,
    }
}

/// Shared daily token budget tracker.
/// Tracks total tokens used today. Resets at midnight.
#[derive(Clone)]
struct TokenBudget {
    inner: Arc<Mutex<TokenBudgetInner>>,
}

struct TokenBudgetInner {
    used: u64,
    limit: u64,
    date: String,
    notified: bool,
}

impl TokenBudget {
    fn new(limit: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenBudgetInner {
                used: 0,
                limit,
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
                notified: false,
            })),
        }
    }

    /// Record token usage. Returns true if still within budget.
    async fn record(&self, tokens: u64) -> bool {
        let mut inner = self.inner.lock().await;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Reset at midnight
        if inner.date != today {
            info!(
                prev_date = %inner.date,
                used = inner.used,
                "Token budget reset for new day"
            );
            inner.used = 0;
            inner.date = today;
            inner.notified = false;
        }

        inner.used += tokens;
        inner.used <= inner.limit
    }

    /// Check if we're within budget without recording.
    async fn is_available(&self) -> bool {
        let mut inner = self.inner.lock().await;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        if inner.date != today {
            inner.used = 0;
            inner.date = today;
            inner.notified = false;
        }

        inner.used < inner.limit
    }

    /// Mark that we've notified the team about budget exhaustion.
    async fn mark_notified(&self) {
        self.inner.lock().await.notified = true;
    }

    /// Check if team has been notified already.
    async fn was_notified(&self) -> bool {
        self.inner.lock().await.notified
    }

    /// Update the limit (e.g., from HEARTBEAT.md reload).
    async fn set_limit(&self, limit: u64) {
        self.inner.lock().await.limit = limit;
    }

    /// Get current usage stats.
    async fn stats(&self) -> (u64, u64) {
        let inner = self.inner.lock().await;
        (inner.used, inner.limit)
    }
}

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

    // Parse initial config for daily budget
    let hb_config = heartbeat::parse_config(ws.path()).await;
    let token_budget = TokenBudget::new(hb_config.daily_token_budget);

    info!(
        daily_budget = hb_config.daily_token_budget,
        "Listening to all channels the bot is invited to"
    );

    let (event_tx, mut event_rx) = mpsc::channel::<serde_json::Value>(100);

    // Single shared SlackSocket — user name cache persists across all event handlers
    let slack = SlackSocket::new(
        config.slack_app_token.clone(),
        config.slack_bot_token.clone(),
    );

    let slack_handle = {
        let slack_for_listener = slack.clone();
        tokio::spawn(async move {
            if let Err(e) = slack_for_listener.run(event_tx).await {
                error!("Socket Mode listener failed: {e}");
            }
        })
    };

    // Spawn heartbeat loop
    let heartbeat_handle = {
        let hb_slack = slack.clone();
        let hb_client = model_client.clone();
        let hb_ws = ws.clone();
        let hb_model = config.model.clone();
        let hb_budget = token_budget.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_heartbeat(&hb_ws, &hb_client, &hb_slack, hb_model.as_deref(), &hb_budget)
                    .await
            {
                error!("Heartbeat loop failed: {e}");
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

        let slack_for_event = slack.clone();
        let client = model_client.clone();
        let ws_clone = ws.clone();
        let bot_user_id = config.bot_user_id.clone();
        let model_override = config.model.clone();
        let evt_budget = token_budget.clone();

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300), // 5-minute timeout per event
                handle_event(
                    evt,
                    &slack_for_event,
                    &client,
                    &ws_clone,
                    &bot_user_id,
                    model_override.as_deref(),
                    &evt_budget,
                ),
            )
            .await;

            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error!("Error handling event: {e:#}"),
                Err(_) => error!("Event handler timed out after 5 minutes"),
            }
        });
    }

    // Both loops run until the process exits
    tokio::select! {
        r = slack_handle => { r?; }
        r = heartbeat_handle => { r?; }
    }
    Ok(())
}

#[tracing::instrument(
    skip(slack, client, ws, budget),
    fields(
        channel = %event.channel,
        user = %event.user,
        event_type = %event.event_type,
        total_tokens = tracing::field::Empty,
        tool_count = tracing::field::Empty,
    )
)]
async fn handle_event(
    event: DelegateEvent,
    slack: &SlackSocket,
    client: &ModelClient,
    ws: &Workspace,
    bot_user_id: &str,
    model_override: Option<&str>,
    budget: &TokenBudget,
) -> Result<()> {
    let event_start = std::time::Instant::now();
    let mut event_tokens: u64 = 0;

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
                logger::append_log(
                    ws.path(),
                    &event.channel,
                    &event.user,
                    &format!("[queued] {}", event.content),
                )
                .await?;
                info!(reason = "tier0-queue", "Queued event");
                return Ok(());
            }
        }
    } else {
        // --- Budget check for Tier 1 triage ---
        if !budget.is_available().await {
            // Log-only mode: skip LLM triage, just log
            logger::append_log(
                ws.path(),
                &event.channel,
                &event.user,
                &format!("[log-only] {}", event.content),
            )
            .await?;
            return Ok(());
        }

        // --- Tier 1 triage (LLM-based) ---
        let raw_intents = ws.intents().await;
        let intent_summary = context::compress_intents(&raw_intents, 500);
        let (label, reasoning) =
            triage::tier1_classify(&event, &intent_summary, client, None).await?;

        // Record triage tokens
        budget.record(200).await; // Tier 1 uses ~100-200 tokens

        info!(label = %label, reasoning = %reasoning, "Tier 1 triage result");

        match label {
            TriageLabel::Ignore => {
                logger::append_log(ws.path(), &event.channel, &event.user, &event.content)
                    .await?;
                return Ok(());
            }
            TriageLabel::Queue => {
                logger::append_log(
                    ws.path(),
                    &event.channel,
                    &event.user,
                    &format!("[queued] {}", event.content),
                )
                .await?;
                return Ok(());
            }
            TriageLabel::ActNow => {}
        }
    }

    // --- Budget check for full reasoning ---
    if !budget.is_available().await {
        logger::append_log(
            ws.path(),
            &event.channel,
            &event.user,
            &format!("[log-only] {}", event.content),
        )
        .await?;
        info!("Token budget exhausted, logging only");
        return Ok(());
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

    let (system_prompt, mut user_prompt) = context::to_prompt(&compiled);

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

    // Record token usage
    event_tokens += response.input_tokens + response.output_tokens;
    let within_budget =
        budget.record(response.input_tokens + response.output_tokens).await;
    if !within_budget && !budget.was_notified().await {
        // Notify team about budget exhaustion
        warn!("Daily token budget exhausted");
        let _ = slack
            .post_message(
                &event.channel,
                "Daily token budget exhausted. Entering log-only mode until midnight. \
                 I'll keep logging events but won't respond until the budget resets.",
                None,
            )
            .await;
        budget.mark_notified().await;
    }

    let mut all_actions = Vec::new();
    let mut has_reply = false;
    let mut final_content = response.content.clone();

    // Execute tools and potentially loop for multi-turn.
    // Accumulate full message history so the model sees all prior tool exchanges.
    let mut conversation: Vec<Value> = vec![
        serde_json::json!({"role": "user", "content": user_prompt}),
    ];
    let mut current_response = response;
    let mut turn = 0;

    loop {
        if current_response.tool_calls.is_empty() {
            break;
        }

        let mut tool_results: Vec<(String, String)> = Vec::new();
        let mut needs_followup = false;

        for call in &current_response.tool_calls {
            let tier = classify_action(&call.name);
            info!(tool = %call.name, tier = ?tier, "Executing tool call");
            all_actions.push(call.name.clone());

            // Audit trail for approval-required actions
            if tier == ActionTier::RequiresApproval {
                let _ = write_pending_action(ws, call, &event).await;
            }

            let result = execute_tool(call, slack, ws, &event, thread_ts).await;

            if call.name == "reply" {
                has_reply = true;
            }

            if call.name == "read_file" || call.name == "recall_memory" || call.name == "channel_history" {
                needs_followup = true;
            }

            tool_results.push((call.id.clone(), result));
        }

        turn += 1;
        if !needs_followup || turn >= MAX_TOOL_TURNS {
            break;
        }

        // Append assistant response + tool results to conversation history
        conversation.push(current_response.raw_assistant_message.clone());
        for (tool_call_id, result) in &tool_results {
            conversation.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": result
            }));
        }

        info!(turn = turn, "Multi-turn: sending tool results back to LLM");

        let next_response = client
            .chat(ChatOptions {
                system: system_prompt.clone(),
                messages: conversation.clone(),
                model: model.clone(),
                max_tokens: Some(2048),
                temperature: Some(0.7),
                tools: Some(tools.clone()),
            })
            .await?;

        event_tokens += next_response.input_tokens + next_response.output_tokens;
        budget.record(next_response.input_tokens + next_response.output_tokens).await;

        info!(
            tool_calls = next_response.tool_calls.len(),
            content_len = next_response.content.len(),
            tokens = next_response.input_tokens + next_response.output_tokens,
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

    // Record span fields for structured logging
    let span = tracing::Span::current();
    span.record("total_tokens", event_tokens);
    span.record("tool_count", all_actions.len());

    info!(
        duration_ms = event_start.elapsed().as_millis() as u64,
        tokens = event_tokens,
        actions = all_actions.len(),
        "Event handling complete"
    );

    Ok(())
}

/// Execute a single tool call and return a result string for multi-turn.
#[tracing::instrument(skip(slack, ws, event), fields(tool = %call.name))]
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
        "dm_user" => {
            let user = call.arguments["user"].as_str().unwrap_or("");
            let text = call.arguments["text"].as_str().unwrap_or("");
            if user.is_empty() || text.is_empty() {
                return "Missing user or text".to_string();
            }
            match slack.send_dm(user, text).await {
                Ok(_) => {
                    info!(user = %user, "Sent DM");
                    format!("DM sent to {user}")
                }
                Err(e) => format!("Failed to send DM: {e}"),
            }
        }
        "channel_history" => {
            let channel = call.arguments["channel"].as_str().unwrap_or("");
            if channel.is_empty() {
                return "No channel provided".to_string();
            }
            let count = call.arguments["count"].as_u64().unwrap_or(20).min(50) as u32;

            match slack.get_channel_history(channel, count).await {
                Ok(messages) => {
                    if messages.is_empty() {
                        return format!("No messages found in channel {channel}");
                    }

                    let mut lines = Vec::new();
                    for msg in &messages {
                        let uid = msg["user"].as_str().unwrap_or("unknown");
                        let name = slack.get_user_name(uid).await;
                        let text = msg["text"].as_str().unwrap_or("");
                        let ts = msg["ts"].as_str().unwrap_or("");
                        lines.push(format!("[{ts}] <{name}> {text}"));
                    }

                    // Reverse so oldest is first (messages come newest-first from API)
                    lines.reverse();

                    info!(channel = %channel, count = lines.len(), "Read channel history");
                    let result = lines.join("\n");
                    // Truncate if too large
                    if result.len() > 8000 {
                        format!("{}\n\n[truncated, {} messages total]", &result[..8000], messages.len())
                    } else {
                        result
                    }
                }
                Err(e) => format!("Failed to read channel history: {e}"),
            }
        }
        "save_memory" => {
            let topic = call.arguments["topic"].as_str().unwrap_or("");
            let content = call.arguments["content"].as_str().unwrap_or("");
            let summary = call.arguments["summary"].as_str().unwrap_or("");
            if topic.is_empty() || content.is_empty() {
                return "Missing topic or content".to_string();
            }
            // Validate topic slug (kebab-case, no path traversal)
            if topic.contains("..") || topic.contains('/') || topic.contains('\\') {
                return "Invalid topic slug".to_string();
            }

            // Write memory/{topic}.md
            let memory_dir = ws.path().join("memory");
            if let Err(e) = tokio::fs::create_dir_all(&memory_dir).await {
                return format!("Failed to create memory directory: {e}");
            }
            let topic_path = memory_dir.join(format!("{topic}.md"));
            if let Err(e) = tokio::fs::write(&topic_path, content).await {
                return format!("Failed to write memory/{topic}.md: {e}");
            }

            // Update MEMORY.md index
            let memory_index_path = ws.path().join("MEMORY.md");
            let existing_index = tokio::fs::read_to_string(&memory_index_path)
                .await
                .unwrap_or_else(|_| "# Memory Index\n\nTopics stored in `memory/`.\n".to_string());

            let entry_marker = format!("memory/{topic}.md");
            let new_entry = format!("- [{topic}]({entry_marker}) — {summary}");

            let updated_index = if existing_index.contains(&entry_marker) {
                // Replace existing entry line
                existing_index
                    .lines()
                    .map(|line| {
                        if line.contains(&entry_marker) {
                            new_entry.as_str()
                        } else {
                            line
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                // Append new entry
                format!("{}\n{}", existing_index.trim_end(), new_entry)
            };

            if let Err(e) = tokio::fs::write(&memory_index_path, &updated_index).await {
                return format!("Wrote memory/{topic}.md but failed to update MEMORY.md: {e}");
            }

            info!(topic = %topic, "Saved memory");
            format!("Saved memory/{topic}.md and updated MEMORY.md index")
        }
        "recall_memory" => {
            let query = call.arguments["query"].as_str().unwrap_or("");
            if query.is_empty() {
                return "No query provided".to_string();
            }

            let memory_dir = ws.path().join("memory");
            let mut results = Vec::new();

            // Also check MEMORY.md index
            let memory_index = tokio::fs::read_to_string(ws.path().join("MEMORY.md"))
                .await
                .unwrap_or_default();
            if !memory_index.is_empty() {
                results.push(format!("## MEMORY.md Index\n{memory_index}"));
            }

            // Scan all memory files
            let mut entries = match tokio::fs::read_dir(&memory_dir).await {
                Ok(e) => e,
                Err(_) => {
                    if results.is_empty() {
                        return "No memory files found. Memory is empty.".to_string();
                    }
                    return results.join("\n\n---\n\n");
                }
            };

            let query_lower = query.to_lowercase();
            let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }

                let filename = path.file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let content = match tokio::fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let content_lower = content.to_lowercase();
                let filename_lower = filename.to_lowercase();

                // Match if any query term appears in filename or content
                let matches = query_terms.iter().any(|term| {
                    filename_lower.contains(term) || content_lower.contains(term)
                });

                if matches {
                    // Truncate long files
                    let display = if content.len() > 3000 {
                        format!("{}...\n[truncated, {} bytes total]", &content[..3000], content.len())
                    } else {
                        content
                    };
                    results.push(format!("## memory/{filename}.md\n{display}"));
                }
            }

            if results.is_empty() {
                format!("No memory entries found matching '{query}'")
            } else {
                info!(query = %query, matches = results.len(), "Memory recall");
                results.join("\n\n---\n\n")
            }
        }
        "log_decision" => {
            let decision = call.arguments["decision"].as_str().unwrap_or("");
            let reasoning = call.arguments["reasoning"].as_str().unwrap_or("");
            let participants = call.arguments["participants"].as_str().unwrap_or("");
            let context = call.arguments["context"].as_str().unwrap_or("unknown");
            if decision.is_empty() {
                return "No decision provided".to_string();
            }

            let decisions_path = ws.path().join("memory").join("decisions.md");
            let _ = tokio::fs::create_dir_all(ws.path().join("memory")).await;

            let existing = tokio::fs::read_to_string(&decisions_path)
                .await
                .unwrap_or_else(|_| "# Decision Log\n\nDecisions captured from team conversations.\n".to_string());

            let date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let new_entry = format!(
                "\n---\n\n### {} ({})\n\n\
                 **Decision:** {}\n\
                 **Reasoning:** {}\n\
                 **Participants:** {}\n\
                 **Context:** {}\n",
                decision.chars().take(60).collect::<String>(),
                date,
                decision,
                reasoning,
                participants,
                context,
            );

            let updated = format!("{}{}", existing.trim_end(), new_entry);
            match tokio::fs::write(&decisions_path, &updated).await {
                Ok(_) => {
                    info!(decision = %decision, "Logged decision");

                    // Also update MEMORY.md index if decisions.md isn't already listed
                    let memory_index_path = ws.path().join("MEMORY.md");
                    let index = tokio::fs::read_to_string(&memory_index_path)
                        .await
                        .unwrap_or_default();
                    if !index.contains("memory/decisions.md") {
                        let updated_index = format!(
                            "{}\n- [decisions](memory/decisions.md) — Team decisions captured from conversations",
                            index.trim_end()
                        );
                        let _ = tokio::fs::write(&memory_index_path, &updated_index).await;
                    }

                    format!("Decision logged: {}", &decision[..decision.len().min(80)])
                }
                Err(e) => format!("Failed to log decision: {e}"),
            }
        }
        "update_intents" => {
            let content = call.arguments["content"].as_str().unwrap_or("");
            let reason = call.arguments["reason"].as_str().unwrap_or("no reason given");
            if content.is_empty() {
                return "No content provided".to_string();
            }

            let intents_path = ws.path().join("INTENTS.md");

            // Log the change reason for auditability
            info!(reason = %reason, "Updating INTENTS.md");
            logger::append_log(
                ws.path(),
                "internal",
                "delegate-bot",
                &format!("[intents-update] {reason}"),
            )
            .await
            .ok();

            match tokio::fs::write(&intents_path, content).await {
                Ok(_) => format!("INTENTS.md updated. Reason: {reason}"),
                Err(e) => format!("Failed to update INTENTS.md: {e}"),
            }
        }
        other => {
            warn!(tool = %other, "Unknown tool call");
            format!("Unknown tool: {other}")
        }
    }
}

/// Write a proposed action to the pending/ audit trail.
/// Per ARCHITECTURE.md: file is never deleted — permanent record of what was proposed.
async fn write_pending_action(
    ws: &Workspace,
    call: &ToolCall,
    event: &DelegateEvent,
) -> Result<()> {
    let pending_dir = ws.path().join("pending");
    tokio::fs::create_dir_all(&pending_dir).await?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = call.name.replace('_', "-");
    let filename = format!("{timestamp}-{slug}.md");

    let content = format!(
        "# Pending Action: {}\n\n\
         **Time:** {}\n\
         **Tool:** {}\n\
         **Trigger channel:** {}\n\
         **Trigger user:** {}\n\
         **Trigger content:** {}\n\n\
         ## Arguments\n\n\
         ```json\n{}\n```\n\n\
         ## Status\n\n\
         Executed (dogfooding mode — approval workflow not yet active)\n",
        call.name,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        call.name,
        event.channel,
        event.user,
        event.content,
        serde_json::to_string_pretty(&call.arguments).unwrap_or_default(),
    );

    tokio::fs::write(pending_dir.join(&filename), &content).await?;
    info!(file = %filename, tool = %call.name, "Wrote pending action audit trail");

    Ok(())
}

/// Background heartbeat loop.
/// Wakes at configured interval, scans daily log for new entries since last tick.
/// If new entries exist, runs batched reasoning through the intent lens.
/// Also checks cron schedules and fires scheduled outputs.
async fn run_heartbeat(
    ws: &Workspace,
    client: &ModelClient,
    slack: &SlackSocket,
    model_override: Option<&str>,
    budget: &TokenBudget,
) -> Result<()> {
    // Initial delay to let the bot start up and load context
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let mut last_log_line: usize = 0;
    let mut last_cron_fire: std::collections::HashMap<String, chrono::DateTime<chrono::Local>> =
        std::collections::HashMap::new();

    info!("Heartbeat loop started");

    loop {
        // Re-read config each tick so HEARTBEAT.md changes take effect immediately
        let config = heartbeat::parse_config(ws.path()).await;
        let interval = std::time::Duration::from_secs(config.interval_secs);

        // Update budget limit in case HEARTBEAT.md changed it
        budget.set_limit(config.daily_token_budget).await;

        // --- Check for new log entries ---
        let (new_entries, current_line) =
            heartbeat::read_log_since(ws.path(), last_log_line).await;

        if !new_entries.is_empty() {
            let has_queued = new_entries.contains("[queued]");
            let entry_count = current_line - last_log_line;

            // Only invoke LLM if there are queued entries or significant activity (5+ entries)
            if !has_queued && entry_count < 5 {
                debug!(
                    new_lines = entry_count,
                    "Heartbeat: new entries but nothing queued, skipping reasoning"
                );
                last_log_line = current_line;
                tokio::time::sleep(interval).await;
                continue;
            }

            // Budget check before heartbeat reasoning
            if !budget.is_available().await {
                debug!("Heartbeat: skipping reasoning, token budget exhausted");
                last_log_line = current_line;
                tokio::time::sleep(interval).await;
                continue;
            }

            info!(
                new_lines = entry_count,
                queued = has_queued,
                "Heartbeat: processing batch"
            );

            // Batched reasoning: assemble context with Digest task type
            let recent_logs = logger::read_recent_logs(ws.path()).await;

            // Count queued vs total entries for the prompt
            let queued_count = new_entries.lines().filter(|l| l.contains("[queued]")).count();
            let total_count = current_line - last_log_line;

            // Build a synthetic event for the heartbeat tick
            let heartbeat_event = DelegateEvent {
                id: "heartbeat".to_string(),
                event_type: "heartbeat".to_string(),
                channel: "internal".to_string(),
                user: "system".to_string(),
                content: format!(
                    "Heartbeat tick. {total_count} new log entries since last check ({queued_count} queued for batch review).\n\n\
                     Review these entries as a batch. Look for:\n\
                     - Patterns across multiple signals (same topic from different people/channels)\n\
                     - Queued items [queued] that individually seemed routine but together suggest something worth flagging\n\
                     - Connections to active intents that weren't obvious at triage time\n\
                     - Anything that warrants proactive action\n\n\
                     If nothing stands out, say \"No patterns detected\" — don't force insights.\n\n\
                     Entries:\n{new_entries}"
                ),
                timestamp: chrono::Local::now().format("%s").to_string(),
                thread_ts: None,
                raw: serde_json::json!({}),
            };

            let compiled = context::compile(
                &heartbeat_event,
                ws.path(),
                context::TaskType::Digest,
                &recent_logs,
                config.qa_token_budget,
            )
            .await;

            match compiled {
                Ok(ctx) => {
                    let (system, prompt) = context::to_prompt(&ctx);

                    let response = client
                        .complete(models::CompleteOptions {
                            system,
                            prompt,
                            model: model_override.map(|s| s.to_string()),
                            max_tokens: Some(1024),
                            temperature: Some(0.5),
                            tools: None, // Heartbeat observes, doesn't act (yet)
                        })
                        .await;

                    match response {
                        Ok(resp) => {
                            budget
                                .record(resp.input_tokens + resp.output_tokens)
                                .await;
                            if !resp.content.is_empty() {
                                info!(
                                    tokens = resp.input_tokens + resp.output_tokens,
                                    "Heartbeat reasoning complete"
                                );
                                // Log the heartbeat's observations
                                logger::append_log(
                                    ws.path(),
                                    "internal",
                                    "delegate-heartbeat",
                                    &format!("[heartbeat] {}", resp.content),
                                )
                                .await
                                .ok();
                            }
                        }
                        Err(e) => {
                            warn!("Heartbeat LLM call failed: {e}");
                        }
                    }
                }
                Err(e) => {
                    warn!("Heartbeat context assembly failed: {e}");
                }
            }
        } else {
            debug!("Heartbeat: no-op, no new entries");
        }

        last_log_line = current_line;

        // --- Check cron schedules ---
        let now = chrono::Local::now();
        for job in &config.cron_jobs {
            if heartbeat::should_fire(job, &now) {
                // Prevent double-firing within the same window
                if let Some(last) = last_cron_fire.get(&job.name) {
                    if (now - *last).num_seconds().unsigned_abs() < config.interval_secs {
                        continue;
                    }
                }

                // Budget check for cron
                if !budget.is_available().await {
                    debug!(job = %job.name, "Skipping cron, token budget exhausted");
                    continue;
                }

                info!(job = %job.name, channel = %job.channel, "Cron job firing");

                let task_type = match job.output_type.as_str() {
                    "update" => context::TaskType::Update,
                    _ => context::TaskType::Digest,
                };

                let recent_logs = logger::read_recent_logs(ws.path()).await;
                let cron_event = DelegateEvent {
                    id: format!("cron-{}", job.name),
                    event_type: "cron".to_string(),
                    channel: job.channel.clone(),
                    user: "system".to_string(),
                    content: format!("Scheduled output: {}. Compile and post.", job.name),
                    timestamp: now.format("%s").to_string(),
                    thread_ts: None,
                    raw: serde_json::json!({}),
                };

                let compiled = context::compile(
                    &cron_event,
                    ws.path(),
                    task_type,
                    &recent_logs,
                    config.qa_token_budget,
                )
                .await;

                match compiled {
                    Ok(ctx) => {
                        let (system, prompt) = context::to_prompt(&ctx);

                        match client
                            .complete(models::CompleteOptions {
                                system,
                                prompt,
                                model: model_override.map(|s| s.to_string()),
                                max_tokens: Some(2048),
                                temperature: Some(0.5),
                                tools: None,
                            })
                            .await
                        {
                            Ok(resp) if !resp.content.is_empty() => {
                                budget
                                    .record(resp.input_tokens + resp.output_tokens)
                                    .await;
                                // Post to the target channel
                                if let Err(e) = slack
                                    .post_message(&job.channel, &resp.content, None)
                                    .await
                                {
                                    warn!(job = %job.name, "Failed to post cron output: {e}");
                                } else {
                                    info!(job = %job.name, "Cron output posted");
                                    logger::append_log(
                                        ws.path(),
                                        &job.channel,
                                        "delegate-cron",
                                        &format!("[{}] {}", job.name, &resp.content[..resp.content.len().min(200)]),
                                    )
                                    .await
                                    .ok();
                                }
                            }
                            Ok(_) => {
                                debug!(job = %job.name, "Cron produced empty output");
                            }
                            Err(e) => {
                                warn!(job = %job.name, "Cron LLM call failed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(job = %job.name, "Cron context assembly failed: {e}");
                    }
                }

                last_cron_fire.insert(job.name.clone(), now);
            }
        }

        tokio::time::sleep(interval).await;
    }
}

