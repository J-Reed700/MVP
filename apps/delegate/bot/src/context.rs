use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::event::DelegateEvent;
use crate::models::estimate_tokens;
use crate::retriever::{format_retrieved_content, retrieve};

#[derive(Debug, Clone)]
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
) -> Result<CompiledContext> {
    // 1. Load always-on files (tiers 1-2: never cut)
    let identity = load_file(&workspace.join("IDENTITY.md")).await;
    let intents = load_file(&workspace.join("INTENTS.md")).await;
    let skills = load_skills(&workspace.join("skills")).await;

    // 2. Build framing (tier 3: never cut)
    let framing = build_framing(task_type, &event.channel);

    // 3. Build trigger (tier 4: never cut)
    let trigger = format!(
        "Channel: {}\nFrom: {}\nTime: {}\n\n{}",
        event.channel, event.user, event.timestamp, event.content
    );

    // 4. Calculate protected (never-cut) token cost
    let skills_text: String = skills.iter().map(|s| s.full_content.as_str()).collect::<Vec<_>>().join("\n");
    let protected_tokens = estimate_tokens(&identity)
        + estimate_tokens(&intents)
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
fn extract_terms(text: &str) -> Vec<String> {
    let stop_words: HashSet<&str> = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "can", "shall", "to", "of", "in", "for",
        "on", "with", "at", "by", "from", "as", "into", "through", "during",
        "before", "after", "above", "below", "between", "and", "but", "or",
        "not", "no", "nor", "so", "yet", "both", "either", "neither", "each",
        "every", "all", "any", "few", "more", "most", "other", "some", "such",
        "than", "too", "very", "just", "about", "up", "out", "if", "then",
        "that", "this", "these", "those", "what", "which", "who", "whom",
        "how", "when", "where", "why", "it", "its", "he", "she", "they",
        "them", "his", "her", "their", "my", "your", "our", "me", "we",
        "i", "you", "him", "us",
    ]
    .into_iter()
    .collect();

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    // Extract backtick-quoted phrases as compound terms (e.g. `billing migration`)
    for cap in regex::Regex::new(r"`([^`]+)`").unwrap().captures_iter(text) {
        let phrase = cap[1].trim().to_lowercase();
        if phrase.len() > 2 && seen.insert(phrase.clone()) {
            result.push(phrase);
        }
    }

    // Extract individual words
    let cleaned: String = text
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
        if lower.len() > 2 && !stop_words.contains(lower.as_str()) && seen.insert(lower.clone()) {
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
    let start = text.len() - max_chars;
    // Find next newline to avoid splitting mid-line
    let start = text[start..].find('\n').map(|i| start + i + 1).unwrap_or(start);
    format!("[...earlier entries truncated]\n{}", &text[start..])
}

/// Truncate text keeping the head (top-level structure). Returns with a suffix marker.
fn truncate_keep_head(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Find last newline before limit to avoid splitting mid-line
    let end = text[..max_chars].rfind('\n').unwrap_or(max_chars);
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

/// Infer audience from Slack channel name patterns.
fn infer_audience(channel_name: &str) -> Audience {
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

    // DM channels start with D in Slack
    if lower.starts_with('d') && lower.len() > 8 && lower.chars().all(|c| c.is_alphanumeric()) {
        return Audience::Direct;
    }

    Audience::Team
}

fn build_framing(task_type: TaskType, channel: &str) -> String {
    let audience = infer_audience(channel);

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
         Use the tools available to you. Only say things you actually know. \
         Never fabricate people, projects, or facts. If you don't have context, say so."
    )
}
