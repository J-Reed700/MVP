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
        body["tools"] = serde_json::json!(to_anthropic_tools(tools));
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

    let (content, tool_calls) = parse_anthropic_response(&resp_body);
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
    let (content, tool_calls) = parse_openai_message(message);

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
    let (content, tool_calls) = parse_openai_message(message);

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
        body["tools"] = serde_json::json!(to_anthropic_tools(tools));
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

    let (content, tool_calls) = parse_anthropic_response(&resp_body);
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

// ── Shared response parsers ────────────────────────────────────────────

/// Parse content text and tool_use blocks from an Anthropic API response body.
fn parse_anthropic_response(resp_body: &Value) -> (String, Vec<ToolCall>) {
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

    (content, tool_calls)
}

/// Parse content and tool calls from an OpenAI message object.
fn parse_openai_message(message: &Value) -> (String, Vec<ToolCall>) {
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

    (content, tool_calls)
}

/// Convert OpenAI-style tool definitions to Anthropic format.
fn to_anthropic_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|t| {
            let func = t.get("function")?;
            Some(serde_json::json!({
                "name": func["name"],
                "description": func["description"],
                "input_schema": func["parameters"]
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_anthropic_text_only() {
        let resp = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello world"}
            ]
        });
        let (content, tools) = parse_anthropic_response(&resp);
        assert_eq!(content, "Hello world");
        assert!(tools.is_empty());
    }

    #[test]
    fn parse_anthropic_with_tool_use() {
        let resp = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me react."},
                {
                    "type": "tool_use",
                    "id": "tu_123",
                    "name": "react",
                    "input": {"emoji": "thumbsup"}
                }
            ]
        });
        let (content, tools) = parse_anthropic_response(&resp);
        assert_eq!(content, "Let me react.");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "react");
        assert_eq!(tools[0].arguments["emoji"], "thumbsup");
    }

    #[test]
    fn parse_anthropic_multiple_tools() {
        let resp = serde_json::json!({
            "content": [
                {"type": "tool_use", "id": "t1", "name": "react", "input": {"emoji": "eyes"}},
                {"type": "tool_use", "id": "t2", "name": "reply", "input": {"text": "on it"}}
            ]
        });
        let (content, tools) = parse_anthropic_response(&resp);
        assert!(content.is_empty());
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn parse_openai_text_only() {
        let msg = serde_json::json!({
            "content": "Hello from GPT",
            "role": "assistant"
        });
        let (content, tools) = parse_openai_message(&msg);
        assert_eq!(content, "Hello from GPT");
        assert!(tools.is_empty());
    }

    #[test]
    fn parse_openai_with_tool_calls() {
        let msg = serde_json::json!({
            "content": null,
            "role": "assistant",
            "tool_calls": [
                {
                    "id": "call_abc",
                    "type": "function",
                    "function": {
                        "name": "react",
                        "arguments": "{\"emoji\": \"wave\"}"
                    }
                }
            ]
        });
        let (content, tools) = parse_openai_message(&msg);
        assert!(content.is_empty());
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "react");
        assert_eq!(tools[0].arguments["emoji"], "wave");
    }

    #[test]
    fn to_anthropic_tools_converts_format() {
        let openai_tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "react",
                "description": "Add a reaction",
                "parameters": {
                    "type": "object",
                    "properties": {"emoji": {"type": "string"}},
                    "required": ["emoji"]
                }
            }
        })];
        let anthropic = to_anthropic_tools(&openai_tools);
        assert_eq!(anthropic.len(), 1);
        assert_eq!(anthropic[0]["name"], "react");
        assert_eq!(anthropic[0]["description"], "Add a reaction");
        assert!(anthropic[0]["input_schema"]["properties"]["emoji"].is_object());
    }

    #[test]
    fn convert_messages_user_assistant_tool() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "Do something"}),
            serde_json::json!({
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "t1", "name": "react", "input": {"emoji": "eyes"}}
                ]
            }),
            serde_json::json!({"role": "tool", "tool_call_id": "t1", "content": "Reacted with :eyes:"}),
        ];
        let converted = convert_messages_to_anthropic(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0]["role"], "user");
        assert_eq!(converted[1]["role"], "assistant");
        // Tool results become a user message with tool_result blocks
        assert_eq!(converted[2]["role"], "user");
        let content = converted[2]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "t1");
    }

    #[test]
    fn convert_messages_consecutive_tools_merged() {
        let messages = vec![
            serde_json::json!({"role": "tool", "tool_call_id": "t1", "content": "result 1"}),
            serde_json::json!({"role": "tool", "tool_call_id": "t2", "content": "result 2"}),
        ];
        let converted = convert_messages_to_anthropic(&messages);
        // Consecutive tool results should merge into one user message
        assert_eq!(converted.len(), 1);
        let content = converted[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn estimate_tokens_rough() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("hello world how are you"), 6); // 22 chars → (22+3)/4 = 6
    }

    #[test]
    fn is_reasoning_model_detection() {
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4-mini"));
        assert!(is_reasoning_model("gpt-5"));
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("claude-sonnet-4-6"));
    }
}

