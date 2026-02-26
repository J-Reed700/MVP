use anyhow::Result;
use chrono::Local;
use std::path::Path;
use tokio::fs;

/// Append an entry to today's daily log file.
/// Format: `HH:MM | #channel | @user | content`
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

    // Create file with header if it doesn't exist
    if !log_file.exists() {
        let header = format!("# Daily Log — {today}\n\n");
        fs::write(&log_file, header).await?;
    }

    // Append the entry
    let mut existing = fs::read_to_string(&log_file).await.unwrap_or_default();
    existing.push_str(&entry);
    fs::write(&log_file, existing).await?;

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
