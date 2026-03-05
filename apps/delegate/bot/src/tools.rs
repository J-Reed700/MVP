use chrono::Duration;
use serde_json::Value;
use tracing::{info, warn};

use crate::event::DelegateEvent;
use crate::logger;
use crate::messenger::Messenger;
use crate::models::ToolCall;
use crate::text;
use crate::workspace::Workspace;

// ── Shared context passed to every tool handler ────────────────────────

/// Everything a tool handler needs to do its job.
pub struct ToolContext<'a> {
    pub messenger: &'a dyn Messenger,
    pub ws: &'a Workspace,
    pub event: &'a DelegateEvent,
    pub thread_ts: &'a str,
}

// ── Dispatch ───────────────────────────────────────────────────────────

/// Execute a single tool call and return a result string.
#[tracing::instrument(skip(ctx), fields(tool = %call.name))]
pub async fn execute_tool(call: &ToolCall, ctx: &ToolContext<'_>) -> String {
    let args = &call.arguments;
    match call.name.as_str() {
        "react" => handle_react(args, ctx).await,
        "reply" => handle_reply(args, ctx).await,
        "post" => handle_post(args, ctx).await,
        "no_action" => handle_no_action(args),
        "create_skill" => handle_create_skill(args, ctx).await,
        "load_skill" => handle_load_skill(args, ctx).await,
        "http_request" => handle_http_request(args).await,
        "run_script" => handle_run_script(args, ctx).await,
        "read_file" => handle_read_file(args, ctx).await,
        "write_file" => handle_write_file(args, ctx).await,
        "dm_user" => handle_dm_user(args, ctx).await,
        "channel_history" => handle_channel_history(args, ctx).await,
        "lookup_user" => handle_lookup_user(args, ctx).await,
        "save_memory" => handle_save_memory(args, ctx).await,
        "recall_memory" => handle_recall_memory(args, ctx).await,
        "log_decision" => handle_log_decision(args, ctx).await,
        "update_intents" => handle_update_intents(args, ctx).await,
        "set_reminder" => handle_set_reminder(args, ctx).await,
        "create_channel" => handle_create_channel(args, ctx).await,
        "invite_to_channel" => handle_invite_to_channel(args, ctx).await,
        "group_dm" => handle_group_dm(args, ctx).await,
        other => {
            warn!(tool = %other, "Unknown tool call");
            format!("Unknown tool: {other}")
        }
    }
}

/// Human-readable summary of a tool call for the daily log.
pub fn summarize_action(call: &ToolCall, result: &str) -> String {
    match call.name.as_str() {
        "react" => {
            let emoji = call.arguments["emoji"].as_str().unwrap_or("?");
            format!("reacted :{emoji}:")
        }
        "reply" => {
            let text = call.arguments["text"].as_str().unwrap_or("");
            format!("replied \"{}\"", truncate_str(text, 80))
        }
        "post" => {
            let ch = call.arguments["channel"].as_str().unwrap_or("?");
            format!("posted to {ch}")
        }
        "dm_user" => {
            let user = call.arguments["user"].as_str().unwrap_or("?");
            let text = call.arguments["text"].as_str().unwrap_or("");
            if result.starts_with("Failed") {
                format!("dm to {user} FAILED: {result}")
            } else {
                format!("dm'd {user}: \"{}\"", truncate_str(text, 80))
            }
        }
        "lookup_user" => {
            let name = call.arguments["name"].as_str().unwrap_or("?");
            format!("looked up user \"{name}\" → {}", truncate_str(result, 80))
        }
        "save_memory" => {
            let topic = call.arguments["topic"].as_str().unwrap_or("?");
            format!("saved memory: {topic}")
        }
        "set_reminder" => {
            let delay = call.arguments["delay_minutes"].as_u64().unwrap_or(0);
            let msg = call.arguments["message"].as_str().unwrap_or("?");
            format!("set reminder ({delay}min): \"{}\"", truncate_str(msg, 60))
        }
        "no_action" => {
            let reason = call.arguments["reason"].as_str().unwrap_or("");
            format!("no_action: {}", truncate_str(reason, 60))
        }
        "create_channel" => {
            let name = call.arguments["name"].as_str().unwrap_or("?");
            format!("created channel #{name}")
        }
        "invite_to_channel" => {
            let ch = call.arguments["channel"].as_str().unwrap_or("?");
            format!("invited users to {ch}")
        }
        "group_dm" => {
            let text = call.arguments["text"].as_str().unwrap_or("");
            format!("group dm: \"{}\"", truncate_str(text, 80))
        }
        other => other.to_string(),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Truncate a string at a char boundary, never panicking on multi-byte UTF-8.
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Check if a workspace-relative path is safe (no traversal, no absolute paths).
fn is_safe_path(path: &str) -> Result<(), &'static str> {
    if path.contains("..") {
        return Err("Path blocked: directory traversal (..) not allowed");
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err("Path blocked: must be a relative path within the workspace");
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err("Path blocked: absolute Windows paths not allowed");
    }
    Ok(())
}

/// Extract a required string arg, returning an error message if missing/empty.
fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    match args[key].as_str() {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Err(format!("Missing required argument: {key}")),
    }
}

// ── Individual tool handlers ───────────────────────────────────────────

async fn handle_react(args: &Value, ctx: &ToolContext<'_>) -> String {
    let emoji = args["emoji"].as_str().unwrap_or("eyes");
    match ctx
        .messenger
        .add_reaction(ctx.event.channel.as_str(), ctx.event.timestamp.as_str(), emoji)
        .await
    {
        Ok(_) => format!("Reacted with :{emoji}:"),
        Err(e) => format!("Failed to react: {e}"),
    }
}

async fn handle_reply(args: &Value, ctx: &ToolContext<'_>) -> String {
    let text = match require_str(args, "text") {
        Ok(t) => t,
        Err(e) => return e,
    };
    match ctx
        .messenger
        .post_message(ctx.event.channel.as_str(), text, Some(ctx.thread_ts))
        .await
    {
        Ok(_) => "Reply posted".to_string(),
        Err(e) => format!("Failed to reply: {e}"),
    }
}

async fn handle_post(args: &Value, ctx: &ToolContext<'_>) -> String {
    let channel_input = match require_str(args, "channel") {
        Ok(c) => c,
        Err(e) => return e,
    };
    let text = match require_str(args, "text") {
        Ok(t) => t,
        Err(e) => return e,
    };
    // Accept either a channel ID (C012345) or a channel name (general).
    // If it doesn't look like an ID, try to resolve the name.
    let channel_id = if channel_input.starts_with('C') || channel_input.starts_with('G') {
        channel_input.to_string()
    } else {
        match ctx.messenger.resolve_channel_id(channel_input).await {
            Some(id) => id,
            None => return format!("Could not resolve channel '{channel_input}'. Use a channel ID (e.g. C012345) or an exact channel name."),
        }
    };
    match ctx.messenger.post_message(&channel_id, text, None).await {
        Ok(_) => format!("Posted to {channel_input}"),
        Err(e) => format!("Failed to post: {e}"),
    }
}

fn handle_no_action(args: &Value) -> String {
    let reason = args["reason"].as_str().unwrap_or("no reason given");
    info!(reason = %reason, "Model chose no_action");
    format!("No action taken: {reason}")
}

async fn handle_create_skill(args: &Value, ctx: &ToolContext<'_>) -> String {
    let name = match require_str(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };
    let description = args["description"].as_str().unwrap_or("");
    let content = match require_str(args, "content") {
        Ok(c) => c,
        Err(e) => return e,
    };

    let skill_dir = ctx.ws.path().join("skills").join(name);
    if let Err(e) = tokio::fs::create_dir_all(&skill_dir).await {
        return format!("Failed to create skill directory: {e}");
    }
    let skill_md = format!("---\nname: {name}\ndescription: {description}\n---\n\n{content}\n");
    match tokio::fs::write(skill_dir.join("SKILL.md"), &skill_md).await {
        Ok(_) => {
            info!(skill = %name, "Created skill");
            format!("Skill '{name}' created")
        }
        Err(e) => format!("Failed to write skill: {e}"),
    }
}

async fn handle_run_script(args: &Value, ctx: &ToolContext<'_>) -> String {
    let language = match require_str(args, "language") {
        Ok(l) => l,
        Err(e) => return e,
    };
    let code = match require_str(args, "code") {
        Ok(c) => c,
        Err(e) => return e,
    };

    let scripts_dir = ctx.ws.path().join(".scripts");
    if let Err(e) = tokio::fs::create_dir_all(&scripts_dir).await {
        return format!("Failed to create scripts directory: {e}");
    }

    let (ext, program, shell_flag) = match language {
        "python" => ("py", "python3", None),
        "shell" => {
            if cfg!(windows) {
                ("bat", "cmd", Some("/C"))
            } else {
                ("sh", "sh", None)
            }
        }
        other => return format!("Unsupported language: {other}. Use 'python' or 'shell'."),
    };

    let script_path = scripts_dir.join(format!("_run.{ext}"));
    if let Err(e) = tokio::fs::write(&script_path, code).await {
        return format!("Failed to write script: {e}");
    }

    let mut cmd = tokio::process::Command::new(program);
    if let Some(flag) = shell_flag {
        cmd.arg(flag);
    }
    cmd.arg(&script_path);

    // Add user-provided args
    if let Some(script_args) = args["args"].as_array() {
        for arg in script_args {
            if let Some(a) = arg.as_str() {
                cmd.arg(a);
            }
        }
    }

    cmd.current_dir(ctx.ws.path());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        cmd.output(),
    )
    .await;

    // Clean up script file
    let _ = tokio::fs::remove_file(&script_path).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let status = output.status;
            info!(language = %language, status = %status, "Script executed");

            let mut result = format!("Exit code: {status}\n");
            if !stdout.is_empty() {
                result.push_str(&format!("\nstdout:\n{}", truncate_str(&stdout, 8192)));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("\nstderr:\n{}", truncate_str(&stderr, 2048)));
            }
            result
        }
        Ok(Err(e)) => format!("Failed to execute script: {e}"),
        Err(_) => "Script timed out after 30 seconds".to_string(),
    }
}

async fn handle_http_request(args: &Value) -> String {
    let method = match require_str(args, "method") {
        Ok(m) => m.to_uppercase(),
        Err(e) => return e,
    };
    let url = match require_str(args, "url") {
        Ok(u) => u,
        Err(e) => return e,
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to create HTTP client: {e}"),
    };

    let mut builder = match method.as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        other => return format!("Unsupported HTTP method: {other}"),
    };

    if let Some(headers) = args["headers"].as_object() {
        for (key, val) in headers {
            if let Some(v) = val.as_str() {
                builder = builder.header(key.as_str(), v);
            }
        }
    }

    if let Some(body) = args["body"].as_str() {
        builder = builder.body(body.to_string());
    }

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            info!(method = %method, url = %url, status = %status, "HTTP request");
            let truncated = truncate_str(&body, 8192);
            if truncated.len() < body.len() {
                format!("HTTP {status}\n\n{truncated}\n\n[truncated, {} bytes total]", body.len())
            } else {
                format!("HTTP {status}\n\n{body}")
            }
        }
        Err(e) => format!("HTTP request failed: {e}"),
    }
}

async fn handle_load_skill(args: &Value, ctx: &ToolContext<'_>) -> String {
    let skill_name = match require_str(args, "skill_name") {
        Ok(n) => n,
        Err(e) => return e,
    };
    if skill_name.contains("..") || skill_name.contains('/') || skill_name.contains('\\') {
        return "Invalid skill name".to_string();
    }
    let skill_path = ctx.ws.path().join("skills").join(skill_name).join("SKILL.md");
    match tokio::fs::read_to_string(&skill_path).await {
        Ok(content) => {
            info!(skill = %skill_name, "Loaded skill");
            content
        }
        Err(_) => format!("Skill not found: {skill_name}"),
    }
}

async fn handle_read_file(args: &Value, ctx: &ToolContext<'_>) -> String {
    let path = match require_str(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };
    if let Err(msg) = is_safe_path(path) {
        return msg.to_string();
    }
    match tokio::fs::read_to_string(ctx.ws.path().join(path)).await {
        Ok(contents) => {
            info!(path = %path, len = contents.len(), "Read file");
            if contents.len() > 10000 {
                format!(
                    "{}\n\n[truncated, {} bytes total]",
                    truncate_str(&contents, 10000),
                    contents.len()
                )
            } else {
                contents
            }
        }
        Err(_) => format!("File not found: {path}"),
    }
}

async fn handle_write_file(args: &Value, ctx: &ToolContext<'_>) -> String {
    let path = match require_str(args, "path") {
        Ok(p) => p,
        Err(e) => return e,
    };
    if let Err(msg) = is_safe_path(path) {
        return msg.to_string();
    }
    let content = args["content"].as_str().unwrap_or("");
    let full = ctx.ws.path().join(path);
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

async fn handle_dm_user(args: &Value, ctx: &ToolContext<'_>) -> String {
    let user = match require_str(args, "user") {
        Ok(u) => u,
        Err(e) => return e,
    };
    let text = match require_str(args, "text") {
        Ok(t) => t,
        Err(e) => return e,
    };
    match ctx.messenger.send_dm(user, text).await {
        Ok(_) => {
            info!(user = %user, "Sent DM");
            format!("DM sent to {user}")
        }
        Err(e) => format!("Failed to send DM: {e}"),
    }
}

async fn handle_channel_history(args: &Value, ctx: &ToolContext<'_>) -> String {
    let channel_input = match require_str(args, "channel") {
        Ok(c) => c,
        Err(e) => return e,
    };
    let count = args["count"].as_u64().unwrap_or(20).min(50) as u32;

    let channel = if channel_input.starts_with('C') || channel_input.starts_with('G') {
        channel_input.to_string()
    } else {
        match ctx.messenger.resolve_channel_id(channel_input).await {
            Some(id) => id,
            None => return format!("Could not resolve channel '{channel_input}'. Use a channel ID or exact name."),
        }
    };

    match ctx.messenger.get_channel_history(&channel, count).await {
        Ok(messages) if messages.is_empty() => {
            format!("No messages found in channel {channel}")
        }
        Ok(messages) => {
            let mut lines = Vec::with_capacity(messages.len());
            for msg in &messages {
                let name = ctx.messenger.get_user_name(&msg.user_id).await;
                lines.push(format!("[{}] <{name}> {}", msg.timestamp, msg.text));
            }
            lines.reverse(); // oldest first
            info!(channel = %channel, count = lines.len(), "Read channel history");
            let result = lines.join("\n");
            if result.len() > 8000 {
                format!(
                    "{}\n\n[truncated, {} messages total]",
                    truncate_str(&result, 8000),
                    messages.len()
                )
            } else {
                result
            }
        }
        Err(e) => format!("Failed to read channel history: {e}"),
    }
}

async fn handle_lookup_user(args: &Value, ctx: &ToolContext<'_>) -> String {
    let name = match require_str(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };
    match ctx.messenger.find_user_by_name(name).await {
        Ok(matches) if matches.is_empty() => format!("No users found matching '{name}'"),
        Ok(matches) => {
            let lines: Vec<String> = matches
                .iter()
                .map(|(id, display)| format!("- {display} (ID: {id})"))
                .collect();
            info!(query = %name, count = matches.len(), "User lookup");
            lines.join("\n")
        }
        Err(e) => format!("User lookup failed: {e}"),
    }
}

async fn handle_save_memory(args: &Value, ctx: &ToolContext<'_>) -> String {
    let topic = match require_str(args, "topic") {
        Ok(t) => t,
        Err(e) => return e,
    };
    let content = match require_str(args, "content") {
        Ok(c) => c,
        Err(e) => return e,
    };
    let summary = args["summary"].as_str().unwrap_or("");

    if topic.contains("..") || topic.contains('/') || topic.contains('\\') {
        return "Invalid topic slug".to_string();
    }

    let memory_dir = ctx.ws.path().join("memory");
    if let Err(e) = tokio::fs::create_dir_all(&memory_dir).await {
        return format!("Failed to create memory directory: {e}");
    }
    if let Err(e) = tokio::fs::write(memory_dir.join(format!("{topic}.md")), content).await {
        return format!("Failed to write memory/{topic}.md: {e}");
    }

    // Update MEMORY.md index
    let index_path = ctx.ws.path().join("MEMORY.md");
    let existing = tokio::fs::read_to_string(&index_path)
        .await
        .unwrap_or_else(|_| "# Memory Index\n\nTopics stored in `memory/`.\n".to_string());

    let entry_marker = format!("memory/{topic}.md");
    let new_entry = format!("- [{topic}]({entry_marker}) — {summary}");

    let updated = if existing.contains(&entry_marker) {
        existing
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
        format!("{}\n{}", existing.trim_end(), new_entry)
    };

    if let Err(e) = tokio::fs::write(&index_path, &updated).await {
        return format!("Wrote memory/{topic}.md but failed to update MEMORY.md: {e}");
    }

    info!(topic = %topic, "Saved memory");
    format!("Saved memory/{topic}.md and updated MEMORY.md index")
}

async fn handle_recall_memory(args: &Value, ctx: &ToolContext<'_>) -> String {
    let query = match require_str(args, "query") {
        Ok(q) => q,
        Err(e) => return e,
    };

    let memory_dir = ctx.ws.path().join("memory");
    let mut results = Vec::new();

    // Always include the index
    let index = tokio::fs::read_to_string(ctx.ws.path().join("MEMORY.md"))
        .await
        .unwrap_or_default();
    if !index.is_empty() {
        results.push(format!("## MEMORY.md Index\n{index}"));
    }

    let mut entries = match tokio::fs::read_dir(&memory_dir).await {
        Ok(e) => e,
        Err(_) => {
            return if results.is_empty() {
                "No memory files found. Memory is empty.".to_string()
            } else {
                results.join("\n\n---\n\n")
            };
        }
    };

    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2 && !text::is_stop_word(w))
        .collect();

    if query_terms.is_empty() {
        return if results.is_empty() {
            format!("No memory entries found matching '{query}'")
        } else {
            results.join("\n\n---\n\n")
        };
    }

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let filename = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        let content_lower = content.to_lowercase();
        let filename_lower = filename.to_lowercase();

        let matches = query_terms
            .iter()
            .any(|term| filename_lower.contains(term) || content_lower.contains(term));

        if matches {
            let display = if content.len() > 3000 {
                format!(
                    "{}...\n[truncated, {} bytes total]",
                    truncate_str(&content, 3000),
                    content.len()
                )
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

async fn handle_log_decision(args: &Value, ctx: &ToolContext<'_>) -> String {
    let decision = match require_str(args, "decision") {
        Ok(d) => d,
        Err(e) => return e,
    };
    let reasoning = args["reasoning"].as_str().unwrap_or("");
    let participants = args["participants"].as_str().unwrap_or("");
    let context = args["context"].as_str().unwrap_or("unknown");

    let decisions_path = ctx.ws.path().join("memory").join("decisions.md");
    let _ = tokio::fs::create_dir_all(ctx.ws.path().join("memory")).await;

    let existing = tokio::fs::read_to_string(&decisions_path)
        .await
        .unwrap_or_else(|_| {
            "# Decision Log\n\nDecisions captured from team conversations.\n".to_string()
        });

    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let heading: String = decision.chars().take(60).collect();
    let new_entry = format!(
        "\n---\n\n### {heading} ({date})\n\n\
         **Decision:** {decision}\n\
         **Reasoning:** {reasoning}\n\
         **Participants:** {participants}\n\
         **Context:** {context}\n"
    );

    let updated = format!("{}{}", existing.trim_end(), new_entry);
    match tokio::fs::write(&decisions_path, &updated).await {
        Ok(_) => {
            info!(decision = %decision, "Logged decision");

            // Ensure decisions.md is in the MEMORY.md index
            let index_path = ctx.ws.path().join("MEMORY.md");
            let index = tokio::fs::read_to_string(&index_path)
                .await
                .unwrap_or_default();
            if !index.contains("memory/decisions.md") {
                let updated_index = format!(
                    "{}\n- [decisions](memory/decisions.md) — Team decisions captured from conversations",
                    index.trim_end()
                );
                let _ = tokio::fs::write(&index_path, &updated_index).await;
            }

            format!("Decision logged: {}", truncate_str(decision, 80))
        }
        Err(e) => format!("Failed to log decision: {e}"),
    }
}

async fn handle_set_reminder(args: &Value, ctx: &ToolContext<'_>) -> String {
    let message = match require_str(args, "message") {
        Ok(m) => m,
        Err(e) => return e,
    };
    let delay_minutes = match args["delay_minutes"].as_i64() {
        Some(d) if (1..=1440).contains(&d) => d,
        Some(_) => return "delay_minutes must be between 1 and 1440 (24 hours)".to_string(),
        None => return "Missing required argument: delay_minutes".to_string(),
    };

    let target = args["target"].as_str().unwrap_or("");
    let channel = if target.is_empty() {
        ctx.event.channel.to_string()
    } else if target.starts_with('C') || target.starts_with('G') || target.starts_with('U') {
        target.to_string()
    } else {
        match ctx.messenger.resolve_channel_id(target).await {
            Some(id) => id,
            None => return format!("Could not resolve target '{target}'"),
        }
    };

    let now = chrono::Local::now();
    let fire_at = now + Duration::minutes(delay_minutes);

    let reminder = serde_json::json!({
        "id": format!("r-{}", now.timestamp()),
        "channel": channel,
        "user": ctx.event.user.as_str(),
        "message": message,
        "fire_at": fire_at.to_rfc3339(),
        "created_at": now.to_rfc3339(),
    });

    let reminders_path = ctx.ws.path().join("reminders.json");
    let mut reminders: Vec<Value> = match tokio::fs::read_to_string(&reminders_path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    reminders.push(reminder);

    match tokio::fs::write(&reminders_path, serde_json::to_string_pretty(&reminders).unwrap_or_default()).await {
        Ok(_) => {
            let time_str = fire_at.format("%H:%M").to_string();
            info!(message = %message, fire_at = %time_str, "Reminder set");
            format!("Reminder set for {time_str}: {message}")
        }
        Err(e) => format!("Failed to save reminder: {e}"),
    }
}

async fn handle_create_channel(args: &Value, ctx: &ToolContext<'_>) -> String {
    let name = match require_str(args, "name") {
        Ok(n) => n,
        Err(e) => return e,
    };
    let purpose = args["purpose"].as_str();
    match ctx.messenger.create_channel(name, purpose).await {
        Ok(sent) => {
            info!(channel = %name, id = %sent.channel, "Created channel");
            format!("Channel #{name} created (ID: {})", sent.channel)
        }
        Err(e) => format!("Failed to create channel: {e}"),
    }
}

async fn handle_invite_to_channel(args: &Value, ctx: &ToolContext<'_>) -> String {
    let channel_input = match require_str(args, "channel") {
        Ok(c) => c,
        Err(e) => return e,
    };
    let users = match args["users"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        None => match require_str(args, "users") {
            Ok(s) => s.split(',').map(|u| u.trim().to_string()).collect(),
            Err(e) => return e,
        },
    };
    if users.is_empty() {
        return "Missing required argument: users".to_string();
    }
    let channel_id = if channel_input.starts_with('C') || channel_input.starts_with('G') {
        channel_input.to_string()
    } else {
        match ctx.messenger.resolve_channel_id(channel_input).await {
            Some(id) => id,
            None => return format!("Could not resolve channel '{channel_input}'"),
        }
    };
    match ctx.messenger.invite_to_channel(&channel_id, &users).await {
        Ok(_) => {
            info!(channel = %channel_input, users = ?users, "Invited users to channel");
            format!("Invited {} user(s) to {channel_input}", users.len())
        }
        Err(e) => format!("Failed to invite users: {e}"),
    }
}

async fn handle_group_dm(args: &Value, ctx: &ToolContext<'_>) -> String {
    let users = match args["users"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        None => match require_str(args, "users") {
            Ok(s) => s.split(',').map(|u| u.trim().to_string()).collect(),
            Err(e) => return e,
        },
    };
    let text = match require_str(args, "text") {
        Ok(t) => t,
        Err(e) => return e,
    };
    if users.len() < 2 {
        return "Group DM requires at least 2 users".to_string();
    }
    match ctx.messenger.send_group_dm(&users, text).await {
        Ok(_) => {
            info!(users = ?users, "Sent group DM");
            format!("Group DM sent to {} users", users.len())
        }
        Err(e) => format!("Failed to send group DM: {e}"),
    }
}

async fn handle_update_intents(args: &Value, ctx: &ToolContext<'_>) -> String {
    let content = match require_str(args, "content") {
        Ok(c) => c,
        Err(e) => return e,
    };
    let reason = args["reason"].as_str().unwrap_or("no reason given");

    info!(reason = %reason, "Updating INTENTS.md");
    logger::append_log(
        ctx.ws.path(),
        "internal",
        "delegate-bot",
        &format!("[intents-update] {reason}"),
    )
    .await
    .ok();

    match tokio::fs::write(ctx.ws.path().join("INTENTS.md"), content).await {
        Ok(_) => format!("INTENTS.md updated. Reason: {reason}"),
        Err(e) => format!("Failed to update INTENTS.md: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── truncate_str ──

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_cuts() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn truncate_str_multibyte_safe() {
        // "café" is 5 bytes (é = 2 bytes). Cutting at 4 must not split the é.
        let s = "café";
        let result = truncate_str(s, 4);
        assert_eq!(result, "caf");
    }

    // ── is_safe_path ──

    #[test]
    fn safe_path_normal() {
        assert!(is_safe_path("memory/people.md").is_ok());
    }

    #[test]
    fn safe_path_blocks_traversal() {
        assert!(is_safe_path("../etc/passwd").is_err());
        assert!(is_safe_path("foo/../../bar").is_err());
    }

    #[test]
    fn safe_path_blocks_absolute_unix() {
        assert!(is_safe_path("/etc/passwd").is_err());
    }

    #[test]
    fn safe_path_blocks_absolute_windows() {
        assert!(is_safe_path("C:\\Windows\\System32").is_err());
    }

    #[test]
    fn safe_path_blocks_backslash_root() {
        assert!(is_safe_path("\\server\\share").is_err());
    }

    // ── require_str ──

    #[test]
    fn require_str_present() {
        let args = serde_json::json!({"text": "hello"});
        assert_eq!(require_str(&args, "text").unwrap(), "hello");
    }

    #[test]
    fn require_str_missing() {
        let args = serde_json::json!({});
        assert!(require_str(&args, "text").is_err());
    }

    #[test]
    fn require_str_empty() {
        let args = serde_json::json!({"text": ""});
        assert!(require_str(&args, "text").is_err());
    }

    // ── summarize_action ──

    #[test]
    fn summarize_react() {
        let call = ToolCall {
            id: "1".to_string(),
            name: "react".to_string(),
            arguments: serde_json::json!({"emoji": "thumbsup"}),
        };
        assert_eq!(summarize_action(&call, "ok"), "reacted :thumbsup:");
    }

    #[test]
    fn summarize_no_action() {
        let call = ToolCall {
            id: "2".to_string(),
            name: "no_action".to_string(),
            arguments: serde_json::json!({"reason": "just noise"}),
        };
        assert_eq!(summarize_action(&call, "ok"), "no_action: just noise");
    }

    #[test]
    fn summarize_dm_failure() {
        let call = ToolCall {
            id: "3".to_string(),
            name: "dm_user".to_string(),
            arguments: serde_json::json!({"user": "U123", "text": "hey"}),
        };
        let result = summarize_action(&call, "Failed to send DM: user not found");
        assert!(result.contains("FAILED"));
    }
}
