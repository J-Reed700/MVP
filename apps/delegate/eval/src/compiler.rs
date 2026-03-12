use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::models::estimate_tokens;
use crate::retriever::{format_retrieved_content, retrieve};

#[derive(Debug, Clone)]
pub struct AudienceProfile {
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Copy)]
pub enum TaskType {
    Respond,
    Update,
    Triage,
    Lifecycle,
}

#[derive(Debug, Clone)]
pub struct Trigger {
    pub r#type: String,
    pub content: String,
    pub channel: Option<String>,
    pub user: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompiledContext {
    pub identity: String,
    pub intents: String,
    pub memory: String,
    pub retrieved: String,
    pub trigger: String,
    pub framing: String,
    pub token_budget: usize,
}

pub struct CompileOptions {
    pub trigger: Trigger,
    pub workspace: String,
    pub include_intents: bool,
    pub retrieval_terms: Vec<String>,
    pub retrieval_bias: Vec<String>,
    pub audience: AudienceProfile,
    pub task_type: TaskType,
    pub task: String,
    pub token_budget: usize,
}

/// The ContextCompiler — assembles context for a model call.
pub async fn compile(opts: CompileOptions) -> Result<CompiledContext> {
    let workspace = Path::new(&opts.workspace);

    // 1. Load always-on files
    let identity = load_file(&workspace.join("IDENTITY.md")).await;
    let intents = if opts.include_intents {
        load_file(&workspace.join("INTENTS.md")).await
    } else {
        String::new()
    };
    let memory = load_file(&workspace.join("MEMORY.md")).await;

    // 2. Extract search terms from trigger
    let trigger_terms = extract_terms(&opts.trigger.content);
    let mut all_search_terms: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for term in trigger_terms.into_iter().chain(opts.retrieval_terms.into_iter()) {
        if seen.insert(term.clone()) {
            all_search_terms.push(term);
        }
    }

    // 3. Run retrieval with intent bias
    let retrieval_results = retrieve(
        workspace,
        &all_search_terms,
        &opts.retrieval_bias,
        15,
        3,
    )
    .await?;

    // 4. Calculate token budget for retrieved content
    let fixed_content_tokens = estimate_tokens(&identity)
        + estimate_tokens(&intents)
        + estimate_tokens(&memory)
        + estimate_tokens(&opts.trigger.content)
        + 500; // framing overhead

    let retrieval_budget = opts.token_budget.saturating_sub(fixed_content_tokens);
    let retrieved = format_retrieved_content(&retrieval_results, retrieval_budget);

    // 5. Construct framing
    let framing = build_framing(&opts.audience, opts.task_type, &opts.task, &opts.trigger);

    Ok(CompiledContext {
        identity,
        intents,
        memory,
        retrieved,
        trigger: format_trigger(&opts.trigger),
        framing,
        token_budget: opts.token_budget,
    })
}

/// Assemble a CompiledContext into a system prompt and user prompt.
pub fn to_prompt(ctx: &CompiledContext) -> (String, String) {
    let mut system_parts = Vec::new();

    system_parts.push(format!("# Team Briefing\n{}", ctx.identity));

    if !ctx.intents.is_empty() {
        system_parts.push(format!("\n# Active Intents\n{}", ctx.intents));
    }

    system_parts.push(format!("\n# Knowledge Index\n{}", ctx.memory));

    if !ctx.retrieved.is_empty() {
        system_parts.push(format!("\n# Retrieved Context\n{}", ctx.retrieved));
    }

    system_parts.push(format!("\n# Your Role\n{}", ctx.framing));

    let system = system_parts.join("\n");
    let prompt = ctx.trigger.clone();

    (system, prompt)
}

async fn load_file(path: &Path) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

/// Extract meaningful search terms from a trigger message.
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
        .map(|c| if c.is_alphanumeric() || c == '-' || c.is_whitespace() { c } else { ' ' })
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

fn format_trigger(trigger: &Trigger) -> String {
    let mut parts = Vec::new();
    if let Some(ref channel) = trigger.channel {
        parts.push(format!("Channel: {channel}"));
    }
    if let Some(ref user) = trigger.user {
        parts.push(format!("From: {user}"));
    }
    if let Some(ref timestamp) = trigger.timestamp {
        parts.push(format!("Time: {timestamp}"));
    }
    parts.push(format!("\n{}", trigger.content));
    parts.join("\n")
}

fn build_framing(
    audience: &AudienceProfile,
    task_type: TaskType,
    task: &str,
    _trigger: &Trigger,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("You are the acting PM for this team. {task}"));
    lines.push(String::new());

    if !audience.instructions.is_empty() {
        lines.push(format!("## Audience: {}", audience.name));
        lines.push(audience.instructions.clone());
        lines.push(String::new());
    }

    match task_type {
        TaskType::Respond => {
            lines.push("## Instructions".to_string());
            lines.push("- Analyze the situation and decide what actions to take.".to_string());
            lines.push("- Draft any necessary communications.".to_string());
            lines.push("- Consider the team's current priorities and dynamics.".to_string());
            lines.push("- Be specific about who to notify and what to say.".to_string());
        }
        TaskType::Update => {
            lines.push("## Instructions".to_string());
            lines.push("- Write a project status update based on available information.".to_string());
            lines.push("- Tailor your language and emphasis to the target audience.".to_string());
            lines.push("- Include only information relevant to this audience.".to_string());
        }
        TaskType::Triage => {
            lines.push("## Instructions".to_string());
            lines.push("- Classify this event as one of: ignore, queue, act-now.".to_string());
            lines.push("- ignore: No relevance to team priorities. Bot noise, social chat, routine notifications.".to_string());
            lines.push("- queue: Relevant but not time-sensitive. Track it, include in next digest.".to_string());
            lines.push("- act-now: Requires immediate attention. Blockers, escalations, risks materializing.".to_string());
            lines.push("- Respond with ONLY the classification label and a one-sentence reasoning.".to_string());
        }
        TaskType::Lifecycle => {
            lines.push("## Instructions".to_string());
            lines.push("- Evaluate whether this event warrants an update to INTENTS.md.".to_string());
            lines.push("- If yes, provide the complete updated INTENTS.md.".to_string());
            lines.push("- Focus on: trajectory changes, new watch signals, urgency shifts, risk changes.".to_string());
            lines.push("- Keep updates proportional — small events get small changes, big events get big changes.".to_string());
            lines.push("- If no update is needed, explain briefly why.".to_string());
        }
    }

    lines.join("\n")
}
