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
}

/// Compile full context for a model call in response to a live event.
pub async fn compile(
    event: &DelegateEvent,
    workspace: &Path,
    task_type: TaskType,
    recent_logs: &str,
    token_budget: usize,
) -> Result<CompiledContext> {
    // 1. Load always-on files
    let identity = load_file(&workspace.join("IDENTITY.md")).await;
    let intents = load_file(&workspace.join("INTENTS.md")).await;
    let memory = load_file(&workspace.join("MEMORY.md")).await;
    let skills = load_skills(&workspace.join("skills")).await;

    // 2. Extract search terms from the event content
    let terms = extract_terms(&event.content);

    // 3. Build bias terms from intents (extract key phrases)
    let bias_terms = extract_terms(&intents);

    // 4. Run retrieval
    let retrieval_results = retrieve(workspace, &terms, &bias_terms, 15, 3).await?;

    // 5. Calculate token budget for retrieved content
    let fixed_tokens = estimate_tokens(&identity)
        + estimate_tokens(&intents)
        + estimate_tokens(&memory)
        + estimate_tokens(&event.content)
        + estimate_tokens(recent_logs)
        + 500; // framing overhead

    let retrieval_budget = token_budget.saturating_sub(fixed_tokens);
    let retrieved = format_retrieved_content(&retrieval_results, retrieval_budget);

    // 6. Build trigger description
    let trigger = format!(
        "Channel: {}\nFrom: {}\nTime: {}\n\n{}",
        event.channel, event.user, event.timestamp, event.content
    );

    // 7. Build framing
    let framing = build_framing(task_type, &event.channel);

    Ok(CompiledContext {
        identity,
        intents,
        memory,
        skills,
        retrieved,
        trigger,
        framing,
    })
}

/// Assemble a CompiledContext into (system_prompt, user_prompt).
pub fn to_prompt(ctx: &CompiledContext, recent_logs: &str) -> (String, String) {
    let mut system_parts = Vec::new();

    system_parts.push(format!("# Team Briefing\n{}", ctx.identity));

    if !ctx.skills.is_empty() {
        let mut skills_section = String::from("\n# Skills\n\nThese are your capabilities. If something isn't listed here, you can't do it.\n");
        for skill in &ctx.skills {
            skills_section.push_str(&format!("\n---\n\n{}", skill.full_content));
        }
        skills_section.push_str("\n---\n\nWhen someone asks you to do something not listed above, say \"I can't do that yet\" and suggest what you *can* do instead.");
        system_parts.push(skills_section);
    }

    if !ctx.intents.is_empty() {
        system_parts.push(format!("\n# Active Intents\n{}", ctx.intents));
    }

    system_parts.push(format!("\n# Knowledge Index\n{}", ctx.memory));

    if !ctx.retrieved.is_empty() {
        system_parts.push(format!("\n# Retrieved Context\n{}", ctx.retrieved));
    }

    if !recent_logs.is_empty() {
        system_parts.push(format!("\n# Recent Activity\n{}", recent_logs));
    }

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

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for word in cleaned.split_whitespace() {
        let lower = word.to_lowercase();
        if lower.len() > 2 && !stop_words.contains(lower.as_str()) && seen.insert(lower.clone()) {
            result.push(lower);
        }
    }

    result
}

fn build_framing(task_type: TaskType, channel: &str) -> String {
    let task_framing = match task_type {
        TaskType::Respond => "You're responding to a live message. Be helpful, concise, and match the energy of what you're responding to.",
        TaskType::Digest => "You're compiling a digest of recent activity. Summarize what happened, highlight what matters, skip noise.",
        TaskType::Update => "You're writing a status update. Be clear about what's done, what's in progress, and what's blocked.",
    };

    format!(
        "This message is from channel {channel}.\n\n\
         {task_framing}\n\n\
         Use the tools available to you. Only say things you actually know. \
         Never fabricate people, projects, or facts. If you don't have context, say so."
    )
}
