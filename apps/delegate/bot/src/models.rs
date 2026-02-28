use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    /// The raw assistant message for appending to conversation history
    pub raw_assistant_message: Value,
}

#[derive(Debug, Clone)]
pub struct CompleteOptions {
    pub system: String,
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub tools: Option<Vec<Value>>,
}

/// Options for multi-turn completion with explicit message history.
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub system: String,
    pub messages: Vec<Value>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Clone)]
pub enum ModelClient {
    Anthropic { api_key: String },
    OpenAI { api_key: String },
}

impl ModelClient {
    pub fn new(provider: &str) -> Result<Self> {
        match provider {
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY")
                    .map_err(|_| anyhow!("ANTHROPIC_API_KEY not set"))?;
                Ok(Self::Anthropic { api_key })
            }
            "openai" => {
                let api_key = std::env::var("OPENAI_API_KEY")
                    .map_err(|_| anyhow!("OPENAI_API_KEY not set"))?;
                Ok(Self::OpenAI { api_key })
            }
            other => Err(anyhow!("Unknown provider: {other}")),
        }
    }

    pub async fn complete(&self, opts: CompleteOptions) -> Result<ModelResponse> {
        let mut last_err = None;
        for attempt in 0..3 {
            let result = match self {
                Self::Anthropic { api_key } => complete_anthropic(api_key, opts.clone()).await,
                Self::OpenAI { api_key } => complete_openai(api_key, opts.clone()).await,
            };
            match result {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let msg = e.to_string();
                    // Don't retry on auth errors or bad requests
                    if msg.contains("401") || msg.contains("403") || msg.contains("400") {
                        return Err(e);
                    }
                    warn!(attempt = attempt + 1, error = %e, "LLM call failed, retrying");
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("LLM call failed after retries")))
    }

    /// Multi-turn completion with explicit message history.
    pub async fn chat(&self, opts: ChatOptions) -> Result<ModelResponse> {
        let mut last_err = None;
        for attempt in 0..3 {
            let result = match self {
                Self::Anthropic { api_key } => chat_anthropic(api_key, opts.clone()).await,
                Self::OpenAI { api_key } => chat_openai(api_key, opts.clone()).await,
            };
            match result {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("401") || msg.contains("403") || msg.contains("400") {
                        return Err(e);
                    }
                    warn!(attempt = attempt + 1, error = %e, "LLM chat call failed, retrying");
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("LLM chat call failed after retries")))
    }
}

async fn complete_anthropic(api_key: &str, opts: CompleteOptions) -> Result<ModelResponse> {
    let model = opts.model.as_deref().unwrap_or("claude-sonnet-4-6");
    let max_tokens = opts.max_tokens.unwrap_or(4096);
    let temperature = opts.temperature.unwrap_or(0.7);

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "system": opts.system,
        "messages": [{"role": "user", "content": opts.prompt}]
    });

    if let Some(tools) = &opts.tools {
        // Convert OpenAI-style tool defs to Anthropic format
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                Some(serde_json::json!({
                    "name": func["name"],
                    "description": func["description"],
                    "input_schema": func["parameters"]
                }))
            })
            .collect();
        body["tools"] = serde_json::json!(anthropic_tools);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let start = Instant::now();

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let resp_body: Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("Anthropic API error ({}): {}", status, err_msg));
    }

    let mut content = String::new();
    let mut tool_calls = Vec::new();

    if let Some(blocks) = resp_body["content"].as_array() {
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(text);
                    }
                }
                Some("tool_use") => {
                    if let Some(name) = block["name"].as_str() {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: name.to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let input_tokens = resp_body["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["output_tokens"].as_u64().unwrap_or(0);

    Ok(ModelResponse {
        content,
        tool_calls,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
        raw_assistant_message: resp_body.clone(),
    })
}

/// Returns true for OpenAI reasoning models that use internal reasoning tokens.
fn is_reasoning_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.starts_with("gpt-5")
}

async fn complete_openai(api_key: &str, opts: CompleteOptions) -> Result<ModelResponse> {
    let model = opts.model.as_deref().unwrap_or("gpt-4o");
    let reasoning = is_reasoning_model(model);

    let base_tokens = opts.max_tokens.unwrap_or(4096);
    let max_tokens = if reasoning { base_tokens * 4 } else { base_tokens };

    let system_role = if reasoning { "developer" } else { "system" };

    let mut body = serde_json::json!({
        "model": model,
        "max_completion_tokens": max_tokens,
        "messages": [
            {"role": system_role, "content": opts.system},
            {"role": "user", "content": opts.prompt}
        ]
    });

    if !reasoning {
        if let Some(temp) = opts.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
    }

    if let Some(tools) = &opts.tools {
        body["tools"] = serde_json::json!(tools);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let start = Instant::now();

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let resp_body: Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("OpenAI API error ({}): {}", status, err_msg));
    }

    let message = &resp_body["choices"][0]["message"];

    let content = message["content"].as_str().unwrap_or("").to_string();

    let mut tool_calls = Vec::new();
    if let Some(calls) = message["tool_calls"].as_array() {
        for call in calls {
            if let Some(func) = call.get("function") {
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = func["name"].as_str().unwrap_or("").to_string();
                let args_str = func["arguments"].as_str().unwrap_or("{}");
                let arguments: Value =
                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                tool_calls.push(ToolCall { id, name, arguments });
            }
        }
    }

    let input_tokens = resp_body["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["completion_tokens"]
        .as_u64()
        .unwrap_or(0);

    Ok(ModelResponse {
        content,
        tool_calls,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
        raw_assistant_message: message.clone(),
    })
}

async fn chat_openai(api_key: &str, opts: ChatOptions) -> Result<ModelResponse> {
    let model = opts.model.as_deref().unwrap_or("gpt-4o");
    let reasoning = is_reasoning_model(model);

    let base_tokens = opts.max_tokens.unwrap_or(4096);
    let max_tokens = if reasoning { base_tokens * 4 } else { base_tokens };

    let system_role = if reasoning { "developer" } else { "system" };

    let mut messages = vec![serde_json::json!({"role": system_role, "content": opts.system})];
    messages.extend(opts.messages);

    let mut body = serde_json::json!({
        "model": model,
        "max_completion_tokens": max_tokens,
        "messages": messages
    });

    if !reasoning {
        if let Some(temp) = opts.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
    }

    if let Some(tools) = &opts.tools {
        body["tools"] = serde_json::json!(tools);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let start = Instant::now();

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let resp_body: Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("OpenAI API error ({}): {}", status, err_msg));
    }

    let message = &resp_body["choices"][0]["message"];
    let content = message["content"].as_str().unwrap_or("").to_string();

    let mut tool_calls = Vec::new();
    if let Some(calls) = message["tool_calls"].as_array() {
        for call in calls {
            if let Some(func) = call.get("function") {
                let id = call["id"].as_str().unwrap_or("").to_string();
                let name = func["name"].as_str().unwrap_or("").to_string();
                let args_str = func["arguments"].as_str().unwrap_or("{}");
                let arguments: Value =
                    serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                tool_calls.push(ToolCall { id, name, arguments });
            }
        }
    }

    let input_tokens = resp_body["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["completion_tokens"]
        .as_u64()
        .unwrap_or(0);

    Ok(ModelResponse {
        content,
        tool_calls,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
        raw_assistant_message: message.clone(),
    })
}

async fn chat_anthropic(api_key: &str, opts: ChatOptions) -> Result<ModelResponse> {
    let model = opts.model.as_deref().unwrap_or("claude-sonnet-4-6");
    let max_tokens = opts.max_tokens.unwrap_or(4096);
    let temperature = opts.temperature.unwrap_or(0.7);

    // Convert OpenAI-style messages to Anthropic format.
    // The main loop sends: user, assistant (raw), tool results (role=tool).
    // Anthropic expects: user, assistant (content blocks), user (tool_result blocks).
    let messages = convert_messages_to_anthropic(&opts.messages);

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "system": opts.system,
        "messages": messages
    });

    if let Some(tools) = &opts.tools {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                Some(serde_json::json!({
                    "name": func["name"],
                    "description": func["description"],
                    "input_schema": func["parameters"]
                }))
            })
            .collect();
        body["tools"] = serde_json::json!(anthropic_tools);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let start = Instant::now();

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let resp_body: Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("Anthropic API error ({}): {}", status, err_msg));
    }

    let mut content = String::new();
    let mut tool_calls = Vec::new();

    if let Some(blocks) = resp_body["content"].as_array() {
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(text);
                    }
                }
                Some("tool_use") => {
                    if let Some(name) = block["name"].as_str() {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: name.to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let input_tokens = resp_body["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["output_tokens"].as_u64().unwrap_or(0);

    Ok(ModelResponse {
        content,
        tool_calls,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
        raw_assistant_message: resp_body.clone(),
    })
}

/// Convert OpenAI-style message history to Anthropic format.
///
/// Input format (from main.rs multi-turn loop):
///   - { role: "user", content: "..." }
///   - { role: "assistant", ... }  (raw Anthropic response with content blocks)
///   - { role: "tool", tool_call_id: "...", content: "..." }  (one per tool result)
///
/// Anthropic format:
///   - { role: "user", content: "..." }
///   - { role: "assistant", content: [...tool_use blocks...] }
///   - { role: "user", content: [...tool_result blocks...] }
fn convert_messages_to_anthropic(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();

    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];
        let role = msg["role"].as_str().unwrap_or("");

        match role {
            "user" => {
                result.push(msg.clone());
                i += 1;
            }
            "assistant" => {
                // The raw_assistant_message from Anthropic already has the right format.
                // Extract just the content array for the assistant turn.
                if let Some(content) = msg.get("content") {
                    result.push(serde_json::json!({
                        "role": "assistant",
                        "content": content
                    }));
                } else {
                    // Fallback: pass through as-is
                    result.push(serde_json::json!({
                        "role": "assistant",
                        "content": msg["content"].clone()
                    }));
                }
                i += 1;
            }
            "tool" => {
                // Collect consecutive tool results into a single user message
                // with tool_result content blocks (Anthropic format)
                let mut tool_results = Vec::new();
                while i < messages.len() && messages[i]["role"].as_str() == Some("tool") {
                    let tool_msg = &messages[i];
                    tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_msg["tool_call_id"].as_str().unwrap_or(""),
                        "content": tool_msg["content"].as_str().unwrap_or("")
                    }));
                    i += 1;
                }
                result.push(serde_json::json!({
                    "role": "user",
                    "content": tool_results
                }));
            }
            _ => {
                i += 1;
            }
        }
    }

    result
}

/// Rough token count estimate. Uses ~4 chars per token heuristic.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Tool definitions for the Delegate bot.
pub fn delegate_tools() -> Vec<Value> {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "react",
                "description": "Add an emoji reaction to the triggering message. Use this to acknowledge, signal thinking, show agreement, etc. Choose the emoji based on context — don't always use the same one.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "emoji": {
                            "type": "string",
                            "description": "Emoji name without colons. Examples: eyes, thumbsup, thinking_face, white_check_mark, wave, raised_hands, warning, memo, rocket"
                        }
                    },
                    "required": ["emoji"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "reply",
                "description": "Reply to the triggering message in a thread. Use this when a substantive response is warranted — answering a question, flagging a risk, providing context, etc.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The message text to post as a threaded reply"
                        }
                    },
                    "required": ["text"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "post",
                "description": "Post a new message to any channel (not as a thread reply). Use this to proactively surface information in a different channel, e.g. alerting #platform-eng about something mentioned in #billing-migration.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {
                            "type": "string",
                            "description": "The channel ID to post to"
                        },
                        "text": {
                            "type": "string",
                            "description": "The message text to post"
                        }
                    },
                    "required": ["channel", "text"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "no_action",
                "description": "Explicitly take no action. Use this when the message doesn't warrant any response or reaction — sometimes the right move is to stay quiet.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "reason": {
                            "type": "string",
                            "description": "Brief internal note on why no action was taken"
                        }
                    },
                    "required": ["reason"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_skill",
                "description": "Create or update a skill in your skill registry. Skills are behavioral patterns that guide how you use your tools. Use this when you notice a recurring pattern worth codifying — a type of message you handle the same way, a workflow you want to remember, or guidance from the team about how to behave. Skills are NOT new tools — they are instructions for how to use your existing tools (react, reply, post, no_action) in specific situations.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Skill name in kebab-case (e.g. summarize-thread, welcome-new-member, flag-blocker)"
                        },
                        "description": {
                            "type": "string",
                            "description": "One-line description of when this skill applies"
                        },
                        "content": {
                            "type": "string",
                            "description": "Full skill instructions in markdown. Include: when to use, how to use your existing tools to accomplish it, what NOT to do, and any examples."
                        }
                    },
                    "required": ["name", "description", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file from your workspace. Path is relative to the workspace root. Use this to check current state before making changes — e.g. read tickets.json before updating it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path within workspace (e.g. tickets.json, memory/people.md)"
                        }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write a file to your workspace. Path is relative to the workspace root. Creates parent directories if needed. Use this to persist state — tickets, notes, memory, data.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path within workspace (e.g. tickets.json, memory/people.md)"
                        },
                        "content": {
                            "type": "string",
                            "description": "File content to write"
                        }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "dm_user",
                "description": "Send a direct message to a specific user. Use this for private nudges, approval requests, or sensitive information that shouldn't be in a public channel. The user will receive it as a DM from the bot.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user": {
                            "type": "string",
                            "description": "User ID to DM (e.g. U012345)"
                        },
                        "text": {
                            "type": "string",
                            "description": "Message text to send as a DM"
                        }
                    },
                    "required": ["user", "text"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "channel_history",
                "description": "Read recent messages from a Slack channel. Returns the most recent messages (newest first). Use this to get broader context about what's happening in a channel beyond the current thread.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "channel": {
                            "type": "string",
                            "description": "Channel ID to read history from"
                        },
                        "count": {
                            "type": "integer",
                            "description": "Number of messages to fetch (default 20, max 50)"
                        }
                    },
                    "required": ["channel"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "save_memory",
                "description": "Persist a piece of knowledge to long-term memory. Writes to memory/{topic}.md and automatically updates MEMORY.md as a structured index. Use this when you learn something worth retaining: people's roles, project context, team preferences, decisions made, or corrections from the team. If the topic already exists, it will be overwritten — read it first if you want to append.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "Topic slug in kebab-case (e.g. people, billing-migration, team-norms, standup-format)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Markdown content to persist. Be structured: use headings, bullets, and dates for context."
                        },
                        "summary": {
                            "type": "string",
                            "description": "One-line summary for the MEMORY.md index entry (e.g. 'Team members, roles, and working styles')"
                        }
                    },
                    "required": ["topic", "content", "summary"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "recall_memory",
                "description": "Search your long-term memory for information about a topic. Scans all memory files for matching content. Use this when someone asks 'what do you know about X?' or when you need context you might have stored previously. Returns matching excerpts from memory files.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What to search for in memory (e.g. 'billing migration', 'Alan', 'team standup format')"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "update_intents",
                "description": "Update INTENTS.md based on your observations. Use this when you notice a new project, priority shift, or recurring theme that should influence how you triage and respond. Read INTENTS.md first to understand the current state before modifying. Provide the FULL updated content — this replaces the file entirely.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "Full updated INTENTS.md content in markdown"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Brief explanation of what changed and why (logged for auditability)"
                        }
                    },
                    "required": ["content", "reason"]
                }
            }
        }
    ])
    .as_array()
    .unwrap()
    .clone()
}
