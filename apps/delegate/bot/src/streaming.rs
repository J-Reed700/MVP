use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

use crate::messenger::Messenger;
use crate::models::StreamEvent;

/// Minimum interval between Slack message updates to avoid rate limits.
const UPDATE_INTERVAL_MS: u64 = 1200;

/// After this many characters of accumulated text, force a Slack update
/// even if the interval hasn't elapsed (keeps the user engaged on long responses).
const FORCE_UPDATE_CHARS: usize = 300;

/// Consumes a stream of `StreamEvent`s and progressively updates a Slack message.
///
/// Posts an initial placeholder, then edits it as text deltas arrive.
/// Throttles updates to avoid Slack rate limits (~1 update per second).
/// Returns the fully accumulated text content.
pub async fn stream_to_slack(
    mut rx: mpsc::UnboundedReceiver<StreamEvent>,
    messenger: Arc<dyn Messenger>,
    channel: &str,
    thread_ts: Option<&str>,
) -> String {
    let mut accumulated = String::new();
    let mut last_update = std::time::Instant::now();
    let mut chars_since_update: usize = 0;
    let mut message_ts: Option<String> = None;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::TextDelta(delta) => {
                accumulated.push_str(&delta);
                chars_since_update += delta.len();

                let elapsed = last_update.elapsed().as_millis() as u64;
                let should_update = elapsed >= UPDATE_INTERVAL_MS
                    || chars_since_update >= FORCE_UPDATE_CHARS;

                if should_update && !accumulated.trim().is_empty() {
                    let display_text = format!("{}...", accumulated.trim());

                    match &message_ts {
                        None => {
                            match messenger.post_message(channel, &display_text, thread_ts).await {
                                Ok(sent) => {
                                    message_ts = Some(sent.timestamp);
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to post initial streaming message");
                                }
                            }
                        }
                        Some(ts) => {
                            if let Err(e) = messenger.update_message(channel, ts, &display_text).await {
                                warn!(error = %e, "Failed to update streaming message");
                            }
                        }
                    }
                    last_update = std::time::Instant::now();
                    chars_since_update = 0;
                }
            }
            StreamEvent::ToolCallComplete(_) => {
                // Tool calls don't produce visible text for streaming display
            }
        }
    }

    // Final update with complete text (no trailing "...")
    if !accumulated.trim().is_empty() {
        if let Some(ts) = &message_ts {
            if let Err(e) = messenger.update_message(channel, ts, accumulated.trim()).await {
                warn!(error = %e, "Failed to send final streaming update");
            }
        } else {
            // Stream finished before we ever posted — just post the full thing
            if let Err(e) = messenger.post_message(channel, accumulated.trim(), thread_ts).await {
                warn!(error = %e, "Failed to post streaming result");
            }
        }
    }

    accumulated
}
