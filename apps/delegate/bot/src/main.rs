mod approval;
mod budget;
mod context;
mod event;
mod heartbeat;
mod logger;
mod messenger;
mod models;
mod registry;
mod retriever;
mod slack;
mod text;
mod tool_loop;
mod tools;
mod triage;
mod workspace;

use anyhow::{anyhow, Result};
use chrono::Datelike;
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use budget::TokenBudget;
use context::TaskType;
use event::DelegateEvent;
use messenger::{ChannelId, Messenger, MessageTs, Transport, UserId};
use models::{ChatOptions, CompleteOptions, ModelClient};
use registry::{classify_action, is_information_tool, is_reply_tool, ActionTier, ToolScope};
use slack::SlackSocket;
use tool_loop::ToolLoopConfig;
use tools::{summarize_action, ToolContext};
use triage::TriageLabel;
use workspace::Workspace;

const MAX_TOOL_TURNS: usize = 5;

type ValidateId = Arc<dyn Fn(&str) -> bool + Send + Sync>;

struct Config {
    transport: String,
    provider: String,
    model: Option<String>,
    workspace_path: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let transport =
            std::env::var("DELEGATE_TRANSPORT").unwrap_or_else(|_| "slack".to_string());
        let provider =
            std::env::var("DELEGATE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
        let model = std::env::var("DELEGATE_MODEL").ok();
        let workspace_path =
            std::env::var("DELEGATE_WORKSPACE").unwrap_or_else(|_| "./workspace".to_string());

        Ok(Self {
            transport,
            provider,
            model,
            workspace_path,
        })
    }
}

/// Build transport and messenger from the configured transport name.
/// Each transport reads its own env vars inside the factory match arm.
/// Returns (Transport, Messenger) — both Arcs from the same concrete instance.
fn build_transport(name: &str) -> Result<(Arc<dyn Transport>, Arc<dyn Messenger>)> {
    match name {
        "slack" => {
            let app_token = std::env::var("SLACK_APP_TOKEN")
                .map_err(|_| anyhow!("SLACK_APP_TOKEN not set (xapp-... token)"))?;
            let bot_token = std::env::var("SLACK_BOT_TOKEN")
                .map_err(|_| anyhow!("SLACK_BOT_TOKEN not set (xoxb-... token)"))?;
            let bot_user_id = std::env::var("SLACK_BOT_USER_ID")
                .map_err(|_| anyhow!("SLACK_BOT_USER_ID not set — required to prevent self-reply loops"))?;
            let slack = SlackSocket::new(app_token, bot_token, bot_user_id);
            let transport: Arc<dyn Transport> = Arc::new(slack.clone());
            let messenger: Arc<dyn Messenger> = Arc::new(slack);
            Ok((transport, messenger))
        }
        other => Err(anyhow!("Unknown transport: {other}. Supported: slack")),
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

    dotenvy::dotenv().ok();
    info!("Delegate Bot starting...");

    let config = Config::from_env()?;
    let ws = Workspace::new(&config.workspace_path);
    let model_client = ModelClient::new(&config.provider)?;

    let (transport, messenger) = build_transport(&config.transport)?;

    let hb_config = heartbeat::parse_config(ws.path(), &|id| transport.is_valid_user_id(id)).await;
    let token_budget = TokenBudget::new(hb_config.daily_token_budget);

    // Resolve watched channel names → IDs for Tier 0 filtering
    let watched_channels = resolve_watched_channels(&ws, &*messenger).await;
    let watched_channels = Arc::new(watched_channels);

    // Resolve notification channel ID at startup
    let notification_channel: Arc<Option<String>> =
        if let Some(ref ch_name) = hb_config.notification_channel {
            Arc::new(messenger.resolve_channel_id(ch_name).await)
        } else {
            Arc::new(None)
        };

    info!(
        daily_budget = hb_config.daily_token_budget,
        "Listening for events..."
    );

    let (event_tx, mut event_rx) = mpsc::channel::<Value>(100);

    let listener_handle = {
        let transport_for_listener = transport.clone();
        tokio::spawn(async move {
            if let Err(e) = transport_for_listener.listen(event_tx).await {
                error!("Transport listener failed: {e}");
            }
        })
    };

    let validate_id: ValidateId = {
        let t = transport.clone();
        Arc::new(move |id: &str| t.is_valid_user_id(id))
    };

    let heartbeat_handle = {
        let hb_messenger = messenger.clone();
        let hb_client = model_client.clone();
        let hb_ws = ws.clone();
        let hb_model = config.model.clone();
        let hb_budget = token_budget.clone();
        let hb_validate = validate_id.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_heartbeat(&hb_ws, &hb_client, &*hb_messenger, hb_model.as_deref(), &hb_budget, &hb_validate)
                    .await
            {
                error!("Heartbeat loop failed: {e}");
            }
        })
    };

    let cron_handle = {
        let cron_messenger = messenger.clone();
        let cron_client = model_client.clone();
        let cron_ws = ws.clone();
        let cron_model = config.model.clone();
        let cron_budget = token_budget.clone();
        let cron_validate = validate_id.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_cron_scheduler(&cron_ws, &cron_client, &*cron_messenger, cron_model.as_deref(), &cron_budget, &cron_validate)
                    .await
            {
                error!("Cron scheduler failed: {e}");
            }
        })
    };

    info!("Event loop running. Waiting for events...");

    // Dedup cache
    let seen_events: Arc<Mutex<(HashSet<String>, VecDeque<String>)>> =
        Arc::new(Mutex::new((HashSet::new(), VecDeque::new())));
    const DEDUP_CACHE_SIZE: usize = 200;

    while let Some(envelope) = event_rx.recv().await {
        let evt = match transport.normalize_event(&envelope) {
            Some(e) => e,
            None => continue,
        };

        // Dedup
        let dedup_key = format!("{}:{}", evt.channel, evt.timestamp);
        {
            let mut cache = seen_events.lock().await;
            if cache.0.contains(&dedup_key) {
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

        let transport_for_event = transport.clone();
        let messenger_for_event = messenger.clone();
        let client = model_client.clone();
        let ws_clone = ws.clone();
        let model_override = config.model.clone();
        let evt_budget = token_budget.clone();
        let evt_watched = watched_channels.clone();
        let evt_notif_channel = notification_channel.clone();

        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                handle_event(
                    evt,
                    &*transport_for_event,
                    &*messenger_for_event,
                    &client,
                    &ws_clone,
                    model_override.as_deref(),
                    &evt_budget,
                    evt_watched.as_ref().as_ref(),
                    evt_notif_channel.as_ref().as_deref(),
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

    tokio::select! {
        r = listener_handle => { r?; }
        r = heartbeat_handle => { r?; }
        r = cron_handle => { r?; }
    }
    Ok(())
}

/// Resolve watched channel names from HEARTBEAT.md to channel IDs.
async fn resolve_watched_channels(
    ws: &Workspace,
    messenger: &dyn Messenger,
) -> Option<HashSet<String>> {
    let names = ws.watched_channels().await;
    if names.is_empty() {
        return None;
    }
    let mut ids = HashSet::new();
    for name in &names {
        if let Some(id) = messenger.resolve_channel_id(name).await {
            ids.insert(id);
        } else {
            warn!(channel = %name, "Could not resolve watched channel name to ID");
        }
    }
    if ids.is_empty() {
        None
    } else {
        info!(channels = ?ids, "Watching specific channels");
        Some(ids)
    }
}

/// Notify that the daily token budget has been exhausted.
async fn notify_budget_exhausted(
    messenger: &dyn Messenger,
    budget: &TokenBudget,
    fallback_channel: &str,
    notification_channel: Option<&str>,
    ws: &Workspace,
) {
    if budget.was_notified().await {
        return;
    }
    warn!("Daily token budget exhausted, entering log-only mode");
    let msg = "I've used my daily token budget. Entering log-only mode until midnight.";
    let channel = notification_channel.unwrap_or(fallback_channel);
    let _ = messenger.post_message(channel, msg, None).await;
    logger::append_log(
        ws.path(),
        "internal",
        "delegate-bot",
        "[budget-exhausted] Entering log-only mode until midnight",
    )
    .await
    .ok();
    budget.mark_notified().await;
}

// ── Event handler ──────────────────────────────────────────────────────

#[tracing::instrument(
    skip(transport, messenger, client, ws, budget, watched_channels, notification_channel),
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
    transport: &dyn Transport,
    messenger: &dyn Messenger,
    client: &ModelClient,
    ws: &Workspace,
    model_override: Option<&str>,
    budget: &TokenBudget,
    watched_channels: Option<&HashSet<String>>,
    notification_channel: Option<&str>,
) -> Result<()> {
    let event_start = std::time::Instant::now();
    let mut event_tokens: u64 = 0;

    // Fast-path: approval reactions
    if event.event_type == "reaction_added" {
        approval::handle_reaction(&event, messenger, ws).await?;
        return Ok(());
    }

    // Triage
    let triage_tokens = run_triage(&event, transport, watched_channels, ws, budget, client).await?;
    if triage_tokens.is_none() {
        return Ok(()); // event was filtered out
    }
    event_tokens += triage_tokens.unwrap();

    // Budget check for full reasoning
    if !budget.is_available().await {
        logger::append_log(ws.path(), event.channel.as_str(), event.user.as_str(), &format!("[log-only] {}", event.content)).await?;
        notify_budget_exhausted(messenger, budget, event.channel.as_str(), notification_channel, ws).await;
        return Ok(());
    }

    // Assemble context
    let thread_ts = event.thread_ts.as_deref().unwrap_or(event.timestamp.as_str());
    let user_name = messenger.get_user_name(event.user.as_str()).await;
    let thread_context = fetch_thread_context(&event, messenger, thread_ts).await;
    let channel_name = messenger.get_channel_name(event.channel.as_str()).await;
    let recent_logs = logger::read_recent_logs(ws.path()).await;

    let is_dm = transport.is_dm_channel(event.channel.as_str());
    let mut compiled = context::compile(
        &event, ws.path(), TaskType::Respond, &recent_logs, 8000, Some(&channel_name), is_dm, ToolScope::Event,
    ).await?;

    let clean_content = transport.strip_mentions(&event.content);

    compiled.trigger = format!(
        "Channel: #{channel_name}\nFrom: {user_name} (ID: {})\nTime: {}\n\n{clean_content}",
        event.user, event.timestamp
    );

    let (system_prompt, mut user_prompt) = context::to_prompt(&compiled, ToolScope::Event);
    if !thread_context.is_empty() {
        user_prompt = format!("{thread_context}\n\n---\nNew message:\n{user_prompt}");
    }

    // Initial LLM call
    let tools = registry::event_tool_schemas();
    let model = model_override.map(|s| s.to_string());

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

    event_tokens += response.input_tokens + response.output_tokens;
    let within_budget = budget.record(response.input_tokens + response.output_tokens).await;
    if !within_budget {
        notify_budget_exhausted(messenger, budget, event.channel.as_str(), notification_channel, ws).await;
    }

    // Multi-turn tool loop with approval workflow
    let hb_config = heartbeat::parse_config(ws.path(), &|id| transport.is_valid_user_id(id)).await;
    let ctx = ToolContext { messenger, ws, event: &event, thread_ts };

    let (final_content, has_reply, action_summaries, silent_actions, loop_tokens) =
        run_event_tool_loop(
            response,
            &user_prompt,
            &system_prompt,
            &model,
            &tools,
            client,
            &ctx,
            budget,
            &hb_config,
        )
        .await;

    event_tokens += loop_tokens;

    // Post text content if model didn't use reply tool
    if !final_content.is_empty() && !has_reply {
        messenger.post_message(event.channel.as_str(), &final_content, Some(thread_ts)).await?;
    }

    // Post italic notice for silent tool executions
    if !silent_actions.is_empty() && !has_reply {
        let notice = format!("_{}_", silent_actions.join("; "));
        let _ = messenger.post_message(event.channel.as_str(), &notice, Some(thread_ts)).await;
    }

    // Log the interaction
    logger::append_log(ws.path(), &channel_name, &user_name, &clean_content).await?;
    if !action_summaries.is_empty() {
        logger::append_log(
            ws.path(),
            &channel_name,
            "delegate-bot",
            &format!("[{}]", action_summaries.join("; ")),
        )
        .await?;
    }

    let span = tracing::Span::current();
    span.record("total_tokens", event_tokens);
    span.record("tool_count", action_summaries.len());

    info!(
        duration_ms = event_start.elapsed().as_millis() as u64,
        tokens = event_tokens,
        "Event handling complete"
    );

    Ok(())
}

/// Run triage (Tier 0 + Tier 1). Returns Some(tokens_used) if event should be processed,
/// None if filtered out.
async fn run_triage(
    event: &DelegateEvent,
    transport: &dyn Transport,
    watched_channels: Option<&HashSet<String>>,
    ws: &Workspace,
    budget: &TokenBudget,
    client: &ModelClient,
) -> Result<Option<u64>> {
    if let Some(label) = triage::tier0_classify(event, transport, watched_channels) {
        match label {
            TriageLabel::Ignore => return Ok(None),
            TriageLabel::ActNow => return Ok(Some(0)),
            TriageLabel::Queue => {
                logger::append_log(
                    ws.path(), event.channel.as_str(), event.user.as_str(),
                    &format!("[queued] {}", event.content),
                ).await?;
                return Ok(None);
            }
        }
    }

    // Tier 1 (LLM-based) — requires budget
    if !budget.is_available().await {
        logger::append_log(
            ws.path(), event.channel.as_str(), event.user.as_str(),
            &format!("[log-only] {}", event.content),
        ).await?;
        return Ok(None);
    }

    let raw_intents = ws.intents().await;
    let intent_summary = context::compress_intents(&raw_intents, 500);
    let (label, reasoning, triage_tokens) =
        triage::tier1_classify(event, &intent_summary, client, None).await?;

    budget.record(triage_tokens).await;
    info!(label = %label, reasoning = %reasoning, triage_tokens, "Tier 1 triage result");

    match label {
        TriageLabel::Ignore => {
            logger::append_log(ws.path(), event.channel.as_str(), event.user.as_str(), &event.content).await?;
            Ok(None)
        }
        TriageLabel::Queue => {
            logger::append_log(
                ws.path(), event.channel.as_str(), event.user.as_str(),
                &format!("[queued] {}", event.content),
            ).await?;
            Ok(None)
        }
        TriageLabel::ActNow => Ok(Some(triage_tokens)),
    }
}

/// Fetch thread context for threaded messages.
async fn fetch_thread_context(
    event: &DelegateEvent,
    messenger: &dyn Messenger,
    thread_ts: &str,
) -> String {
    if event.thread_ts.is_none() {
        return String::new();
    }
    let messages = match messenger.get_thread(event.channel.as_str(), thread_ts).await {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    let mut lines = Vec::new();
    for msg in &messages {
        if msg.timestamp != *event.timestamp {
            let name = messenger.get_user_name(&msg.user_id).await;
            lines.push(format!("<{name}> {}", msg.text));
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("Thread history (oldest first):\n{}", lines.join("\n"))
    }
}

/// Event handler's tool loop — includes approval workflow and reply/action tracking.
/// Returns (final_content, has_reply, action_summaries, silent_actions, tokens_used).
async fn run_event_tool_loop(
    initial_response: models::ModelResponse,
    user_prompt: &str,
    system: &str,
    model: &Option<String>,
    tools: &[Value],
    client: &ModelClient,
    ctx: &ToolContext<'_>,
    budget: &TokenBudget,
    hb_config: &heartbeat::HeartbeatConfig,
) -> (String, bool, Vec<String>, Vec<String>, u64) {
    let mut conversation: Vec<Value> = vec![
        serde_json::json!({"role": "user", "content": user_prompt}),
    ];
    let mut current_response = initial_response;
    let mut tokens_used: u64 = 0;
    let mut has_reply = false;
    let mut action_summaries: Vec<String> = Vec::new();
    let mut silent_actions: Vec<String> = Vec::new();
    let mut turn = 0;

    loop {
        if current_response.tool_calls.is_empty() {
            break;
        }

        let mut tool_results: Vec<(String, String)> = Vec::new();
        let mut needs_followup = false;

        for call in &current_response.tool_calls {
            let tier = classify_action(&call.name);

            // Approval workflow for RequiresApproval tools
            if tier == ActionTier::RequiresApproval {
                if let Some(deferred) = try_defer_for_approval(call, ctx, hb_config).await {
                    tool_results.push((call.id.clone(), deferred.tool_result));
                    has_reply = true;
                    action_summaries.push(deferred.summary);
                    continue;
                }
                // No approver configured or DM failed — execute with audit trail
                let _ = approval::write_audit_trail(ctx.ws, call, ctx.event).await;
            }

            let result = tools::execute_tool(call, ctx).await;

            if is_reply_tool(&call.name) {
                has_reply = true;
            }
            if is_information_tool(&call.name) {
                needs_followup = true;
            }
            if tier == ActionTier::AutonomousWithNotice && !is_reply_tool(&call.name) && call.name != "post" {
                silent_actions.push(summarize_action(call, &result));
            }

            action_summaries.push(summarize_action(call, &result));
            tool_results.push((call.id.clone(), result));
        }

        turn += 1;
        if !needs_followup || turn >= MAX_TOOL_TURNS || has_reply {
            break;
        }

        // Continue multi-turn conversation
        conversation.push(current_response.raw_assistant_message.clone());
        for (id, result) in &tool_results {
            conversation.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": result
            }));
        }

        if !budget.is_available().await {
            break;
        }

        match client
            .chat(ChatOptions {
                system: system.to_string(),
                messages: conversation.clone(),
                model: model.clone(),
                max_tokens: Some(2048),
                temperature: Some(0.7),
                tools: Some(tools.to_vec()),
            })
            .await
        {
            Ok(resp) => {
                let t = resp.input_tokens + resp.output_tokens;
                tokens_used += t;
                budget.record(t).await;
                current_response = resp;
            }
            Err(e) => {
                warn!("Event tool loop LLM call failed: {e}");
                break;
            }
        }
    }

    (current_response.content.clone(), has_reply, action_summaries, silent_actions, tokens_used)
}

/// Result of attempting to defer a tool call for approval.
struct ApprovalDeferral {
    tool_result: String,
    summary: String,
}

/// Try to defer a tool call to the approval workflow. Returns Some if deferred, None if
/// execution should proceed normally (no approver configured or DM failed).
async fn try_defer_for_approval(
    call: &models::ToolCall,
    ctx: &ToolContext<'_>,
    hb_config: &heartbeat::HeartbeatConfig,
) -> Option<ApprovalDeferral> {
    let approver_id = hb_config.default_approver.as_deref()?;

    let pending = approval::PendingAction::new(
        &call.name,
        &call.arguments,
        &ctx.event.user,
        &ctx.event.channel,
        &ctx.event.timestamp,
        ctx.event.thread_ts.as_deref(),
        approver_id,
        hb_config.backup_approver.as_deref(),
        hb_config.approval_timeout_secs,
    );

    let approver_name = ctx.messenger.get_user_name(approver_id).await;
    let approval_msg = format!(
        "Approval request from Delegate:\n\
         *Tool:* `{}`\n\
         *Args:* ```{}```\n\
         *Triggered by:* <@{}> in <#{}>\n\n\
         React with :white_check_mark: to approve or :x: to reject.\n\
         Expires in {} hours.",
        call.name,
        serde_json::to_string_pretty(&call.arguments).unwrap_or_default(),
        ctx.event.user,
        ctx.event.channel,
        hb_config.approval_timeout_secs / 3600,
    );

    let sent = match ctx.messenger.send_dm(approver_id, &approval_msg).await {
        Ok(s) => s,
        Err(e) => {
            // Policy: if DM fails, fall through to immediate execution
            warn!(error = %e, "Approval DM failed, executing tool immediately");
            return None;
        }
    };

    let mut pending = pending;
    pending.dm_channel = Some(sent.channel);
    pending.dm_ts = Some(sent.timestamp);
    if let Err(e) = approval::save_pending(ctx.ws.path(), &pending).await {
        warn!(error = %e, "Failed to save pending approval");
        return None;
    }

    // Notify the triggering thread
    let _ = ctx.messenger
        .post_message(
            &ctx.event.channel,
            &format!("That needs approval. I've sent a request to {approver_name}."),
            Some(ctx.thread_ts),
        )
        .await;

    Some(ApprovalDeferral {
        tool_result: format!("Approval request sent to {approver_name}. Action deferred."),
        summary: format!("deferred {} (approval sent to {approver_name})", call.name),
    })
}

// ── Heartbeat loop ─────────────────────────────────────────────────────

async fn run_heartbeat(
    ws: &Workspace,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    budget: &TokenBudget,
    validate_id: &ValidateId,
) -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let (_, initial_line) = heartbeat::read_log_since(ws.path(), 0).await;
    let mut last_log_line: usize = initial_line;

    info!("Heartbeat loop started");

    loop {
        let config = heartbeat::parse_config(ws.path(), &**validate_id).await;
        let interval = std::time::Duration::from_secs(config.interval_secs);
        budget.set_limit(config.daily_token_budget).await;

        // Scan pending approvals for timeouts
        approval::scan_timeouts(ws.path(), messenger, &config).await;

        // Check for new log entries
        let (new_entries, current_line) = heartbeat::read_log_since(ws.path(), last_log_line).await;

        if !new_entries.is_empty() {
            process_heartbeat_batch(
                &new_entries,
                current_line - last_log_line,
                ws,
                client,
                messenger,
                model_override,
                budget,
                &config,
            )
            .await;
        }

        last_log_line = current_line;

        tokio::time::sleep(interval).await;
    }
}

/// Process a batch of new log entries during a heartbeat tick.
async fn process_heartbeat_batch(
    new_entries: &str,
    entry_count: usize,
    ws: &Workspace,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    budget: &TokenBudget,
    config: &heartbeat::HeartbeatConfig,
) {
    let has_queued = new_entries.contains("[queued]");

    if !has_queued && entry_count < 5 {
        debug!(new_lines = entry_count, "Heartbeat: nothing queued, skipping");
        return;
    }
    if !budget.is_available().await {
        debug!("Heartbeat: skipping, budget exhausted");
        return;
    }

    info!(new_lines = entry_count, queued = has_queued, "Heartbeat: processing batch");

    let recent_logs = logger::read_recent_logs(ws.path()).await;
    let queued_count = new_entries.lines().filter(|l| l.contains("[queued]")).count();

    let heartbeat_event = DelegateEvent {
        id: "heartbeat".to_string(),
        event_type: "heartbeat".to_string(),
        channel: ChannelId::from("internal"),
        user: UserId::from("system"),
        content: format!(
            "Heartbeat tick. {entry_count} new log entries since last check ({queued_count} queued for batch review).\n\n\
             Review these entries as a batch. Run these checks:\n\n\
             **Cross-channel connections:** Are different people/channels discussing the same topic without knowing it? If so, connect them — post in the relevant channel or thread.\n\n\
             **Stale threads:** Any question asked >2 hours ago with no answer? Any commitment made with no follow-up? Use channel_history to verify before flagging.\n\n\
             **Blocker detection:** Look for language signaling blocks: \"waiting on\", \"blocked by\", \"can't proceed\", \"need X before\". If you spot one, surface it proactively.\n\n\
             **Decision capture:** Did someone make a decision in passing? (\"let's go with X\", \"I think we should\", \"approved\"). Use log_decision to record it.\n\n\
             **Pattern detection:** Do queued items [queued] that individually seemed routine form a pattern together? Same topic from different people = signal.\n\n\
             **Intent alignment:** Do any entries connect to active intents in ways that weren't obvious at triage time?\n\n\
             If nothing stands out, use no_action — don't force insights. Quality over quantity.\n\n\
             Use dm_user only for approval escalations or urgent notifications.\n\n\
             Entries:\n{new_entries}"
        ),
        timestamp: MessageTs::from(chrono::Local::now().format("%s").to_string()),
        thread_ts: None,
        raw: serde_json::json!({}),
    };

    let compiled = match context::compile(
        &heartbeat_event, ws.path(), TaskType::Digest, &recent_logs, config.qa_token_budget, None, false, ToolScope::Heartbeat,
    ).await {
        Ok(ctx) => ctx,
        Err(e) => { warn!("Heartbeat context assembly failed: {e}"); return; }
    };

    let (system, prompt) = context::to_prompt(&compiled, ToolScope::Heartbeat);
    let hb_tools = registry::heartbeat_tool_schemas();

    let response = match client.complete(CompleteOptions {
        system: system.clone(),
        prompt: prompt.clone(),
        model: model_override.map(|s| s.to_string()),
        max_tokens: Some(2048),
        temperature: Some(0.5),
        tools: Some(hb_tools.clone()),
    }).await {
        Ok(r) => r,
        Err(e) => { warn!("Heartbeat LLM call failed: {e}"); return; }
    };

    budget.record(response.input_tokens + response.output_tokens).await;

    let outcome = tool_loop::run_tool_loop(
        response,
        &prompt,
        client,
        messenger,
        ws,
        &heartbeat_event,
        "",
        budget,
        &ToolLoopConfig {
            system,
            model: model_override.map(|s| s.to_string()),
            tools: hb_tools,
            max_turns: 5,
            max_tokens: 2048,
            temperature: 0.5,
        },
    )
    .await;

    if !outcome.final_content.is_empty() {
        info!(tokens = outcome.total_tokens, "Heartbeat reasoning complete");
        logger::append_log(
            ws.path(), "internal", "delegate-heartbeat",
            &format!("[heartbeat] {}", outcome.final_content),
        ).await.ok();
    }
}

// ── Cron scheduler ──────────────────────────────────────────────────────

const CRON_TICK_SECS: u64 = 60;
const CRON_DEDUP_SECS: i64 = 300;
const CRON_CATCHUP_WINDOW_SECS: i64 = 2 * 3600; // 2 hours

/// Load last-fired timestamps from workspace/cron_state.json.
async fn load_last_fired(ws: &Workspace) -> HashMap<String, chrono::DateTime<chrono::Local>> {
    let path = ws.path().join("cron_state.json");
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let map: HashMap<String, String> = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            warn!("Corrupt cron_state.json, starting fresh: {e}");
            return HashMap::new();
        }
    };
    map.into_iter()
        .filter_map(|(k, v)| {
            chrono::DateTime::parse_from_rfc3339(&v)
                .ok()
                .map(|dt| (k, dt.with_timezone(&chrono::Local)))
        })
        .collect()
}

/// Save last-fired timestamps to workspace/cron_state.json.
async fn save_last_fired(ws: &Workspace, state: &HashMap<String, chrono::DateTime<chrono::Local>>) {
    let map: HashMap<&str, String> = state
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_rfc3339()))
        .collect();
    let json = match serde_json::to_string_pretty(&map) {
        Ok(j) => j,
        Err(e) => {
            warn!("Failed to serialize cron state: {e}");
            return;
        }
    };
    if let Err(e) = tokio::fs::write(ws.path().join("cron_state.json"), json).await {
        warn!("Failed to write cron_state.json: {e}");
    }
}

/// Fire any cron jobs that were missed while the bot was down (within a 2-hour window).
async fn check_missed_jobs(
    config: &heartbeat::HeartbeatConfig,
    last_fired: &mut HashMap<String, chrono::DateTime<chrono::Local>>,
    ws: &Workspace,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    budget: &TokenBudget,
) {
    let now = chrono::Local::now();
    let today = now.date_naive();
    let weekday = now.weekday().num_days_from_monday();

    for job in &config.cron_jobs {
        // Skip if wrong day of week
        if !job.days.is_empty() && !job.days.contains(&weekday) {
            continue;
        }

        // Build today's scheduled DateTime for this job
        let scheduled = match today.and_time(job.time).and_local_timezone(chrono::Local) {
            chrono::offset::LocalResult::Single(dt) => dt,
            _ => continue,
        };

        // Only catch up if scheduled time is in the past
        if scheduled > now {
            continue;
        }

        // Only catch up within the 2-hour window
        let elapsed = (now - scheduled).num_seconds();
        if elapsed > CRON_CATCHUP_WINDOW_SECS {
            continue;
        }

        // Skip if already fired today
        if let Some(last) = last_fired.get(&job.name) {
            if last.date_naive() == today {
                continue;
            }
        }

        info!(job = %job.name, "Catch-up: firing missed cron job");
        run_cron_job(job, ws, client, messenger, model_override, budget, config).await;
        last_fired.insert(job.name.clone(), now);
    }

    save_last_fired(ws, last_fired).await;
}

/// Independent cron scheduler loop with 60-second tick.
async fn run_cron_scheduler(
    ws: &Workspace,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    budget: &TokenBudget,
    validate_id: &ValidateId,
) -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let mut last_fired = load_last_fired(ws).await;

    // Startup catch-up: fire any missed jobs
    let startup_config = heartbeat::parse_config(ws.path(), &**validate_id).await;
    check_missed_jobs(
        &startup_config,
        &mut last_fired,
        ws,
        client,
        messenger,
        model_override,
        budget,
    )
    .await;

    info!("Cron scheduler started (tick: {CRON_TICK_SECS}s)");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(CRON_TICK_SECS)).await;

        let config = heartbeat::parse_config(ws.path(), &**validate_id).await;
        let now = chrono::Local::now();

        for job in &config.cron_jobs {
            if !heartbeat::should_fire(job, &now, CRON_TICK_SECS * 2) {
                continue;
            }

            // Dedup cooldown: skip if fired within CRON_DEDUP_SECS
            if let Some(last) = last_fired.get(&job.name) {
                if (now - *last).num_seconds().unsigned_abs() < CRON_DEDUP_SECS as u64 {
                    continue;
                }
            }

            run_cron_job(job, ws, client, messenger, model_override, budget, &config).await;
            last_fired.insert(job.name.clone(), now);
            save_last_fired(ws, &last_fired).await;
        }
    }
}

/// Run a single cron job.
async fn run_cron_job(
    job: &heartbeat::CronJob,
    ws: &Workspace,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    budget: &TokenBudget,
    config: &heartbeat::HeartbeatConfig,
) {
    if !budget.is_available().await {
        debug!(job = %job.name, "Skipping cron, budget exhausted");
        return;
    }

    info!(job = %job.name, channel = %job.channel, "Cron job firing");

    let task_type = match job.output_type.as_str() {
        "update" => TaskType::Update,
        _ => TaskType::Digest,
    };

    let channel_id = match messenger.resolve_channel_id(&job.channel).await {
        Some(id) => id,
        None => {
            warn!(job = %job.name, channel = %job.channel, "Could not resolve channel, skipping cron");
            return;
        }
    };

    let now = chrono::Local::now();
    let recent_logs = logger::read_recent_logs(ws.path()).await;
    let cron_event = DelegateEvent {
        id: format!("cron-{}", job.name),
        event_type: "cron".to_string(),
        channel: ChannelId::from(channel_id.as_str()),
        user: UserId::from("system"),
        content: format!("Scheduled output: {}. Compile and post.", job.name),
        timestamp: MessageTs::from(now.format("%s").to_string()),
        thread_ts: None,
        raw: serde_json::json!({}),
    };

    let compiled = match context::compile(
        &cron_event, ws.path(), task_type, &recent_logs, config.qa_token_budget, Some(&job.channel), false, ToolScope::Heartbeat,
    ).await {
        Ok(ctx) => ctx,
        Err(e) => { warn!(job = %job.name, "Cron context assembly failed: {e}"); return; }
    };

    let (system, prompt) = context::to_prompt(&compiled, ToolScope::Heartbeat);
    let cron_tools = registry::heartbeat_tool_schemas();

    let response = match client.complete(CompleteOptions {
        system: system.clone(),
        prompt: prompt.clone(),
        model: model_override.map(|s| s.to_string()),
        max_tokens: Some(2048),
        temperature: Some(0.5),
        tools: Some(cron_tools.clone()),
    }).await {
        Ok(r) => r,
        Err(e) => { warn!(job = %job.name, "Cron LLM call failed: {e}"); return; }
    };

    budget.record(response.input_tokens + response.output_tokens).await;

    let outcome = tool_loop::run_tool_loop(
        response,
        &prompt,
        client,
        messenger,
        ws,
        &cron_event,
        "",
        budget,
        &ToolLoopConfig {
            system,
            model: model_override.map(|s| s.to_string()),
            tools: cron_tools,
            max_turns: 3,
            max_tokens: 2048,
            temperature: 0.5,
        },
    )
    .await;

    if !outcome.final_content.is_empty() {
        if let Err(e) = messenger.post_message(&channel_id, &outcome.final_content, None).await {
            warn!(job = %job.name, "Failed to post cron output: {e}");
        } else {
            info!(job = %job.name, "Cron output posted");
            logger::append_log(
                ws.path(), &job.channel, "delegate-cron",
                &format!("[{}] {}", job.name, tools::truncate_str(&outcome.final_content, 200)),
            ).await.ok();
        }
    }
}
