use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::db::Db;
use crate::event::DelegateEvent;
use crate::heartbeat::HeartbeatConfig;
use crate::messenger::{ChannelId, Messenger, MessageTs, UserId};
use crate::models::ToolCall;
use crate::workspace::Workspace;

/// Approval state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Escalated,
    Expired,
}

/// A pending action awaiting approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: String,
    pub state: ApprovalState,
    pub tool_name: String,
    pub tool_arguments: Value,
    pub requester: String,
    pub trigger_channel: String,
    pub trigger_ts: String,
    pub thread_ts: Option<String>,
    pub approver: String,
    pub dm_channel: Option<String>,
    pub dm_ts: Option<String>,
    pub created_at: String,
    pub timeout_secs: u64,
    pub backup_approver: Option<String>,
    pub escalated: bool,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

impl PendingAction {
    pub fn new(
        tool_name: &str,
        tool_arguments: &Value,
        requester: &str,
        trigger_channel: &str,
        trigger_ts: &str,
        thread_ts: Option<&str>,
        approver: &str,
        backup_approver: Option<&str>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            state: ApprovalState::Pending,
            tool_name: tool_name.to_string(),
            tool_arguments: tool_arguments.clone(),
            requester: requester.to_string(),
            trigger_channel: trigger_channel.to_string(),
            trigger_ts: trigger_ts.to_string(),
            thread_ts: thread_ts.map(|s| s.to_string()),
            approver: approver.to_string(),
            dm_channel: None,
            dm_ts: None,
            created_at: chrono::Local::now().to_rfc3339(),
            timeout_secs,
            backup_approver: backup_approver.map(|s| s.to_string()),
            escalated: false,
            resolved_at: None,
            resolution_note: None,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&self.created_at) {
            let elapsed = chrono::Local::now()
                .signed_duration_since(created)
                .num_seconds()
                .unsigned_abs();
            elapsed >= self.timeout_secs
        } else {
            false
        }
    }
}

/// Save a pending action to the database.
pub async fn save_pending(db: &Db, action: &PendingAction) -> Result<()> {
    db.save_approval(action).await?;
    info!(id = %action.id, tool = %action.tool_name, state = ?action.state, "Saved pending action");
    Ok(())
}

/// Write a markdown audit trail for actions executed without approval (dogfooding/fallback).
pub async fn write_audit_trail(
    ws: &Workspace,
    call: &ToolCall,
    event: &DelegateEvent,
) -> Result<()> {
    let pending_dir = ws.path().join("pending");
    tokio::fs::create_dir_all(&pending_dir).await?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f").to_string();
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
         Executed (no approver configured — audit trail only)\n",
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

/// Handle a reaction event — check if it matches any pending approval DM.
pub async fn handle_reaction(
    event: &DelegateEvent,
    messenger: &dyn Messenger,
    ws: &Workspace,
    db: &Db,
) -> Result<bool> {
    let reaction = event.content.trim_matches(':');

    let mut action = match db
        .find_approval_by_dm(event.channel.as_str(), event.timestamp.as_str())
        .await?
    {
        Some(a) => a,
        None => return Ok(false),
    };

    if matches!(
        reaction,
        "white_check_mark" | "heavy_check_mark" | "thumbsup" | "+1" | "check"
    ) {
        action.state = ApprovalState::Approved;
        action.resolved_at = Some(chrono::Local::now().to_rfc3339());
        action.resolution_note = Some(format!("Approved by {} via reaction", event.user));
        save_pending(db, &action).await?;

        let tool_call = ToolCall {
            id: format!("approval-{}", action.id),
            name: action.tool_name.clone(),
            arguments: action.tool_arguments.clone(),
        };
        let synthetic_event = DelegateEvent {
            id: action.id.clone(),
            event_type: "approval".to_string(),
            channel: ChannelId::from(action.trigger_channel.as_str()),
            user: UserId::from(action.requester.as_str()),
            content: String::new(),
            timestamp: MessageTs::from(action.trigger_ts.as_str()),
            thread_ts: action.thread_ts.as_deref().map(MessageTs::from),
            raw: Value::Null,
        };
        let thread_ts = action
            .thread_ts
            .as_deref()
            .unwrap_or(&action.trigger_ts);
        let ctx = crate::tools::ToolContext {
            messenger,
            ws,
            event: &synthetic_event,
            thread_ts,
            db,
        };
        let result = crate::tools::execute_tool(&tool_call, &ctx).await;

        let approver_name = messenger.get_user_name(event.user.as_str()).await;
        let _ = messenger
            .post_message(
                &action.trigger_channel,
                &format!("Approved by {approver_name}. Executed `{}`: {result}", action.tool_name),
                Some(thread_ts),
            )
            .await;

        info!(
            id = %action.id,
            tool = %action.tool_name,
            approver = %event.user,
            "Approval granted, tool executed"
        );

        return Ok(true);
    }

    if matches!(reaction, "x" | "thumbsdown" | "-1" | "no_entry_sign") {
        action.state = ApprovalState::Rejected;
        action.resolved_at = Some(chrono::Local::now().to_rfc3339());
        action.resolution_note = Some(format!("Rejected by {} via reaction", event.user));
        save_pending(db, &action).await?;

        let thread_ts = action
            .thread_ts
            .as_deref()
            .unwrap_or(&action.trigger_ts);
        let rejector_name = messenger.get_user_name(event.user.as_str()).await;
        let _ = messenger
            .post_message(
                &action.trigger_channel,
                &format!("Rejected by {rejector_name}. `{}` will not be executed.", action.tool_name),
                Some(thread_ts),
            )
            .await;

        info!(
            id = %action.id,
            tool = %action.tool_name,
            rejector = %event.user,
            "Approval rejected"
        );

        return Ok(true);
    }

    Ok(false)
}

/// Scan pending actions for timeouts. Called from the heartbeat loop.
pub async fn scan_timeouts(
    db: &Db,
    messenger: &dyn Messenger,
    config: &HeartbeatConfig,
) {
    let actions = match db.load_pending_approvals().await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, "Failed to load pending approvals from DB");
            return;
        }
    };

    for mut action in actions {
        if !action.is_timed_out() {
            continue;
        }

        if !action.escalated {
            if let Some(ref backup) = action.backup_approver {
                let backup_id = backup.clone();
                let escalation_msg = format!(
                    "Escalated approval request (original approver didn't respond):\n\
                     *Tool:* `{}`\n\
                     *Args:* ```{}```\n\
                     *Originally requested:* {}\n\n\
                     React with :white_check_mark: to approve or :x: to reject.",
                    action.tool_name,
                    serde_json::to_string_pretty(&action.tool_arguments).unwrap_or_default(),
                    action.created_at,
                );

                match messenger.send_dm(&backup_id, &escalation_msg).await {
                    Ok(sent) => {
                        action.state = ApprovalState::Escalated;
                        action.escalated = true;
                        action.dm_channel = Some(sent.channel);
                        action.dm_ts = Some(sent.timestamp);
                        action.created_at = chrono::Local::now().to_rfc3339();
                        if let Err(e) = save_pending(db, &action).await {
                            warn!(id = %action.id, "Failed to save escalated action: {e}");
                        }
                        info!(id = %action.id, backup = %backup_id, "Escalated approval to backup");
                    }
                    Err(e) => {
                        warn!(id = %action.id, "Failed to DM backup approver: {e}");
                        expire_action(db, messenger, config, &mut action).await;
                    }
                }
            } else {
                expire_action(db, messenger, config, &mut action).await;
            }
        } else {
            expire_action(db, messenger, config, &mut action).await;
        }
    }
}

async fn expire_action(
    db: &Db,
    messenger: &dyn Messenger,
    config: &HeartbeatConfig,
    action: &mut PendingAction,
) {
    action.state = ApprovalState::Expired;
    action.resolved_at = Some(chrono::Local::now().to_rfc3339());
    action.resolution_note = Some("Expired — no response from approver(s)".to_string());
    if let Err(e) = save_pending(db, action).await {
        warn!(id = %action.id, "Failed to save expired action: {e}");
    }

    let notify_msg = format!(
        "Approval expired for `{}` (requested by <@{}>). No approver responded.",
        action.tool_name, action.requester,
    );

    if let Some(ref ch_name) = config.notification_channel {
        if let Some(ch_id) = messenger.resolve_channel_id(ch_name).await {
            let _ = messenger.post_message(&ch_id, &notify_msg, None).await;
        }
    }

    let thread_ts = action.thread_ts.as_deref().unwrap_or(&action.trigger_ts);
    let _ = messenger
        .post_message(
            &action.trigger_channel,
            &format!("Approval expired for `{}`. No approver responded in time.", action.tool_name),
            Some(thread_ts),
        )
        .await;

    info!(id = %action.id, tool = %action.tool_name, "Approval expired");
}
