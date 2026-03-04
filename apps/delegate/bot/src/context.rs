use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::event::DelegateEvent;
use crate::models::estimate_tokens;
use crate::retriever::{format_retrieved_content, retrieve};
use crate::text;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub full_content: String,
}

#[derive(Debug, Clone, Copy)]
pub enum TaskType {
    Respond,
    Digest,
    Update,
}

#[derive(Debug, Clone)]
pub struct CompiledContext {
    pub identity: String,
    pub intents: String,
    pub heartbeat: String,
    pub memory: String,
    pub skills: Vec<Skill>,
    pub retrieved: String,
    pub trigger: String,
    pub framing: String,
    pub recent_logs: String,
}

/// Compile full context for a model call in response to a live event.
///
/// Token budget priority ordering (per ARCHITECTURE.md):
///   1. IDENTITY.md — Never cut.
///   2. INTENTS.md — Never cut.
///   3. Task framing — Never cut.
///   4. Current trigger — Never cut.
///   5. Relevant history — Compressed/truncated when tight.
///   6. MEMORY.md — Truncated to top-level pointers if critically tight.
///
/// Retrieved context fills remaining budget after all priority tiers.
pub async fn compile(
    event: &DelegateEvent,
    workspace: &Path,
    task_type: TaskType,
    recent_logs: &str,
    token_budget: usize,
    channel_name: Option<&str>,
    is_dm: bool,
) -> Result<CompiledContext> {
    // 1. Load always-on files (tiers 1-2: never cut)
    let identity = load_file(&workspace.join("IDENTITY.md")).await;
    let intents = load_file(&workspace.join("INTENTS.md")).await;
    let heartbeat = load_file(&workspace.join("HEARTBEAT.md")).await;
    let skills = load_skills(&workspace.join("skills")).await;

    // 2. Build framing (tier 3: never cut)
    // Use resolved channel name for audience inference, fall back to channel ID
    let channel_for_framing = channel_name.unwrap_or(event.channel.as_str());
    let framing = build_framing(task_type, channel_for_framing, is_dm);

    // 3. Build trigger (tier 4: never cut)
    let trigger = format!(
        "Channel: {}\nFrom: {}\nTime: {}\n\n{}",
        event.channel, event.user, event.timestamp, event.content
    );

    // 4. Calculate protected (never-cut) token cost
    let skills_text: String = skills.iter().map(|s| s.full_content.as_str()).collect::<Vec<_>>().join("\n");
    let protected_tokens = estimate_tokens(&identity)
        + estimate_tokens(&intents)
        + estimate_tokens(&heartbeat)
        + estimate_tokens(&framing)
        + estimate_tokens(&trigger)
        + estimate_tokens(&skills_text)
        + 200; // structural overhead (headers, separators)

    let mut remaining = token_budget.saturating_sub(protected_tokens);

    // 5. Recent logs (tier 5: compressible)
    let logs_tokens = estimate_tokens(recent_logs);
    let recent_logs_trimmed = if logs_tokens <= remaining {
        remaining -= logs_tokens;
        recent_logs.to_string()
    } else if remaining > 200 {
        // Truncate to fit: keep the most recent lines
        let budget_chars = remaining * 4;
        let trimmed = truncate_keep_tail(recent_logs, budget_chars);
        remaining = 0;
        trimmed
    } else {
        String::new()
    };

    // 6. MEMORY.md (tier 6: truncatable to pointers)
    let memory_full = load_file(&workspace.join("MEMORY.md")).await;
    let memory_tokens = estimate_tokens(&memory_full);
    let memory = if memory_tokens <= remaining {
        remaining -= memory_tokens;
        memory_full
    } else if remaining > 100 {
        // Truncate to top-level pointers (first N lines)
        let budget_chars = remaining * 4;
        let truncated = truncate_keep_head(&memory_full, budget_chars);
        remaining = remaining.saturating_sub(estimate_tokens(&truncated));
        truncated
    } else {
        String::new()
    };

    // 7. Retrieved context fills whatever budget remains
    let terms = extract_terms(&event.content);
    let bias_terms = extract_terms(&intents);
    let retrieval_results = retrieve(workspace, &terms, &bias_terms, 15, 3).await?;
    let retrieved = format_retrieved_content(&retrieval_results, remaining);

    Ok(CompiledContext {
        identity,
        intents,
        heartbeat,
        memory,
        skills,
        retrieved,
        trigger,
        framing,
        recent_logs: recent_logs_trimmed,
    })
}

/// Assemble a CompiledContext into (system_prompt, user_prompt).
pub fn to_prompt(ctx: &CompiledContext) -> (String, String) {
    let mut system_parts = Vec::new();

    // Priority 1: Identity (never cut)
    system_parts.push(format!("# Team Briefing\n{}", ctx.identity));

    if !ctx.skills.is_empty() {
        let mut skills_section = String::from("\n# Skills\n\nThese are your capabilities. If something isn't listed here, you can't do it.\n");
        for skill in &ctx.skills {
            skills_section.push_str(&format!("\n---\n\n{}", skill.full_content));
        }
        skills_section.push_str("\n---\n\nWhen someone asks you to do something not listed above, say \"I can't do that yet\" and suggest what you *can* do instead.");
        system_parts.push(skills_section);
    }

    // Priority 2: Intents (never cut)
    if !ctx.intents.is_empty() {
        system_parts.push(format!("\n# Active Intents\n{}", ctx.intents));
    }

    // Operational config (never cut)
    if !ctx.heartbeat.is_empty() {
        system_parts.push(format!("\n# Operational Config\n{}", ctx.heartbeat));
    }

    // Priority 6: Memory (may be truncated)
    if !ctx.memory.is_empty() {
        system_parts.push(format!("\n# Knowledge Index\n{}", ctx.memory));
    }

    // Retrieved context (fills remaining budget)
    if !ctx.retrieved.is_empty() {
        system_parts.push(format!("\n# Retrieved Context\n{}", ctx.retrieved));
    }

    // Priority 5: Recent logs (may be truncated)
    if !ctx.recent_logs.is_empty() {
        system_parts.push(format!("\n# Recent Activity\n{}", ctx.recent_logs));
    }

    // Priority 3: Framing (never cut)
    system_parts.push(format!("\n# Your Role\n{}", ctx.framing));

    let system = system_parts.join("\n");
    let prompt = ctx.trigger.clone();

    (system, prompt)
}

async fn load_file(path: &Path) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

/// Load all skills from workspace/skills/*/SKILL.md
async fn load_skills(skills_dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut entries = match tokio::fs::read_dir(skills_dir).await {
        Ok(e) => e,
        Err(_) => return skills,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let skill_file = entry.path().join("SKILL.md");
        if let Ok(content) = tokio::fs::read_to_string(&skill_file).await {
            let (name, description, body) = parse_skill_frontmatter(&content);
            let name = name.unwrap_or_else(|| {
                entry.file_name().to_string_lossy().to_string()
            });
            skills.push(Skill {
                name,
                description: description.unwrap_or_default(),
                full_content: body,
            });
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Parse YAML frontmatter from a SKILL.md file.
/// Returns (name, description, body after frontmatter).
fn parse_skill_frontmatter(content: &str) -> (Option<String>, Option<String>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, None, content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let close = match after_first.find("---") {
        Some(i) => i,
        None => return (None, None, content.to_string()),
    };

    let frontmatter = &after_first[..close];
    let body = after_first[close + 3..].trim_start().to_string();

    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().to_string());
        }
    }

    (name, description, body)
}

/// Extract meaningful search terms using stop-word filtering.
/// Also extracts multi-word phrases from backtick-quoted text and capitalized sequences.
fn extract_terms(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    // Extract backtick-quoted phrases as compound terms (e.g. `billing migration`)
    for cap in regex::Regex::new(r"`([^`]+)`").unwrap().captures_iter(input) {
        let phrase = cap[1].trim().to_lowercase();
        if phrase.len() > 2 && seen.insert(phrase.clone()) {
            result.push(phrase);
        }
    }

    // Extract individual words
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();

    for word in cleaned.split_whitespace() {
        let lower = word.to_lowercase();
        if lower.len() > 2 && !text::is_stop_word(&lower) && seen.insert(lower.clone()) {
            result.push(lower);
        }
    }

    result
}

/// Truncate text keeping the tail (most recent content). Returns with a prefix marker.
fn truncate_keep_tail(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Find a valid char boundary near the start of the tail window
    let mut start = text.len().saturating_sub(max_chars);
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    // Find next newline to avoid splitting mid-line
    let start = text[start..].find('\n').map(|i| start + i + 1).unwrap_or(start);
    format!("[...earlier entries truncated]\n{}", &text[start..])
}

/// Truncate text keeping the head (top-level structure). Returns with a suffix marker.
fn truncate_keep_head(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Find a valid char boundary at or before max_chars
    let mut end = max_chars;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // Find last newline before limit to avoid splitting mid-line
    let end = text[..end].rfind('\n').unwrap_or(end);
    format!("{}\n[...truncated]", &text[..end])
}

/// Compress INTENTS.md to fit within a token budget for cheap triage models.
/// Keeps headings and bullet points, trims verbose prose paragraphs.
/// Target: ~500 tokens for Tier 1 triage.
pub fn compress_intents(intents: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens * 4;

    if intents.len() <= max_chars {
        return intents.to_string();
    }

    // Priority: keep headings (#) and bullet lines (- or *), trim paragraphs
    let mut structural = Vec::new();
    let mut prose = Vec::new();

    for line in intents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("- ") {
            structural.push(line);
        } else if !trimmed.is_empty() {
            prose.push(line);
        }
    }

    // First try: all structural lines
    let structural_text = structural.join("\n");
    if structural_text.len() <= max_chars {
        // Fill remaining budget with prose
        let remaining = max_chars - structural_text.len();
        let prose_text: String = prose.join("\n");
        if prose_text.len() <= remaining {
            return format!("{}\n{}", structural_text, prose_text);
        }
        let truncated_prose = truncate_keep_head(&prose_text, remaining);
        return format!("{}\n{}", structural_text, truncated_prose);
    }

    // Structural lines alone exceed budget — truncate them
    truncate_keep_head(&structural_text, max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_terms ──

    #[test]
    fn extract_terms_filters_stop_words() {
        let terms = extract_terms("the quick brown fox and the lazy dog");
        assert!(terms.contains(&"quick".to_string()));
        assert!(terms.contains(&"brown".to_string()));
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"and".to_string()));
    }

    #[test]
    fn extract_terms_backtick_phrases() {
        let terms = extract_terms("check `billing migration` and `api team`");
        assert!(terms.contains(&"billing migration".to_string()));
        assert!(terms.contains(&"api team".to_string()));
    }

    #[test]
    fn extract_terms_deduplicates() {
        let terms = extract_terms("deploy deploy deploy");
        let deploy_count = terms.iter().filter(|t| *t == "deploy").count();
        assert_eq!(deploy_count, 1);
    }

    #[test]
    fn extract_terms_strips_punctuation() {
        let terms = extract_terms("what's the status of PROJECT-123?");
        // Punctuation becomes spaces, so "what's" → "what s"
        assert!(!terms.iter().any(|t| t.contains('?')));
    }

    // ── truncate_keep_tail ──

    #[test]
    fn truncate_tail_short_text() {
        let text = "line 1\nline 2";
        assert_eq!(truncate_keep_tail(text, 100), text);
    }

    #[test]
    fn truncate_tail_keeps_recent() {
        let text = "old line 1\nold line 2\nnew line 3\nnew line 4";
        let result = truncate_keep_tail(text, 25);
        assert!(result.contains("new line"));
        assert!(result.starts_with("[...earlier entries truncated]"));
    }

    // ── truncate_keep_head ──

    #[test]
    fn truncate_head_short_text() {
        let text = "line 1\nline 2";
        assert_eq!(truncate_keep_head(text, 100), text);
    }

    #[test]
    fn truncate_head_keeps_beginning() {
        let text = "# Header\n- item 1\n- item 2\n\nLong paragraph that goes on and on and on";
        let result = truncate_keep_head(text, 30);
        assert!(result.contains("# Header"));
        assert!(result.ends_with("\n[...truncated]"));
    }

    // ── compress_intents ──

    #[test]
    fn compress_intents_short() {
        let intents = "# Priorities\n- Ship billing\n- Fix auth bug";
        assert_eq!(compress_intents(intents, 500), intents);
    }

    #[test]
    fn compress_intents_keeps_structure() {
        let mut intents = String::from("# Priorities\n- Ship billing\n- Fix auth bug\n");
        // Add enough prose to exceed the budget
        for i in 0..50 {
            intents.push_str(&format!("This is paragraph {} with lots of detail about nothing important at all.\n", i));
        }
        let result = compress_intents(&intents, 100); // ~100 tokens = ~400 chars
        assert!(result.contains("# Priorities"));
        assert!(result.contains("- Ship billing"));
    }

    // ── parse_skill_frontmatter ──

    #[test]
    fn parse_skill_with_frontmatter() {
        let content = "---\nname: summarize\ndescription: Summarize threads\n---\n\n# Summarize\n\nDo the thing.";
        let (name, desc, body) = parse_skill_frontmatter(content);
        assert_eq!(name.unwrap(), "summarize");
        assert_eq!(desc.unwrap(), "Summarize threads");
        assert!(body.contains("# Summarize"));
    }

    #[test]
    fn parse_skill_without_frontmatter() {
        let content = "# Just a skill\n\nNo frontmatter here.";
        let (name, desc, body) = parse_skill_frontmatter(content);
        assert!(name.is_none());
        assert!(desc.is_none());
        assert_eq!(body, content);
    }

    // ── infer_audience ──

    #[test]
    fn audience_engineering() {
        assert!(matches!(infer_audience("platform-eng", false), Audience::Engineering));
        assert!(matches!(infer_audience("backend-dev", false), Audience::Engineering));
        assert!(matches!(infer_audience("sre-oncall", false), Audience::Engineering));
    }

    #[test]
    fn audience_executive() {
        assert!(matches!(infer_audience("exec-updates", false), Audience::Executive));
        assert!(matches!(infer_audience("leadership", false), Audience::Executive));
    }

    #[test]
    fn audience_team_default() {
        assert!(matches!(infer_audience("random", false), Audience::Team));
        assert!(matches!(infer_audience("general", false), Audience::Team));
    }

    #[test]
    fn audience_direct_from_is_dm() {
        assert!(matches!(infer_audience("D0123456789", true), Audience::Direct));
        assert!(matches!(infer_audience("anything", true), Audience::Direct));
    }
}

/// Audience type inferred from channel name conventions.
#[derive(Debug, Clone, Copy)]
enum Audience {
    /// Engineering channels: technical specifics, brevity, code references ok
    Engineering,
    /// Leadership/executive channels: outcomes, risks, recommendations
    Executive,
    /// General team channels: informal, conversational
    Team,
    /// DM or unknown: match the energy of the sender
    Direct,
}

/// Infer audience from channel name patterns and DM flag.
fn infer_audience(channel_name: &str, is_dm: bool) -> Audience {
    if is_dm {
        return Audience::Direct;
    }

    let lower = channel_name.to_lowercase();

    // Engineering patterns
    if lower.contains("eng") || lower.contains("dev") || lower.contains("infra")
        || lower.contains("platform") || lower.contains("backend") || lower.contains("frontend")
        || lower.contains("deploy") || lower.contains("ci-cd") || lower.contains("code")
        || lower.contains("tech") || lower.contains("sre") || lower.contains("oncall")
    {
        return Audience::Engineering;
    }

    // Executive/leadership patterns
    if lower.contains("exec") || lower.contains("leadership") || lower.contains("board")
        || lower.contains("stakeholder") || lower.contains("c-suite") || lower.contains("strategy")
    {
        return Audience::Executive;
    }

    Audience::Team
}

fn build_framing(task_type: TaskType, channel: &str, is_dm: bool) -> String {
    let audience = infer_audience(channel, is_dm);

    let task_framing = match task_type {
        TaskType::Respond => "You're responding to a live message.",
        TaskType::Digest => "You're compiling a digest of recent activity. Summarize what happened, highlight what matters, skip noise.",
        TaskType::Update => "You're writing a status update. Be clear about what's done, what's in progress, and what's blocked.",
    };

    let audience_framing = match audience {
        Audience::Engineering => "\
            Your audience is engineers. Be technical and concise. \
            Reference specific systems, PRs, or tickets when relevant. \
            Skip high-level fluff — they want specifics. \
            Brevity is respect for their time.",
        Audience::Executive => "\
            Your audience is leadership. Lead with outcomes and risks, not process details. \
            Use plain language — no jargon unless it's domain-specific and necessary. \
            Surface decisions needed and recommendations. \
            Keep it structured: what happened, what it means, what's next.",
        Audience::Team => "\
            This is a team channel. Be conversational and informal. \
            Match the energy of the conversation. \
            It's ok to be brief — a sentence or emoji reaction is often better than a paragraph.",
        Audience::Direct => "\
            This is a direct conversation. Match the energy and formality of the sender. \
            Be helpful and personal. Ask clarifying questions if needed.",
    };

    format!(
        "This message is from channel {channel}.\n\n\
         {task_framing}\n\n\
         {audience_framing}\n\n\
         Use the tools available to you. You can call multiple tools at once — for example, \
         react AND reply, or react AND dm_user. Match the tool to what's being asked: \
         if someone asks for a DM, use dm_user; if someone asks a question, use reply; \
         if something just needs acknowledgment, react is fine on its own.\n\n\
         Only say things you actually know. \
         Never fabricate people, projects, or facts. If you don't have context, say so."
    )
}
