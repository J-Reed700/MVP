use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use tracing::{info, warn};

use crate::event::DelegateEvent;
use crate::heartbeat::HeartbeatConfig;
use crate::messenger::Messenger;
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

/// Load all pending/escalated actions from workspace/pending/*.json.
/// Used by scan_timeouts (periodic, not per-event).
pub async fn load_pending(workspace: &Path) -> Vec<PendingAction> {
    let pending_dir = workspace.join("pending");
    let mut actions = Vec::new();

    let mut entries = match tokio::fs::read_dir(&pending_dir).await {
        Ok(e) => e,
        Err(_) => return actions,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to read pending action file");
                continue;
            }
        };
        let action: PendingAction = match serde_json::from_str(&content) {
            Ok(a) => a,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to parse pending action JSON");
                continue;
            }
        };
        if action.state == ApprovalState::Pending || action.state == ApprovalState::Escalated {
            actions.push(action);
        }
    }

    actions
}

/// Find a pending action by its DM coordinates (channel, timestamp).
/// More efficient than load_pending for reaction handling — stops at first match.
pub async fn find_by_dm(
    workspace: &Path,
    dm_channel: &str,
    dm_ts: &str,
) -> Option<PendingAction> {
    let pending_dir = workspace.join("pending");
    let mut entries = match tokio::fs::read_dir(&pending_dir).await {
        Ok(e) => e,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let action: PendingAction = match serde_json::from_str(&content) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if (action.state == ApprovalState::Pending || action.state == ApprovalState::Escalated)
            && action.dm_channel.as_deref() == Some(dm_channel)
            && action.dm_ts.as_deref() == Some(dm_ts)
        {
            return Some(action);
        }
    }

    None
}

/// Save a pending action to workspace/pending/{id}.json.
pub async fn save_pending(workspace: &Path, action: &PendingAction) -> Result<()> {
    let pending_dir = workspace.join("pending");
    tokio::fs::create_dir_all(&pending_dir).await?;

    let filename = format!("{}.json", action.id);
    let content = serde_json::to_string_pretty(action)?;
    tokio::fs::write(pending_dir.join(&filename), &content).await?;

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
/// Returns true if the reaction was handled (matched a pending action).
pub async fn handle_reaction(
    event: &DelegateEvent,
    messenger: &dyn Messenger,
    ws: &Workspace,
) -> Result<bool> {
    let reaction = event.content.trim_matches(':');

    // Targeted lookup instead of loading all pending files
    let mut action = match find_by_dm(ws.path(), &event.channel, &event.timestamp).await {
        Some(a) => a,
        None => return Ok(false),
    };

    // Approve reactions
    if matches!(
        reaction,
        "white_check_mark" | "heavy_check_mark" | "thumbsup" | "+1" | "check"
    ) {
        action.state = ApprovalState::Approved;
        action.resolved_at = Some(chrono::Local::now().to_rfc3339());
        action.resolution_note = Some(format!("Approved by {} via reaction", event.user));
        save_pending(ws.path(), &action).await?;

        // Execute the deferred tool
        let tool_call = ToolCall {
            id: format!("approval-{}", action.id),
            name: action.tool_name.clone(),
            arguments: action.tool_arguments.clone(),
        };
        let synthetic_event = DelegateEvent {
            id: action.id.clone(),
            event_type: "approval".to_string(),
            channel: action.trigger_channel.clone(),
            user: action.requester.clone(),
            content: String::new(),
            timestamp: action.trigger_ts.clone(),
            thread_ts: action.thread_ts.clone(),
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
        };
        let result = crate::tools::execute_tool(&tool_call, &ctx).await;

        let approver_name = messenger.get_user_name(&event.user).await;
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

    // Reject reactions
    if matches!(reaction, "x" | "thumbsdown" | "-1" | "no_entry_sign") {
        action.state = ApprovalState::Rejected;
        action.resolved_at = Some(chrono::Local::now().to_rfc3339());
        action.resolution_note = Some(format!("Rejected by {} via reaction", event.user));
        save_pending(ws.path(), &action).await?;

        let thread_ts = action
            .thread_ts
            .as_deref()
            .unwrap_or(&action.trigger_ts);
        let rejector_name = messenger.get_user_name(&event.user).await;
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

    // Unrecognized reaction on an approval DM — ignore
    Ok(false)
}

/// Scan pending actions for timeouts. Called from the heartbeat loop.
/// - Timed out + not escalated → DM backup approver, set Escalated
/// - Timed out + already escalated → set Expired, notify team channel
pub async fn scan_timeouts(
    workspace: &Path,
    messenger: &dyn Messenger,
    config: &HeartbeatConfig,
) {
    let actions = load_pending(workspace).await;

    for mut action in actions {
        if !action.is_timed_out() {
            continue;
        }

        if !action.escalated {
            // Try escalation to backup approver
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
                        // Reset timeout for the backup approver
                        action.created_at = chrono::Local::now().to_rfc3339();
                        if let Err(e) = save_pending(workspace, &action).await {
                            warn!(id = %action.id, "Failed to save escalated action: {e}");
                        }
                        info!(id = %action.id, backup = %backup_id, "Escalated approval to backup");
                    }
                    Err(e) => {
                        warn!(id = %action.id, "Failed to DM backup approver: {e}");
                        // Fall through to expire
                        expire_action(workspace, messenger, config, &mut action).await;
                    }
                }
            } else {
                // No backup configured → expire immediately
                expire_action(workspace, messenger, config, &mut action).await;
            }
        } else {
            // Already escalated and timed out again → expire
            expire_action(workspace, messenger, config, &mut action).await;
        }
    }
}

async fn expire_action(
    workspace: &Path,
    messenger: &dyn Messenger,
    config: &HeartbeatConfig,
    action: &mut PendingAction,
) {
    action.state = ApprovalState::Expired;
    action.resolved_at = Some(chrono::Local::now().to_rfc3339());
    action.resolution_note = Some("Expired — no response from approver(s)".to_string());
    if let Err(e) = save_pending(workspace, action).await {
        warn!(id = %action.id, "Failed to save expired action: {e}");
    }

    // Notify team channel if configured
    let notify_msg = format!(
        "Approval expired for `{}` (requested by <@{}>). No approver responded.",
        action.tool_name, action.requester,
    );

    if let Some(ref ch_name) = config.notification_channel {
        if let Some(ch_id) = messenger.resolve_channel_id(ch_name).await {
            let _ = messenger.post_message(&ch_id, &notify_msg, None).await;
        }
    }

    // Also notify the original thread
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
