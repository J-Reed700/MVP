use chrono::{Datelike, Local, NaiveTime};
use std::path::Path;
use tokio::fs;

/// Parsed configuration from HEARTBEAT.md.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Heartbeat interval in seconds (default: 300 = 5 min)
    pub interval_secs: u64,
    /// Scheduled cron jobs parsed from the config
    pub cron_jobs: Vec<CronJob>,
    /// Token budget for question answering
    pub qa_token_budget: usize,
    /// Token budget for triage classification
    pub triage_token_budget: usize,
    /// Daily token budget across all LLM calls (default: 500_000)
    pub daily_token_budget: u64,
    /// Channel name for system notifications (budget exhaustion, errors, etc.)
    pub notification_channel: Option<String>,
    /// Default approver user ID or name for approval workflow
    pub default_approver: Option<String>,
    /// Backup approver if default doesn't respond
    pub backup_approver: Option<String>,
    /// Approval timeout in seconds (default: 14400 = 4 hours)
    pub approval_timeout_secs: u64,
}

/// A scheduled output job (standup, weekly summary, etc.)
#[derive(Debug, Clone)]
pub struct CronJob {
    /// Human-readable name
    pub name: String,
    /// Time of day to fire (e.g., 09:15)
    pub time: NaiveTime,
    /// Days of week (0=Mon..6=Sun). Empty = every day.
    pub days: Vec<u32>,
    /// Target channel to post to
    pub channel: String,
    /// Type of output: "digest" or "update"
    pub output_type: String,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            cron_jobs: Vec::new(),
            qa_token_budget: 8000,
            triage_token_budget: 500,
            daily_token_budget: 500_000,
            notification_channel: None,
            default_approver: None,
            backup_approver: None,
            approval_timeout_secs: 14400,
        }
    }
}

/// Parse HEARTBEAT.md into a structured config.
/// Supports sections: Schedule, Watched Channels, Triage Thresholds, Token Budgets.
/// Read and parse HEARTBEAT.md.
///
/// Split into async file read + sync parse so the validator closure doesn't
/// need to live across an await boundary.
pub async fn parse_config(workspace: &Path, validate_id: &(dyn Fn(&str) -> bool + Send + Sync)) -> HeartbeatConfig {
    let path = workspace.join("HEARTBEAT.md");
    let content = fs::read_to_string(&path).await.unwrap_or_default();
    parse_heartbeat_content(&content, validate_id)
}

fn parse_heartbeat_content(content: &str, validate_id: &dyn Fn(&str) -> bool) -> HeartbeatConfig {
    let mut config = HeartbeatConfig::default();
    let mut current_section = "";

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect sections
        if trimmed.starts_with("## ") {
            let section = trimmed[3..].trim().to_lowercase();
            current_section = match section.as_str() {
                s if s.contains("schedule") => "schedule",
                s if s.contains("interval") => "interval",
                s if s.contains("token") => "tokens",
                s if s.contains("notification") => "notifications",
                s if s.contains("approval") => "approvals",
                _ => "",
            };
            continue;
        }

        if !trimmed.starts_with('-') && !trimmed.starts_with('*') {
            continue;
        }

        let item = trimmed.trim_start_matches('-').trim_start_matches('*').trim();

        match current_section {
            "interval" => {
                if let Some(secs) = parse_interval(item) {
                    config.interval_secs = secs;
                }
            }
            "schedule" => {
                if let Some(job) = parse_cron_entry(item) {
                    config.cron_jobs.push(job);
                }
            }
            "tokens" => {
                let lower = item.to_lowercase();
                if lower.contains("daily") || lower.contains("budget") || lower.contains("total") {
                    if let Some(budget) = extract_number(item) {
                        config.daily_token_budget = budget as u64;
                    }
                } else if lower.contains("question") || lower.contains("answering") || lower.contains("qa") {
                    if let Some(budget) = extract_number(item) {
                        config.qa_token_budget = budget;
                    }
                } else if lower.contains("triage") || lower.contains("classification") {
                    if let Some(budget) = extract_number(item) {
                        config.triage_token_budget = budget;
                    }
                }
            }
            "notifications" => {
                let lower = item.to_lowercase();
                if lower.contains("channel") {
                    if let Some(ch) = extract_channel_name(item) {
                        config.notification_channel = Some(ch);
                    }
                }
            }
            "approvals" => {
                let lower = item.to_lowercase();
                if lower.contains("default") && lower.contains("approver") {
                    if let Some(val) = extract_value_after_colon(item) {
                        config.default_approver = validate_approver_id(val, "default approver", validate_id);
                    }
                } else if lower.contains("backup") && lower.contains("approver") {
                    if let Some(val) = extract_value_after_colon(item) {
                        config.backup_approver = validate_approver_id(val, "backup approver", validate_id);
                    }
                } else if lower.contains("timeout") {
                    if let Some(secs) = parse_interval(item) {
                        config.approval_timeout_secs = secs;
                    }
                }
            }
            _ => {}
        }
    }

    config
}

/// Validate that a parsed value looks like a valid user ID using the
/// transport-provided validator. If invalid, logs a warning and returns None.
fn validate_approver_id(value: String, label: &str, validate_id: &dyn Fn(&str) -> bool) -> Option<String> {
    let trimmed = value.trim();
    if validate_id(trimmed) {
        Some(trimmed.to_string())
    } else {
        tracing::warn!(
            value = %trimmed,
            label = %label,
            "HEARTBEAT.md: expected a valid user ID for {}, got '{}' — ignoring",
            label,
            trimmed,
        );
        None
    }
}

/// Parse a cron entry line.
/// Expected formats:
///   "Standup: 9:15am daily → #general (digest)"
///   "Weekly summary: Friday 4pm → #general (update)"
///   "Standup: 09:15 daily #general digest"
fn parse_cron_entry(line: &str) -> Option<CronJob> {
    // Skip "no scheduled" lines
    let lower = line.to_lowercase();
    if lower.contains("no scheduled") || lower.contains("reactive only") {
        return None;
    }

    // Extract name (before colon)
    let (name, rest) = if let Some(i) = line.find(':') {
        (line[..i].trim().to_string(), &line[i + 1..])
    } else {
        return None;
    };

    let rest = rest.trim();

    // Extract time
    let time = parse_time_from_text(rest)?;

    // Extract days
    let days = parse_days_from_text(rest);

    // Extract channel (look for #channel or → #channel)
    let channel = regex::Regex::new(r"#(\S+)")
        .ok()?
        .captures(rest)?
        .get(1)?
        .as_str()
        .to_string();

    // Extract output type — check for explicit (type) marker first, then infer from name
    let output_type = if let Some(caps) = regex::Regex::new(r"\((\w+)\)").ok().and_then(|re| re.captures(rest)) {
        caps[1].to_lowercase()
    } else if lower.contains("update") || lower.contains("status") {
        "update".to_string()
    } else {
        "digest".to_string()
    };

    Some(CronJob {
        name,
        time,
        days,
        channel,
        output_type,
    })
}

/// Parse time from text like "9:15am", "16:00", "4pm".
fn parse_time_from_text(text: &str) -> Option<NaiveTime> {
    let re = regex::Regex::new(r"(\d{1,2}):(\d{2})\s*(am|pm)?").ok()?;
    if let Some(caps) = re.captures(&text.to_lowercase()) {
        let mut hour: u32 = caps[1].parse().ok()?;
        let min: u32 = caps[2].parse().ok()?;
        if let Some(ampm) = caps.get(3) {
            match ampm.as_str() {
                "pm" if hour < 12 => hour += 12,
                "am" if hour == 12 => hour = 0,
                _ => {}
            }
        }
        return NaiveTime::from_hms_opt(hour, min, 0);
    }

    // Try just "4pm" pattern
    let re_simple = regex::Regex::new(r"(\d{1,2})\s*(am|pm)").ok()?;
    if let Some(caps) = re_simple.captures(&text.to_lowercase()) {
        let mut hour: u32 = caps[1].parse().ok()?;
        match &caps[2] {
            "pm" if hour < 12 => hour += 12,
            "am" if hour == 12 => hour = 0,
            _ => {}
        }
        return NaiveTime::from_hms_opt(hour, 0, 0);
    }

    None
}

/// Parse days from text. Returns empty vec for "daily".
fn parse_days_from_text(text: &str) -> Vec<u32> {
    let lower = text.to_lowercase();
    if lower.contains("daily") || lower.contains("every day") {
        return Vec::new(); // empty = every day
    }

    let mut days = Vec::new();
    let day_map = [
        ("monday", 0), ("mon", 0),
        ("tuesday", 1), ("tue", 1),
        ("wednesday", 2), ("wed", 2),
        ("thursday", 3), ("thu", 3),
        ("friday", 4), ("fri", 4),
        ("saturday", 5), ("sat", 5),
        ("sunday", 6), ("sun", 6),
    ];

    for (name, num) in &day_map {
        if lower.contains(name) && !days.contains(num) {
            days.push(*num);
        }
    }

    days
}

fn parse_interval(text: &str) -> Option<u64> {
    let lower = text.to_lowercase();
    let num = extract_number(text)? as u64;

    if lower.contains("min") {
        Some(num * 60)
    } else if lower.contains("sec") {
        Some(num)
    } else if lower.contains("hour") {
        Some(num * 3600)
    } else {
        // Assume minutes if no unit
        Some(num * 60)
    }
}

fn extract_number(text: &str) -> Option<usize> {
    regex::Regex::new(r"(\d+)")
        .ok()?
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

/// Extract a channel name from text like "Channel: #ops-notifications" or "Channel: ops-notifications".
fn extract_channel_name(text: &str) -> Option<String> {
    // Try #channel pattern first
    if let Some(caps) = regex::Regex::new(r"#(\S+)").ok()?.captures(text) {
        return Some(caps[1].to_string());
    }
    // Fall back to value after colon
    extract_value_after_colon(text)
}

/// Extract the value after the last colon in "Key: value" format.
fn extract_value_after_colon(text: &str) -> Option<String> {
    let val = text.rsplit_once(':')?.1.trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

/// Read log entries since a given timestamp from today's log.
/// Returns the new entries as a string and the line count for tracking.
pub async fn read_log_since(workspace: &Path, since_line: usize) -> (String, usize) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let log_file = workspace.join("logs").join(format!("{today}.md"));

    let content = match fs::read_to_string(&log_file).await {
        Ok(c) => c,
        Err(_) => return (String::new(), 0),
    };

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if total <= since_line {
        return (String::new(), total);
    }

    let new_entries = lines[since_line..].join("\n");
    (new_entries, total)
}

/// Check if a cron job should fire at the current time.
/// Uses a forward-only window: fires if the scheduled time is within [now - interval/2, now].
/// This prevents double-firing when interval equals the window size.
pub fn should_fire(job: &CronJob, now: &chrono::DateTime<Local>, interval_secs: u64) -> bool {
    // Check day of week
    if !job.days.is_empty() {
        let weekday = now.weekday().num_days_from_monday();
        if !job.days.contains(&weekday) {
            return false;
        }
    }

    // Forward-only window: job fires if we're within half the interval *after* the scheduled time.
    // This avoids the symmetric window that causes double-firing.
    let current_time = now.time();
    let diff_secs = (current_time - job.time).num_seconds();
    // Only fire if we're 0..window_secs past the scheduled time (not before it)
    let window_secs = (interval_secs / 2).max(30) as i64;
    diff_secs >= 0 && diff_secs < window_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slack_validate_id(id: &str) -> bool {
        (id.starts_with('U') || id.starts_with('W'))
            && id.len() > 1
            && id.chars().skip(1).all(|c| c.is_alphanumeric())
    }

    #[test]
    fn test_parse_default() {
        let config = parse_heartbeat_content("", &slack_validate_id);
        assert_eq!(config.interval_secs, 300);
        assert!(config.cron_jobs.is_empty());
    }

    #[test]
    fn test_parse_token_budgets() {
        let content = "## Token Budgets\n- Daily budget: 750000 tokens\n- Question answering: 10000 tokens\n- Triage classification: 800 tokens\n";
        let config = parse_heartbeat_content(content, &slack_validate_id);
        assert_eq!(config.daily_token_budget, 750000);
        assert_eq!(config.qa_token_budget, 10000);
        assert_eq!(config.triage_token_budget, 800);
    }

    #[test]
    fn test_parse_cron_entry() {
        let job = parse_cron_entry("Standup: 9:15am daily → #general (digest)").unwrap();
        assert_eq!(job.name, "Standup");
        assert_eq!(job.time, NaiveTime::from_hms_opt(9, 15, 0).unwrap());
        assert!(job.days.is_empty()); // daily
        assert_eq!(job.channel, "general");
        assert_eq!(job.output_type, "digest");
    }

    #[test]
    fn test_parse_cron_friday() {
        let job = parse_cron_entry("Weekly summary: Friday 4pm → #general (update)").unwrap();
        assert_eq!(job.name, "Weekly summary");
        assert_eq!(job.time, NaiveTime::from_hms_opt(16, 0, 0).unwrap());
        assert_eq!(job.days, vec![4]); // Friday
        assert_eq!(job.output_type, "update");
    }

    #[test]
    fn test_no_scheduled() {
        assert!(parse_cron_entry("No scheduled posts yet — reactive only during dogfooding").is_none());
    }

    #[test]
    fn test_parse_time() {
        assert_eq!(parse_time_from_text("9:15am"), NaiveTime::from_hms_opt(9, 15, 0));
        assert_eq!(parse_time_from_text("4pm"), NaiveTime::from_hms_opt(16, 0, 0));
        assert_eq!(parse_time_from_text("16:00"), NaiveTime::from_hms_opt(16, 0, 0));
        assert_eq!(parse_time_from_text("12:30pm"), NaiveTime::from_hms_opt(12, 30, 0));
    }
}
