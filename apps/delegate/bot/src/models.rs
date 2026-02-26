use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;

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
        match self {
            Self::Anthropic { api_key } => complete_anthropic(api_key, opts).await,
            Self::OpenAI { api_key } => complete_openai(api_key, opts).await,
        }
    }

    /// Multi-turn completion with explicit message history.
    pub async fn chat(&self, opts: ChatOptions) -> Result<ModelResponse> {
        match self {
            Self::Anthropic { api_key } => chat_anthropic(api_key, opts).await,
            Self::OpenAI { api_key } => chat_openai(api_key, opts).await,
        }
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

async fn chat_anthropic(_api_key: &str, _opts: ChatOptions) -> Result<ModelResponse> {
    // TODO: implement Anthropic multi-turn when needed
    Err(anyhow!("Anthropic multi-turn chat not yet implemented"))
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
        }
    ])
    .as_array()
    .unwrap()
    .clone()
}
