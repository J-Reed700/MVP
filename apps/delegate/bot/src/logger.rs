use anyhow::Result;
use chrono::Local;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Append an entry to today's daily log file.
/// Format: `HH:MM | #channel | @user | content`
///
/// Uses atomic file-append (O_APPEND) to avoid data loss under concurrent writes.
pub async fn append_log(
    workspace: &Path,
    channel: &str,
    user: &str,
    content: &str,
) -> Result<()> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let time = Local::now().format("%H:%M").to_string();
    let logs_dir = workspace.join("logs");
    fs::create_dir_all(&logs_dir).await?;

    let log_file = logs_dir.join(format!("{today}.md"));
    let entry = format!("- {time} | #{channel} | @{user} | {content}\n");

    // Use OpenOptions with create + append for atomic concurrent writes.
    // If the file is new, prepend the header.
    let is_new = !log_file.exists();

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .await?;

    if is_new {
        let header = format!("# Daily Log — {today}\n\n");
        file.write_all(header.as_bytes()).await?;
    }

    file.write_all(entry.as_bytes()).await?;

    Ok(())
}

/// Read today's log (and optionally yesterday's) for context.
pub async fn read_recent_logs(workspace: &Path) -> String {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    let logs_dir = workspace.join("logs");
    let mut content = String::new();

    for date in [&yesterday, &today] {
        let path = logs_dir.join(format!("{date}.md"));
        if let Ok(text) = fs::read_to_string(&path).await {
            content.push_str(&text);
            content.push('\n');
        }
    }

    content
}
