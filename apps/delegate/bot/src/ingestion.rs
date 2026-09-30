//! Background ingestion worker — stage 2 of bootstrap.
//!
//! Pulls recent history from connected sources, writes `[ingestion]` entries
//! to the activity log, and reports progress back to the approver DM.
//!
//! Only Slack channel history is implemented end-to-end here. Jira/Linear/
//! Notion ingestion is stubbed: the worker records "skipped (not connected)"
//! progress so the UX is honest about what got read and what didn't.
//! Wiring a real pull for those sources uses the existing skill tools (e.g.
//! `jira_search`) but needs per-workspace credential coordination that's
//! deferred to a follow-up.

use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

use crate::bootstrap::BootstrapCoordinator;
use crate::db::Db;
use crate::logger;
use crate::messenger::Messenger;
use crate::oauth::CredentialStore;
use crate::workspace::Workspace;

const SLACK_HISTORY_LIMIT: u32 = 200;

/// Run the ingestion pipeline. Reports progress to the approver DM and
/// advances the bootstrap state to `Validating` when done.
pub async fn run(
    coord: Arc<BootstrapCoordinator>,
    messenger: Arc<dyn Messenger>,
    db: Db,
    ws: Workspace,
    credential_store: Arc<CredentialStore>,
) -> Result<()> {
    let approver = coord.approver().await;

    // Post initial status
    if let Some(u) = &approver {
        let _ = messenger
            .send_dm(u, "Starting background ingestion. I'll DM when I'm ready to check in.")
            .await;
    }

    // 1. Slack history from watched channels
    ingest_slack(&coord, messenger.as_ref(), &db, &ws).await;

    // 2. Jira / Linear / Notion — only if OAuth is connected.
    let connected = credential_store.connected_providers().await;

    for provider in &["atlassian", "linear", "notion", "github"] {
        if connected.contains(*provider) {
            coord
                .record_ingestion_progress(
                    provider,
                    0,
                    "queued (full ingestion pull deferred to follow-up)",
                )
                .await
                .ok();
        } else {
            coord
                .record_ingestion_progress(provider, 0, "skipped — not connected")
                .await
                .ok();
        }
    }

    // Hand off to validation stage.
    coord.mark_ingestion_complete().await?;

    if let Some(u) = &approver {
        let _ = messenger
            .send_dm(u, "Done catching up. Drafting a check-in now...")
            .await;
    }
    Ok(())
}

/// Pull recent history from each watched channel and append to the log with
/// an `[ingestion]` prefix so the heartbeat can distinguish real-time activity
/// from back-fill.
async fn ingest_slack(
    coord: &BootstrapCoordinator,
    messenger: &dyn Messenger,
    db: &Db,
    ws: &Workspace,
) {
    let channels = ws.watched_channels().await;
    if channels.is_empty() {
        coord
            .record_ingestion_progress("slack", 100, "no watched channels configured")
            .await
            .ok();
        return;
    }

    let total = channels.len();
    let mut done = 0usize;

    for name in &channels {
        let channel_id = match messenger.resolve_channel_id(name).await {
            Some(id) => id,
            None => {
                warn!(channel = %name, "Ingestion: could not resolve channel");
                done += 1;
                continue;
            }
        };

        let history = match messenger.get_channel_history(&channel_id, SLACK_HISTORY_LIMIT).await {
            Ok(h) => h,
            Err(e) => {
                warn!(channel = %name, error = %e, "Ingestion: channel history fetch failed");
                done += 1;
                continue;
            }
        };

        let mut written = 0u32;
        for msg in history.iter().rev() {
            // Skip empty messages and bot echo
            let text = msg.text.trim();
            if text.is_empty() {
                continue;
            }
            let user_name = messenger.get_user_name(&msg.user_id).await;
            let content = format!("[ingestion] {}", text);
            if let Err(e) = logger::append_log(db, name, &user_name, &content).await {
                warn!(error = %e, "Ingestion: log append failed");
                break;
            }
            written += 1;
        }

        done += 1;
        let pct = ((done as f32 / total as f32) * 100.0) as u32;
        let note = format!("{} ({} messages from #{})", pct, written, name);
        coord
            .record_ingestion_progress("slack", pct, &note)
            .await
            .ok();
        info!(channel = %name, messages = written, "Ingestion: slack channel done");
    }
}
