use anyhow::Result;
use chrono::Local;
use tracing::warn;

use crate::db::Db;

/// Append an entry to today's activity log in the database.
pub async fn append_log(
    db: &Db,
    channel: &str,
    user: &str,
    content: &str,
) -> Result<()> {
    let now = Local::now();
    let date = now.date_naive();
    let time = now.format("%H:%M").to_string();
    db.append_log(date, &time, channel, user, content).await
}

/// Read today's and yesterday's logs from the database, formatted as markdown.
pub async fn read_recent_logs(db: &Db) -> String {
    let today = Local::now().date_naive();
    match db.read_recent_logs(today).await {
        Ok(content) => content,
        Err(e) => {
            warn!(error = %e, "Failed to read recent logs from DB");
            String::new()
        }
    }
}

/// Read log entries since a given row ID. Returns (formatted entries, last_id).
/// Used by the heartbeat loop to process new entries incrementally.
pub async fn read_log_since(db: &Db, after_id: i64) -> (String, i64) {
    let today = Local::now().date_naive();
    match db.read_logs_since(today, after_id).await {
        Ok((entries, last_id)) => (entries, last_id),
        Err(e) => {
            warn!(error = %e, "Failed to read log entries from DB");
            (String::new(), after_id)
        }
    }
}
