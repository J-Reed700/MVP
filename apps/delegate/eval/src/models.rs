use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CompleteOptions {
    pub system: String,
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum ModelClient {
    Anthropic { api_key: String },
    OpenAI { api_key: String },
}

impl ModelClient {
    pub fn new_anthropic() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow!("ANTHROPIC_API_KEY not set"))?;
        Ok(Self::Anthropic { api_key })
    }

    pub fn new_openai() -> Result<Self> {
        let api_key =
            std::env::var("OPENAI_API_KEY").map_err(|_| anyhow!("OPENAI_API_KEY not set"))?;
        Ok(Self::OpenAI { api_key })
    }

    pub fn new(provider: &str) -> Result<Self> {
        match provider {
            "anthropic" => Self::new_anthropic(),
            "openai" => Self::new_openai(),
            other => Err(anyhow!("Unknown provider: {other}")),
        }
    }

    pub async fn complete(&self, opts: CompleteOptions) -> Result<ModelResponse> {
        match self {
            Self::Anthropic { api_key } => complete_anthropic(api_key, opts).await,
            Self::OpenAI { api_key } => complete_openai(api_key, opts).await,
        }
    }
}

async fn complete_anthropic(api_key: &str, opts: CompleteOptions) -> Result<ModelResponse> {
    let model = opts.model.as_deref().unwrap_or("claude-sonnet-4-6");
    let max_tokens = opts.max_tokens.unwrap_or(4096);
    let temperature = opts.temperature.unwrap_or(0.7);

    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "system": opts.system,
        "messages": [{"role": "user", "content": opts.prompt}]
    });

    let client = reqwest::Client::new();
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
    let resp_body: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("Anthropic API error ({}): {}", status, err_msg));
    }

    let content = resp_body["content"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|block| block["type"].as_str() == Some("text"))
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let input_tokens = resp_body["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["output_tokens"].as_u64().unwrap_or(0);

    Ok(ModelResponse {
        content,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Returns true for OpenAI reasoning models (o-series, gpt-5-*) that use
/// internal reasoning tokens and don't support the system role or temperature.
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

    // Reasoning models burn most of max_completion_tokens on internal reasoning,
    // so we multiply the requested budget to leave room for visible output.
    let base_tokens = opts.max_tokens.unwrap_or(4096);
    let max_tokens = if reasoning { base_tokens * 4 } else { base_tokens };

    // Reasoning models don't support the "system" role — use "developer" instead.
    let system_role = if reasoning { "developer" } else { "system" };

    let mut body = serde_json::json!({
        "model": model,
        "max_completion_tokens": max_tokens,
        "messages": [
            {"role": system_role, "content": opts.system},
            {"role": "user", "content": opts.prompt}
        ]
    });

    // Reasoning models don't support temperature — only include it for non-reasoning models.
    if !reasoning {
        if let Some(temp) = opts.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
    }

    let client = reqwest::Client::new();
    let start = Instant::now();

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let err_msg = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("Unknown API error");
        return Err(anyhow!("OpenAI API error ({}): {}", status, err_msg));
    }

    let content = resp_body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let input_tokens = resp_body["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let output_tokens = resp_body["usage"]["completion_tokens"]
        .as_u64()
        .unwrap_or(0);

    Ok(ModelResponse {
        content,
        model: model.to_string(),
        input_tokens,
        output_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Rough token count estimate. Uses ~4 chars per token heuristic.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}
