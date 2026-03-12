use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::event::DelegateEvent;
use crate::models::estimate_tokens;
use crate::registry::ToolScope;
use crate::retriever::{format_retrieved_content, retrieve};
use crate::text;

/// A tool defined within a skill's SKILL.md frontmatter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SkillTool {
    pub name: String,
    pub skill_name: String,
    pub description: String,
    pub schema: Value, // OpenAI function-calling format
    pub handler: SkillHandler,
}

/// How a skill-defined tool executes.
#[derive(Debug, Clone)]
pub enum SkillHandler {
    Script {
        language: String,
        script_path: PathBuf,
    },
    Http {
        method: String,
        url_template: String,
        headers: HashMap<String, String>,
        body_template: Option<String>,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub full_content: String,
    pub tools: Vec<SkillTool>,
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
    scope: ToolScope,
) -> Result<CompiledContext> {
    // 1. Load always-on files (tiers 1-2: never cut)
    let identity = load_file(&workspace.join("IDENTITY.md")).await;
    let intents = load_file(&workspace.join("INTENTS.md")).await;
    let heartbeat = load_file(&workspace.join("HEARTBEAT.md")).await;
    let skills = load_skills(&workspace.join("skills"), None).await;

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
    let playbook = crate::registry::tool_playbook(scope);
    // Skills are progressive-disclosure: only name+description in system prompt
    let skills_text: String = skills.iter().map(|s| format!("- {}: {}", s.name, s.description)).collect::<Vec<_>>().join("\n");
    let protected_tokens = estimate_tokens(&identity)
        + estimate_tokens(&intents)
        + estimate_tokens(&heartbeat)
        + estimate_tokens(&framing)
        + estimate_tokens(&trigger)
        + estimate_tokens(&skills_text)
        + estimate_tokens(&playbook)
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
pub fn to_prompt(ctx: &CompiledContext, scope: ToolScope) -> (String, String) {
    let mut system_parts = Vec::new();

    // Priority 1: Identity (never cut)
    system_parts.push(format!("# Team Briefing\n{}", ctx.identity));

    if !ctx.skills.is_empty() {
        let mut skills_section = String::from("\n# Skills\n\nThese are your loaded capabilities:\n");
        for skill in &ctx.skills {
            if skill.description.is_empty() {
                skills_section.push_str(&format!("- **{}**\n", skill.name));
            } else {
                skills_section.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
            }
        }
        skills_section.push_str("\nUse `load_skill` to read full skill instructions when you need them.\n");
        system_parts.push(skills_section);
    }

    // Self-extension awareness (hardcoded — never overwritten by user config)
    system_parts.push("\n# Self-Extension\n\
        You are not limited to your current tools. When someone asks for something you can't do yet, \
        **proactively offer to build the capability** using `create_skill`.\n\n\
        Example: \"I can't check PRs yet, but I can build myself that capability right now. Want me to?\"\n\n\
        Use judgment: only offer to create a skill when it's something the team will need again. \
        For one-off requests, just use `run_script` or `http_request` directly.".to_string());

    // Platform formatting rules
    system_parts.push("\n# Formatting\n\
        You are posting in Slack. Use Slack's native mrkdwn, NOT standard markdown:\n\
        - Bold: *bold* (single asterisks, NOT **bold**)\n\
        - Italic: _italic_ (underscores)\n\
        - Strikethrough: ~strikethrough~\n\
        - Code: `code` or ```code block```\n\
        - Lists: use bullet characters or dashes\n\
        - Links: <url|display text>\n\
        Never use **double asterisks** — they render as literal asterisks in Slack.".to_string());

    // Tool Playbook — tells the model when to use each tool
    system_parts.push(format!("\n{}", crate::registry::tool_playbook(scope)));

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
///
/// Filtering logic for skills with `required_credentials`:
/// - If `connected_providers` is None → load all skills (backward compat, no OAuth at all).
/// - If a skill's required provider is in `connected_providers` → load (OAuth connected).
/// - If a skill's required provider is NOT in `configured_providers` → load (no OAuth config
///   for this provider, so it uses env vars).
/// - Otherwise → skip (OAuth is configured but user hasn't connected yet).
///
/// Skills without `required_credentials` always load.
pub async fn load_skills(
    skills_dir: &Path,
    connected_providers: Option<&HashSet<String>>,
) -> Vec<Skill> {
    load_skills_filtered(skills_dir, connected_providers, None).await
}

/// Load skills with explicit configured provider filtering.
pub async fn load_skills_filtered(
    skills_dir: &Path,
    connected_providers: Option<&HashSet<String>>,
    configured_providers: Option<&HashSet<String>>,
) -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut entries = match tokio::fs::read_dir(skills_dir).await {
        Ok(e) => e,
        Err(_) => return skills,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let skill_file = entry.path().join("SKILL.md");
        if let Ok(content) = tokio::fs::read_to_string(&skill_file).await {
            let parsed = parse_skill_frontmatter(&content);

            // Filter by required_credentials
            if let Some(ref req_cred) = parsed.required_credentials {
                if let Some(connected) = connected_providers {
                    if !connected.contains(req_cred) {
                        // Not OAuth-connected. Check if provider has OAuth config.
                        if let Some(configured) = configured_providers {
                            if configured.contains(req_cred) {
                                // OAuth IS configured but not connected yet — skip
                                continue;
                            }
                            // OAuth NOT configured — fall through, will use env vars
                        }
                        // No configured_providers info — fall through (backward compat)
                    }
                }
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            let skill_name = parsed.name.unwrap_or_else(|| dir_name.clone());
            let skill_dir = entry.path();

            // Parse tools_json if present
            let tools = parse_skill_tools(&skill_name, &skill_dir, parsed.tools_json.as_deref());

            skills.push(Skill {
                name: skill_name,
                description: parsed.description.unwrap_or_default(),
                full_content: parsed.body,
                tools,
            });
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Parse tool definitions from a skill's tools_json frontmatter field.
fn parse_skill_tools(skill_name: &str, skill_dir: &Path, tools_json: Option<&str>) -> Vec<SkillTool> {
    let json_str = match tools_json {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };

    let tool_defs: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut tools = Vec::new();
    for def in tool_defs {
        let name = match def["name"].as_str() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let description = def["description"].as_str().unwrap_or("").to_string();
        let params = def.get("parameters").cloned().unwrap_or(serde_json::json!({
            "type": "object", "properties": {}, "required": []
        }));

        // Build OpenAI function-calling schema
        let schema = serde_json::json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": params
            }
        });

        let handler_type = def["handler"].as_str().unwrap_or("script");
        let handler = match handler_type {
            "http" => {
                let method = def["method"].as_str().unwrap_or("GET").to_string();
                let url_template = def["url_template"].as_str().unwrap_or("").to_string();
                let headers: HashMap<String, String> = def["headers"]
                    .as_object()
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();
                let body_template = def["body_template"].as_str().map(|s| s.to_string());
                SkillHandler::Http { method, url_template, headers, body_template }
            }
            _ => {
                // Default: script handler
                let handler_file = def["handler_file"].as_str().unwrap_or("");
                let language = if handler_file.ends_with(".py") {
                    "python".to_string()
                } else {
                    "shell".to_string()
                };
                let script_path = skill_dir.join(handler_file);
                SkillHandler::Script { language, script_path }
            }
        };

        tools.push(SkillTool {
            name,
            skill_name: skill_name.to_string(),
            description,
            schema,
            handler,
        });
    }
    tools
}

/// Parsed result from SKILL.md frontmatter.
struct ParsedSkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    tools_json: Option<String>,
    required_credentials: Option<String>,
    body: String,
}

/// Parse YAML frontmatter from a SKILL.md file.
/// Returns parsed name, description, tools_json, and body after frontmatter.
fn parse_skill_frontmatter(content: &str) -> ParsedSkillFrontmatter {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return ParsedSkillFrontmatter {
            name: None, description: None, tools_json: None,
            required_credentials: None, body: content.to_string(),
        };
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let close = match after_first.find("---") {
        Some(i) => i,
        None => return ParsedSkillFrontmatter {
            name: None, description: None, tools_json: None,
            required_credentials: None, body: content.to_string(),
        },
    };

    let frontmatter = &after_first[..close];
    let body = after_first[close + 3..].trim_start().to_string();

    let mut name = None;
    let mut description = None;
    let mut tools_json = None;
    let mut required_credentials = None;
    let mut in_tools_json = false;
    let mut tools_json_lines: Vec<String> = Vec::new();

    for line in frontmatter.lines() {
        let trimmed_line = line.trim();

        // Detect start of tools_json multiline block
        if trimmed_line.starts_with("tools_json:") {
            let after = trimmed_line.strip_prefix("tools_json:").unwrap().trim();
            if after == "|" || after.is_empty() {
                in_tools_json = true;
                continue;
            } else {
                // Inline value
                tools_json = Some(after.to_string());
                continue;
            }
        }

        if in_tools_json {
            // YAML block scalar: lines must be indented
            if line.starts_with("  ") || line.starts_with('\t') || trimmed_line.is_empty() {
                tools_json_lines.push(line.trim_start().to_string());
            } else {
                // End of block
                in_tools_json = false;
                // Process this line normally below
                if let Some(val) = trimmed_line.strip_prefix("name:") {
                    name = Some(val.trim().to_string());
                } else if let Some(val) = trimmed_line.strip_prefix("description:") {
                    description = Some(val.trim().to_string());
                } else if let Some(val) = trimmed_line.strip_prefix("required_credentials:") {
                    required_credentials = Some(val.trim().to_string());
                }
            }
            continue;
        }

        if let Some(val) = trimmed_line.strip_prefix("name:") {
            name = Some(val.trim().to_string());
        } else if let Some(val) = trimmed_line.strip_prefix("description:") {
            description = Some(val.trim().to_string());
        } else if let Some(val) = trimmed_line.strip_prefix("required_credentials:") {
            required_credentials = Some(val.trim().to_string());
        }
    }

    if !tools_json_lines.is_empty() && tools_json.is_none() {
        tools_json = Some(tools_json_lines.join("\n"));
    }

    ParsedSkillFrontmatter { name, description, tools_json, required_credentials, body }
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
        let parsed = parse_skill_frontmatter(content);
        assert_eq!(parsed.name.unwrap(), "summarize");
        assert_eq!(parsed.description.unwrap(), "Summarize threads");
        assert!(parsed.body.contains("# Summarize"));
        assert!(parsed.tools_json.is_none());
        assert!(parsed.required_credentials.is_none());
    }

    #[test]
    fn parse_skill_with_required_credentials() {
        let content = "---\nname: jira\ndescription: Jira integration\nrequired_credentials: atlassian\ntools_json: |\n  []\n---\n\n# Jira";
        let parsed = parse_skill_frontmatter(content);
        assert_eq!(parsed.name.unwrap(), "jira");
        assert_eq!(parsed.required_credentials.unwrap(), "atlassian");
    }

    #[test]
    fn parse_skill_without_frontmatter() {
        let content = "# Just a skill\n\nNo frontmatter here.";
        let parsed = parse_skill_frontmatter(content);
        assert!(parsed.name.is_none());
        assert!(parsed.description.is_none());
        assert_eq!(parsed.body, content);
    }

    #[test]
    fn parse_skill_with_tools_json() {
        let content = "---\nname: github-prs\ndescription: Check open PRs\ntools_json: |\n  [{\"name\": \"check_prs\", \"description\": \"List PRs\", \"parameters\": {\"type\": \"object\", \"properties\": {}}, \"handler\": \"script\", \"handler_file\": \"check.py\"}]\n---\n\n# Instructions";
        let parsed = parse_skill_frontmatter(content);
        assert_eq!(parsed.name.unwrap(), "github-prs");
        let tools_json = parsed.tools_json.unwrap();
        assert!(tools_json.contains("check_prs"));
    }

    // ── integration skill loading ──

    #[tokio::test]
    async fn load_integration_skills_all_parse() {
        let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let skills = load_skills(&skills_dir, None).await;

        // Verify all 6 integration skills loaded (plus any pre-existing behavioral skills)
        let integration_names: Vec<&str> = vec![
            "confluence", "gmail", "google-calendar", "jira", "linear", "notion",
        ];
        for name in &integration_names {
            let skill = skills.iter().find(|s| s.name == *name);
            assert!(skill.is_some(), "Skill '{}' not found", name);
            let skill = skill.unwrap();
            assert!(!skill.description.is_empty(), "Skill '{}' has empty description", name);
            assert!(!skill.tools.is_empty(), "Skill '{}' has no tools", name);
        }
    }

    #[tokio::test]
    async fn integration_skill_tool_counts() {
        let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let skills = load_skills(&skills_dir, None).await;

        let expected: Vec<(&str, usize)> = vec![
            ("jira", 11),          // search, get, create, update, transition, get_transitions, add_comment, assign, get_sprint, get_boards, link_issues
            ("linear", 9),         // search, get, create, update, add_comment, list_projects, get_cycle, list_members, list_states
            ("notion", 8),         // search, get_page, get_content, create, query_db, append, create_db_entry, update_properties
            ("confluence", 7),     // search, get, create, update, list_spaces, add_comment, get_children
            ("google-calendar", 4),// list_events, get_event, list_calendars, freebusy
            ("gmail", 7),          // list, read, send, draft, labels, get_thread, reply
        ];

        for (name, count) in &expected {
            let skill = skills.iter().find(|s| s.name == *name).unwrap();
            assert_eq!(
                skill.tools.len(), *count,
                "Skill '{}' expected {} tools, got {}", name, count, skill.tools.len()
            );
        }
    }

    #[tokio::test]
    async fn integration_skills_all_http_handlers() {
        let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let skills = load_skills(&skills_dir, None).await;

        let integration_names = ["confluence", "gmail", "google-calendar", "jira", "linear", "notion"];
        for name in &integration_names {
            let skill = skills.iter().find(|s| s.name == *name).unwrap();
            for tool in &skill.tools {
                match &tool.handler {
                    SkillHandler::Http { method, url_template, .. } => {
                        assert!(!url_template.is_empty(), "Tool '{}' has empty url_template", tool.name);
                        assert!(
                            ["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()),
                            "Tool '{}' has invalid method '{}'", tool.name, method
                        );
                    }
                    SkillHandler::Script { .. } => {
                        panic!("Integration tool '{}' in '{}' should be HTTP handler, not script", tool.name, name);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn integration_skills_have_env_var_auth() {
        let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let skills = load_skills(&skills_dir, None).await;

        let integration_names = ["confluence", "gmail", "google-calendar", "jira", "linear", "notion"];
        for name in &integration_names {
            let skill = skills.iter().find(|s| s.name == *name).unwrap();
            let has_env_auth = skill.tools.iter().any(|tool| {
                if let SkillHandler::Http { headers, .. } = &tool.handler {
                    headers.values().any(|v| v.contains("{{env."))
                } else {
                    false
                }
            });
            assert!(has_env_auth, "Skill '{}' has no env var auth in headers", name);
        }
    }

    #[tokio::test]
    async fn integration_skill_schemas_valid() {
        let skills_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let skills = load_skills(&skills_dir, None).await;

        let integration_names = ["confluence", "gmail", "google-calendar", "jira", "linear", "notion"];
        for name in &integration_names {
            let skill = skills.iter().find(|s| s.name == *name).unwrap();
            for tool in &skill.tools {
                // Schema must have OpenAI function-calling format
                assert_eq!(tool.schema["type"], "function", "Tool '{}' schema missing type=function", tool.name);
                assert!(tool.schema["function"]["name"].is_string(), "Tool '{}' schema missing function.name", tool.name);
                assert!(tool.schema["function"]["parameters"].is_object(), "Tool '{}' schema missing parameters", tool.name);
            }
        }
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
         {audience_framing}"
    )
}
