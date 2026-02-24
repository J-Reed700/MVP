use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, RequestBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    application::ports::InsightAnalytics,
    domain::models::{Decision, DecisionStatus, Insight},
};

#[derive(Clone, Copy)]
enum LlmProvider {
    OpenAi,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRuntimeSettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_seconds: u64,
    pub basic_auth_user: Option<String>,
    pub basic_auth_pass: Option<String>,
    pub user_header_name: Option<String>,
    pub user_header_value: Option<String>,
    pub pass_header_name: Option<String>,
    pub pass_header_value: Option<String>,
}

impl Default for LlmRuntimeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            timeout_seconds: 120,
            basic_auth_user: None,
            basic_auth_pass: None,
            user_header_name: None,
            user_header_value: None,
            pass_header_name: None,
            pass_header_value: None,
        }
    }
}

#[derive(Clone)]
struct BasicAuthConfig {
    user: String,
    pass: String,
}

#[derive(Clone)]
pub struct InsightLlmClient {
    provider: LlmProvider,
    api_key: Option<String>,
    settings: LlmRuntimeSettings,
    basic_auth: Option<BasicAuthConfig>,
    auth_headers: Vec<(HeaderName, HeaderValue)>,
    http: Client,
}

impl InsightLlmClient {
    pub fn from_env() -> Result<Option<Self>> {
        let provider = detect_provider()?;
        let Some(provider) = provider else {
            return Ok(None);
        };

        let settings = LlmRuntimeSettings {
            enabled: true,
            provider: provider_label(provider).to_string(),
            model: read_non_empty_env(&["SIGNALOPS_LLM_MODEL"]).unwrap_or_else(|| match provider {
                LlmProvider::OpenAi => "gpt-4.1-mini".to_string(),
                LlmProvider::Ollama => "llama3.1:8b".to_string(),
            }),
            base_url: match provider {
                LlmProvider::OpenAi => {
                    read_non_empty_env(&["OPENAI_BASE_URL", "SIGNALOPS_LLM_BASE_URL"])
                        .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
                }
                LlmProvider::Ollama => {
                    read_non_empty_env(&["OLLAMA_BASE_URL", "SIGNALOPS_LLM_BASE_URL"])
                        .unwrap_or_else(|| "http://localhost:11434".to_string())
                }
            },
            api_key: read_non_empty_env(&["OPENAI_API_KEY"]),
            timeout_seconds: read_non_empty_env(&["SIGNALOPS_LLM_TIMEOUT_SECONDS"])
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_else(|| default_timeout_for_provider(provider)),
            basic_auth_user: read_non_empty_env(&[
                "SIGNALOPS_LLM_BASIC_AUTH_USER",
                "OLLAMA_BASIC_AUTH_USER",
                "SIGNALOPS_LLM_AUTH_USER",
            ]),
            basic_auth_pass: read_non_empty_env(&[
                "SIGNALOPS_LLM_BASIC_AUTH_PASS",
                "OLLAMA_BASIC_AUTH_PASS",
                "SIGNALOPS_LLM_AUTH_PASS",
            ]),
            user_header_name: read_non_empty_env(&[
                "SIGNALOPS_LLM_USER_HEADER_NAME",
                "OLLAMA_AUTH_USER_HEADER",
            ]),
            user_header_value: read_non_empty_env(&[
                "SIGNALOPS_LLM_USER_HEADER_VALUE",
                "OLLAMA_AUTH_USER_VALUE",
            ]),
            pass_header_name: read_non_empty_env(&[
                "SIGNALOPS_LLM_PASS_HEADER_NAME",
                "OLLAMA_AUTH_PASS_HEADER",
            ]),
            pass_header_value: read_non_empty_env(&[
                "SIGNALOPS_LLM_PASS_HEADER_VALUE",
                "OLLAMA_AUTH_PASS_VALUE",
            ]),
        };

        Self::from_settings(settings)
    }

    pub fn from_settings(settings: LlmRuntimeSettings) -> Result<Option<Self>> {
        if !settings.enabled {
            return Ok(None);
        }

        let provider = parse_provider(&settings.provider)?;
        let requested_timeout_seconds = settings.timeout_seconds;
        let timeout_seconds = if requested_timeout_seconds < 60 {
            120
        } else {
            requested_timeout_seconds.clamp(60, 300)
        };
        let base_url = settings.base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            bail!("base_url is required when llm settings are enabled");
        }
        if provider_is_ollama_remote_http(provider, &base_url) {
            bail!(
                "remote ollama base_url must use https (received '{base_url}'); use https://... or localhost http"
            );
        }

        let model = settings.model.trim().to_string();
        if model.is_empty() {
            bail!("model is required when llm settings are enabled");
        }

        let api_key = match provider {
            LlmProvider::OpenAi => Some(
                settings
                    .api_key
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .context("api_key is required for openai provider")?,
            ),
            LlmProvider::Ollama => settings
                .api_key
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };

        let basic_auth = build_basic_auth_from_settings(&settings)?;
        let auth_headers = build_auth_headers_from_settings(&settings, basic_auth.as_ref())?;

        let http = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("failed to build llm http client")?;

        Ok(Some(Self {
            provider,
            api_key,
            settings: LlmRuntimeSettings {
                enabled: true,
                provider: provider_label(provider).to_string(),
                model,
                base_url,
                timeout_seconds,
                ..settings
            },
            basic_auth,
            auth_headers,
            http,
        }))
    }

    pub fn settings(&self) -> LlmRuntimeSettings {
        self.settings.clone()
    }

    fn enrichment_system_prompt(&self) -> &'static str {
        "You are a customer success intelligence analyst. Improve and calibrate operational insights while staying grounded in provided signal evidence. Never invent data. Keep recommendations specific and execution-ready for PM/CSM leadership."
    }

    fn enrichment_user_prompt(&self, context_json: &str) -> String {
        format!(
            "{}\n\n{}\n{}",
            "Return strict JSON with shape: {\"updates\":[{...}]}.",
            "For each insight category, return one update with concise title, recommendation, rationale, evidence, confidence_explanation, and up to 3 concrete playbook steps.",
            context_json
        )
    }

    async fn enrich_batch(
        &self,
        signal_count: usize,
        batch: Vec<InsightContext>,
    ) -> Result<Vec<InsightUpdate>> {
        let context = InsightContextRequest {
            insight_count: batch.len(),
            signal_count,
            insights: batch,
        };
        let context_json =
            serde_json::to_string(&context).context("failed to serialize llm context")?;
        let system_prompt = self.enrichment_system_prompt();
        let user_prompt = self.enrichment_user_prompt(&context_json);

        let content = match self.provider {
            LlmProvider::OpenAi => self.call_openai(system_prompt, user_prompt).await?,
            LlmProvider::Ollama => self.call_ollama(system_prompt, user_prompt).await?,
        };

        let enrichment = parse_enrichment_response(&content)?;
        Ok(enrichment.updates)
    }

    async fn call_openai(&self, system_prompt: &str, user_prompt: String) -> Result<String> {
        let request = OpenAiChatCompletionRequest {
            model: self.settings.model.clone(),
            response_format: OpenAiResponseFormat {
                kind: "json_object".to_string(),
            },
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
        };

        let api_key = self
            .api_key
            .as_ref()
            .context("missing OPENAI_API_KEY for openai provider")?;
        let url = format!("{}/chat/completions", self.settings.base_url);
        let response = self
            .apply_auth(self.http.post(url))
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await
            .context("openai request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "openai request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let parsed: OpenAiChatCompletionResponse = response
            .json()
            .await
            .context("failed to parse openai completion response")?;
        parsed
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .context("openai response did not include message content")
    }

    async fn call_ollama_native(
        &self,
        system_prompt: &str,
        user_prompt: String,
        base_url: &str,
    ) -> Result<String> {
        let request = OllamaChatRequest {
            model: self.settings.model.clone(),
            stream: false,
            format: Some("json".to_string()),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
        };

        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
        let response = self
            .apply_auth(self.http.post(url))
            .json(&request)
            .send()
            .await
            .context("ollama request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "ollama request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let parsed: OllamaChatResponse = response
            .json()
            .await
            .context("failed to parse ollama chat response")?;

        if let Some(error) = parsed.error.filter(|value| !value.trim().is_empty()) {
            bail!("ollama error: {error}");
        }

        parsed
            .message
            .as_ref()
            .map(|message| message.content.trim().to_string())
            .filter(|value| !value.is_empty())
            .context("ollama response did not include message content")
    }

    async fn call_ollama_openai_compat(
        &self,
        system_prompt: &str,
        user_prompt: String,
        base_url: &str,
    ) -> Result<String> {
        let request = OpenAiCompatChatCompletionRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
        };

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let mut request_builder = self.apply_auth(self.http.post(url));
        if let Some(api_key) = self.api_key.as_ref() {
            request_builder = request_builder.bearer_auth(api_key);
        }
        let response = request_builder
            .json(&request)
            .send()
            .await
            .context("ollama openai-compatible request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "ollama openai-compatible request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let parsed: OpenAiChatCompletionResponse = response
            .json()
            .await
            .context("failed to parse ollama openai-compatible chat response")?;

        parsed
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .context("ollama openai-compatible response did not include message content")
    }

    async fn call_ollama(&self, system_prompt: &str, user_prompt: String) -> Result<String> {
        let base_url = self.settings.base_url.as_str();
        if uses_v1_base_path(base_url) {
            let compat_result = self
                .call_ollama_openai_compat(system_prompt, user_prompt.clone(), base_url)
                .await;
            match compat_result {
                Ok(value) => Ok(value),
                Err(compat_error) => {
                    if is_timeout_error(&compat_error) {
                        bail!(
                            "ollama openai-compatible request timed out after {}s [{}]",
                            self.settings.timeout_seconds,
                            self.auth_debug_summary()
                        );
                    }
                    let native_base = strip_v1_base_path(base_url);
                    match self
                        .call_ollama_native(system_prompt, user_prompt, &native_base)
                        .await
                    {
                        Ok(value) => Ok(value),
                        Err(native_error) => bail!(
                            "ollama request failed via /v1/chat/completions and /api/chat ({compat_error}; {native_error}) [{}]"
                            ,
                            self.auth_debug_summary()
                        ),
                    }
                }
            }
        } else {
            let compat_base = ensure_v1_base_path(base_url);
            if prefer_openai_compat_for_ollama(base_url) {
                let compat_result = self
                    .call_ollama_openai_compat(system_prompt, user_prompt.clone(), &compat_base)
                    .await;
                match compat_result {
                    Ok(value) => Ok(value),
                    Err(compat_error) => {
                        if is_timeout_error(&compat_error) {
                            bail!(
                                "ollama openai-compatible request timed out after {}s [{}]",
                                self.settings.timeout_seconds,
                                self.auth_debug_summary()
                            );
                        }
                        match self
                            .call_ollama_native(system_prompt, user_prompt, base_url)
                            .await
                        {
                            Ok(value) => Ok(value),
                            Err(native_error) => bail!(
                                "ollama request failed via /v1/chat/completions and /api/chat ({compat_error}; {native_error}) [{}]",
                                self.auth_debug_summary()
                            ),
                        }
                    }
                }
            } else {
                let native_result = self
                    .call_ollama_native(system_prompt, user_prompt.clone(), base_url)
                    .await;
                match native_result {
                    Ok(value) => Ok(value),
                    Err(native_error) => {
                        if is_timeout_error(&native_error) {
                            bail!(
                                "ollama native request timed out after {}s [{}]",
                                self.settings.timeout_seconds,
                                self.auth_debug_summary()
                            );
                        }
                        match self
                            .call_ollama_openai_compat(system_prompt, user_prompt, &compat_base)
                            .await
                        {
                            Ok(value) => Ok(value),
                            Err(compat_error) => bail!(
                                "ollama request failed via /api/chat and /v1/chat/completions ({native_error}; {compat_error}) [{}]",
                                self.auth_debug_summary()
                            ),
                        }
                    }
                }
            }
        }
    }

    async fn list_models_openai_compat(
        &self,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let mut request_builder = self.apply_auth(self.http.get(url));
        if let Some(key) = api_key {
            request_builder = request_builder.bearer_auth(key);
        }
        let response = request_builder
            .send()
            .await
            .context("openai-compatible models request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "openai-compatible models request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let parsed: OpenAiModelsResponse = response
            .json()
            .await
            .context("failed to parse openai-compatible models response")?;
        let mut models = parsed
            .data
            .into_iter()
            .map(|item| item.id)
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        Ok(models)
    }

    async fn list_models_ollama_native(&self, base_url: &str) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
        let response = self
            .apply_auth(self.http.get(url))
            .send()
            .await
            .context("ollama tags request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!(
                "ollama tags request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let parsed: OllamaTagsResponse = response
            .json()
            .await
            .context("failed to parse ollama tags response")?;
        let mut models = parsed
            .models
            .into_iter()
            .map(|item| {
                if !item.name.trim().is_empty() {
                    item.name
                } else {
                    item.model
                }
            })
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        Ok(models)
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        match self.provider {
            LlmProvider::OpenAi => {
                let api_key = self
                    .api_key
                    .as_ref()
                    .context("missing OPENAI_API_KEY for openai provider")?;
                self.list_models_openai_compat(&self.settings.base_url, Some(api_key))
                    .await
                    .context("openai models request failed")
            }
            LlmProvider::Ollama => {
                let base_url = self.settings.base_url.as_str();
                if uses_v1_base_path(base_url) {
                    let compat_result = self
                        .list_models_openai_compat(base_url, self.api_key.as_deref())
                        .await;
                    match compat_result {
                        Ok(models) => Ok(models),
                        Err(compat_error) => {
                            let native_base = strip_v1_base_path(base_url);
                            match self.list_models_ollama_native(&native_base).await {
                                Ok(models) => Ok(models),
                                Err(native_error) => bail!(
                                    "ollama model discovery failed via /v1/models and /api/tags ({compat_error}; {native_error}) [{}]"
                                    ,
                                    self.auth_debug_summary()
                                ),
                            }
                        }
                    }
                } else {
                    let native_result = self.list_models_ollama_native(base_url).await;
                    match native_result {
                        Ok(models) => Ok(models),
                        Err(native_error) => {
                            let compat_base = ensure_v1_base_path(base_url);
                            match self
                                .list_models_openai_compat(&compat_base, self.api_key.as_deref())
                                .await
                            {
                                Ok(models) => Ok(models),
                                Err(compat_error) => bail!(
                                    "ollama model discovery failed via /api/tags and /v1/models ({native_error}; {compat_error}) [{}]"
                                    ,
                                    self.auth_debug_summary()
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn warmup(&self) -> Result<String> {
        let system_prompt =
            "You are warming up for customer success analytics. Reply with READY only.";
        let user_prompt = "READY".to_string();
        let raw = match self.provider {
            LlmProvider::OpenAi => self.call_openai(system_prompt, user_prompt).await?,
            LlmProvider::Ollama => self.call_ollama(system_prompt, user_prompt).await?,
        };
        Ok(trim_text(raw.trim(), 120))
    }

    fn apply_auth(&self, mut request: RequestBuilder) -> RequestBuilder {
        if let Some(auth) = &self.basic_auth {
            request = request.basic_auth(auth.user.clone(), Some(auth.pass.clone()));
        }

        for (name, value) in &self.auth_headers {
            request = request.header(name, value.clone());
        }

        request
    }

    fn auth_debug_summary(&self) -> String {
        let mut header_names = self
            .auth_headers
            .iter()
            .map(|(name, _)| name.as_str().to_string())
            .collect::<Vec<_>>();
        header_names.sort();
        let header_summary = if header_names.is_empty() {
            "none".to_string()
        } else {
            header_names.join(",")
        };

        format!(
            "provider={} base_url={} basic_auth={} api_key={} headers={}",
            self.settings.provider,
            self.settings.base_url,
            self.basic_auth.is_some(),
            self.api_key.is_some(),
            header_summary
        )
    }
}

#[async_trait]
impl InsightAnalytics for InsightLlmClient {
    async fn enrich_insights(
        &self,
        decisions: &[Decision],
        insights: &[Insight],
    ) -> Result<Vec<Insight>> {
        if insights.is_empty() {
            return Ok(Vec::new());
        }

        let signal_index = decisions
            .iter()
            .map(|decision| (decision.id, decision))
            .collect::<HashMap<_, _>>();

        let mut contexts = insights
            .iter()
            .map(|insight| {
                let related_signals = insight
                    .related_signal_ids
                    .iter()
                    .take(4)
                    .filter_map(|id| signal_index.get(id))
                    .map(|decision| SignalEvidence {
                        id: decision.id.to_string(),
                        title: trim_text(&decision.title, 100),
                        summary: trim_text(&decision.summary, 120),
                        owner: decision.owner.clone(),
                        status: status_label(&decision.status).to_string(),
                        sources: decision.source_systems.iter().take(3).cloned().collect(),
                        tags: decision.tags.iter().take(4).cloned().collect(),
                        updated_at: decision.updated_at.to_rfc3339(),
                    })
                    .collect::<Vec<_>>();

                InsightContext {
                    category: insight.category.clone(),
                    audience: insight.audience.clone(),
                    priority: insight.priority.clone(),
                    title: insight.title.clone(),
                    recommendation: insight.recommendation.clone(),
                    metric: insight.metric.clone(),
                    current_confidence: insight.confidence,
                    current_owner_role: insight.owner_role.clone(),
                    current_due_in_days: insight.due_in_days,
                    related_signals,
                }
            })
            .collect::<Vec<_>>();

        let batch_size = enrichment_batch_size();
        let max_parallel = enrichment_parallelism();
        let signal_count = decisions.len();
        let mut updates = Vec::new();
        let mut failures = Vec::new();
        let mut join_set = tokio::task::JoinSet::new();

        while !contexts.is_empty() {
            let take = contexts.len().min(batch_size);
            let batch = contexts.drain(0..take).collect::<Vec<_>>();
            let client = self.clone();
            join_set.spawn(async move { client.enrich_batch(signal_count, batch).await });

            if join_set.len() >= max_parallel {
                match join_set.join_next().await {
                    Some(Ok(Ok(batch_updates))) => updates.extend(batch_updates),
                    Some(Ok(Err(error))) => failures.push(error),
                    Some(Err(join_error)) => {
                        failures.push(anyhow!("llm enrichment task failed to join: {join_error}"))
                    }
                    None => break,
                }
            }
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(batch_updates)) => updates.extend(batch_updates),
                Ok(Err(error)) => failures.push(error),
                Err(join_error) => {
                    failures.push(anyhow!("llm enrichment task failed to join: {join_error}"))
                }
            }
        }

        if updates.is_empty() {
            if let Some(error) = failures.into_iter().next() {
                return Err(error);
            }
            bail!("llm enrichment produced no updates");
        }

        if !failures.is_empty() {
            tracing::warn!(
                failed_batches = failures.len(),
                successful_updates = updates.len(),
                "some llm enrichment batches failed; applying partial enrichment"
            );
        }

        let (merged, applied_updates) = merge_enrichment(insights, updates);
        if applied_updates == 0 {
            tracing::warn!(
                "llm enrichment returned updates but none could be mapped to known insight categories"
            );
        }
        Ok(merged)
    }
}

#[derive(Serialize)]
struct OpenAiChatCompletionRequest {
    model: String,
    response_format: OpenAiResponseFormat,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct OpenAiCompatChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct OpenAiResponseFormat {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    stream: bool,
    format: Option<String>,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChoiceMessage,
}

#[derive(Deserialize)]
struct OpenAiChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: Option<ChatMessage>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelItem>,
}

#[derive(Deserialize)]
struct OpenAiModelItem {
    id: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaTagModel>,
}

#[derive(Deserialize)]
struct OllamaTagModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Serialize)]
struct InsightContextRequest {
    insight_count: usize,
    signal_count: usize,
    insights: Vec<InsightContext>,
}

#[derive(Debug, Serialize)]
struct InsightContext {
    category: String,
    audience: String,
    priority: String,
    title: String,
    recommendation: String,
    metric: Option<String>,
    current_confidence: f32,
    current_owner_role: String,
    current_due_in_days: u16,
    related_signals: Vec<SignalEvidence>,
}

#[derive(Debug, Serialize)]
struct SignalEvidence {
    id: String,
    title: String,
    summary: String,
    owner: Option<String>,
    status: String,
    sources: Vec<String>,
    tags: Vec<String>,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct InsightEnrichmentResponse {
    #[serde(default)]
    updates: Vec<InsightUpdate>,
}

#[derive(Debug, Deserialize)]
struct InsightUpdate {
    category: String,
    priority: Option<String>,
    title: Option<String>,
    recommendation: Option<String>,
    owner_role: Option<String>,
    due_in_days: Option<u16>,
    confidence: Option<f32>,
    confidence_explanation: Option<String>,
    rationale: Option<String>,
    evidence: Option<Vec<String>>,
    playbook_steps: Option<Vec<String>>,
}

fn detect_provider() -> Result<Option<LlmProvider>> {
    if let Some(raw) = read_non_empty_env(&["SIGNALOPS_LLM_PROVIDER"]) {
        return match raw.to_ascii_lowercase().as_str() {
            "openai" => Ok(Some(LlmProvider::OpenAi)),
            "ollama" => Ok(Some(LlmProvider::Ollama)),
            other => {
                bail!("unsupported SIGNALOPS_LLM_PROVIDER '{other}' (expected openai or ollama)")
            }
        };
    }

    if read_non_empty_env(&["OPENAI_API_KEY"]).is_some() {
        return Ok(Some(LlmProvider::OpenAi));
    }

    if read_non_empty_env(&["OLLAMA_BASE_URL"]).is_some() {
        return Ok(Some(LlmProvider::Ollama));
    }

    Ok(None)
}

fn parse_provider(raw: &str) -> Result<LlmProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(LlmProvider::OpenAi),
        "ollama" => Ok(LlmProvider::Ollama),
        other => bail!("unsupported provider '{other}' (expected openai or ollama)"),
    }
}

fn provider_label(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenAi => "openai",
        LlmProvider::Ollama => "ollama",
    }
}

fn default_timeout_for_provider(provider: LlmProvider) -> u64 {
    let _ = provider;
    120
}

fn enrichment_parallelism() -> usize {
    read_non_empty_env(&["SIGNALOPS_LLM_ENRICH_PARALLELISM"])
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 6))
        .unwrap_or(3)
}

fn enrichment_batch_size() -> usize {
    read_non_empty_env(&["SIGNALOPS_LLM_ENRICH_BATCH_SIZE"])
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 12))
        .unwrap_or(4)
}

fn read_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn build_basic_auth_from_settings(
    settings: &LlmRuntimeSettings,
) -> Result<Option<BasicAuthConfig>> {
    let user = settings
        .basic_auth_user
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let pass = settings
        .basic_auth_pass
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match (user, pass) {
        (None, None) => Ok(None),
        (Some(user), Some(pass)) => Ok(Some(BasicAuthConfig { user, pass })),
        _ => bail!("both SIGNALOPS_LLM_BASIC_AUTH_USER and SIGNALOPS_LLM_BASIC_AUTH_PASS must be set together"),
    }
}

fn build_auth_headers_from_settings(
    settings: &LlmRuntimeSettings,
    basic_auth: Option<&BasicAuthConfig>,
) -> Result<Vec<(HeaderName, HeaderValue)>> {
    let mut headers = Vec::new();

    let user_header_name = settings
        .user_header_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(name) = user_header_name {
        let value = settings
            .user_header_value
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| basic_auth.map(|auth| auth.user.clone()))
            .context(
                "user header name was provided but no user header value or basic auth user is set",
            )?;
        headers.push(build_header_pair(&name, &value)?);
    }

    let pass_header_name = settings
        .pass_header_name
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(name) = pass_header_name {
        let value = settings
            .pass_header_value
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| basic_auth.map(|auth| auth.pass.clone()))
            .context(
                "pass header name was provided but no pass header value or basic auth pass is set",
            )?;
        headers.push(build_header_pair(&name, &value)?);
    }

    Ok(headers)
}

fn build_header_pair(name: &str, value: &str) -> Result<(HeaderName, HeaderValue)> {
    let header_name = HeaderName::from_bytes(name.as_bytes())
        .with_context(|| format!("invalid header name '{name}'"))?;
    let header_value = HeaderValue::from_str(value)
        .with_context(|| format!("invalid header value for '{name}'"))?;
    Ok((header_name, header_value))
}

fn uses_v1_base_path(base_url: &str) -> bool {
    base_url.trim_end_matches('/').ends_with("/v1")
}

fn strip_v1_base_path(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

fn ensure_v1_base_path(base_url: &str) -> String {
    if uses_v1_base_path(base_url) {
        base_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/v1", base_url.trim_end_matches('/'))
    }
}

fn provider_is_ollama_remote_http(provider: LlmProvider, base_url: &str) -> bool {
    if !matches!(provider, LlmProvider::Ollama) {
        return false;
    }
    let lowered = base_url.to_ascii_lowercase();
    if !lowered.starts_with("http://") {
        return false;
    }

    let host_port_path = lowered.trim_start_matches("http://");
    let host = host_port_path
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    !matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0")
}

fn prefer_openai_compat_for_ollama(base_url: &str) -> bool {
    !is_local_base_url(base_url)
}

fn is_local_base_url(base_url: &str) -> bool {
    let lowered = base_url.to_ascii_lowercase();
    let host_port_path = lowered
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = host_port_path
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0")
}

fn summarize_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    trim_text(&compact, 600)
}

fn is_timeout_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_timeout)
    })
}

fn parse_enrichment_response(raw_output: &str) -> Result<InsightEnrichmentResponse> {
    let cleaned = strip_code_fences(raw_output).trim();
    if cleaned.is_empty() {
        bail!("llm returned an empty enrichment response");
    }

    if let Ok(parsed) = serde_json::from_str::<InsightEnrichmentResponse>(cleaned) {
        return Ok(parsed);
    }

    if let Some(payload) = extract_json_payload(cleaned) {
        if let Ok(parsed) = serde_json::from_str::<InsightEnrichmentResponse>(payload) {
            return Ok(parsed);
        }

        if let Ok(value) = serde_json::from_str::<Value>(payload) {
            return normalize_enrichment_value(value);
        }
    }

    let value = serde_json::from_str::<Value>(cleaned)
        .context("failed to parse llm insight enrichment JSON")?;
    normalize_enrichment_value(value)
}

fn extract_json_payload(text: &str) -> Option<&str> {
    let object = text
        .find('{')
        .zip(text.rfind('}'))
        .filter(|(start, end)| end > start)
        .map(|(start, end)| &text[start..=end]);
    let array = text
        .find('[')
        .zip(text.rfind(']'))
        .filter(|(start, end)| end > start)
        .map(|(start, end)| &text[start..=end]);

    match (object, array) {
        (Some(obj), Some(arr)) => {
            if obj.len() >= arr.len() {
                Some(obj)
            } else {
                Some(arr)
            }
        }
        (Some(obj), None) => Some(obj),
        (None, Some(arr)) => Some(arr),
        (None, None) => None,
    }
}

fn normalize_enrichment_value(value: Value) -> Result<InsightEnrichmentResponse> {
    let updates_value = match value {
        Value::Object(mut root) => root.remove("updates").unwrap_or(Value::Object(root)),
        other => other,
    };

    let updates = updates_from_json_value(updates_value)?;
    if updates.is_empty() {
        bail!("llm enrichment JSON parsed but contained no usable updates");
    }

    Ok(InsightEnrichmentResponse { updates })
}

fn updates_from_json_value(value: Value) -> Result<Vec<InsightUpdate>> {
    match value {
        Value::Array(items) => {
            let mut updates = Vec::new();
            for item in items {
                match item {
                    Value::Object(map) => {
                        if let Some(update) = update_from_map(map.clone(), None) {
                            updates.push(update);
                            continue;
                        }
                        updates.extend(updates_from_object_map(map));
                    }
                    other => {
                        let text = json_value_as_string(other);
                        if let Some(category) = canonical_category_key(&text) {
                            updates.push(InsightUpdate {
                                category,
                                priority: None,
                                title: None,
                                recommendation: None,
                                owner_role: None,
                                due_in_days: None,
                                confidence: None,
                                confidence_explanation: None,
                                rationale: None,
                                evidence: None,
                                playbook_steps: None,
                            });
                        }
                    }
                }
            }
            Ok(updates)
        }
        Value::Object(map) => {
            if let Some(update) = update_from_map(map.clone(), None) {
                return Ok(vec![update]);
            }

            Ok(updates_from_object_map(map))
        }
        other => {
            let hint = json_value_as_string(other);
            bail!("unsupported llm enrichment payload shape: {hint}");
        }
    }
}

fn update_from_map(
    mut map: serde_json::Map<String, Value>,
    fallback_category: Option<String>,
) -> Option<InsightUpdate> {
    let category = map
        .remove("category")
        .map(json_value_as_string)
        .and_then(|value| {
            canonical_category_key(&value).or_else(|| {
                let normalized = value.trim().replace('-', "_").replace(' ', "_");
                if normalized.is_empty() {
                    None
                } else {
                    Some(normalized.to_ascii_lowercase())
                }
            })
        })
        .or(fallback_category)
        .or_else(|| {
            let combined = format!(
                "{} {} {}",
                map.get("title")
                    .map_or(String::new(), |v| json_value_as_string(v.clone())),
                map.get("recommendation")
                    .map_or(String::new(), |v| json_value_as_string(v.clone())),
                map.get("rationale")
                    .map_or(String::new(), |v| json_value_as_string(v.clone()))
            );
            canonical_category_key(&combined)
        })?;

    Some(InsightUpdate {
        category,
        priority: map.remove("priority").map(json_value_as_string),
        title: map.remove("title").map(json_value_as_string),
        recommendation: map.remove("recommendation").map(json_value_as_string),
        owner_role: map.remove("owner_role").map(json_value_as_string),
        due_in_days: map.remove("due_in_days").and_then(json_value_as_u16),
        confidence: map.remove("confidence").and_then(json_value_as_f32),
        confidence_explanation: map
            .remove("confidence_explanation")
            .map(json_value_as_string),
        rationale: map.remove("rationale").map(json_value_as_string),
        evidence: map.remove("evidence").and_then(json_value_as_string_vec),
        playbook_steps: map
            .remove("playbook_steps")
            .and_then(json_value_as_string_vec),
    })
}

fn updates_from_object_map(map: serde_json::Map<String, Value>) -> Vec<InsightUpdate> {
    let mut updates = Vec::new();
    for (key, nested) in map {
        let category = canonical_category_key(&key).or_else(|| {
            let normalized = key.trim().replace('-', "_").replace(' ', "_");
            if normalized.is_empty() {
                None
            } else {
                Some(normalized.to_ascii_lowercase())
            }
        });
        if let Some(category) = category {
            match nested {
                Value::Object(inner) => {
                    if let Some(update) = update_from_map(inner, Some(category)) {
                        updates.push(update);
                    }
                }
                Value::Array(_) | Value::String(_) => {
                    updates.push(InsightUpdate {
                        category,
                        priority: None,
                        title: None,
                        recommendation: None,
                        owner_role: None,
                        due_in_days: None,
                        confidence: None,
                        confidence_explanation: None,
                        rationale: None,
                        evidence: None,
                        playbook_steps: None,
                    });
                }
                _ => {}
            }
        }
    }
    updates
}

fn json_value_as_string(value: Value) -> String {
    match value {
        Value::String(text) => text,
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(items) => items
            .into_iter()
            .map(json_value_as_string)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(map) => serde_json::to_string(&map).unwrap_or_default(),
        Value::Null => String::new(),
    }
}

fn json_value_as_u16(value: Value) -> Option<u16> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|v| u16::try_from(v).ok()),
        Value::String(text) => text.trim().parse::<u16>().ok(),
        _ => None,
    }
}

fn json_value_as_f32(value: Value) -> Option<f32> {
    match value {
        Value::Number(number) => number.as_f64().map(|v| v as f32),
        Value::String(text) => text.trim().parse::<f32>().ok(),
        _ => None,
    }
}

fn json_value_as_string_vec(value: Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => {
            let values = items
                .into_iter()
                .map(json_value_as_string)
                .filter(|item| !item.trim().is_empty())
                .collect::<Vec<_>>();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        }
        Value::String(single) => {
            let trimmed = single.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(vec![trimmed.to_string()])
            }
        }
        _ => None,
    }
}

fn canonical_category_key(raw: &str) -> Option<String> {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    if normalized.is_empty() {
        return None;
    }

    let text = format!("_{}_", normalized);
    let key = match normalized.as_str() {
        "stale_signals" | "stale_signal" | "stale_records" => "stale_signals",
        "missing_owners" | "missing_owner" | "ownerless_signals" | "unowned_signals" => {
            "missing_owners"
        }
        "superseded_records" | "superseded_signals" => "superseded_records",
        "owner_concentration_risk" | "owner_concentration" => "owner_concentration_risk",
        "source_dependency_risk" | "source_concentration_risk" | "source_concentration" => {
            "source_dependency_risk"
        }
        "metadata_hygiene_gap" | "untagged_hygiene" => "metadata_hygiene_gap",
        "possible_duplicate_signals" | "duplicate_signals" => "possible_duplicate_signals",
        "source_owner_gap" | "owner_gap_by_source" => "source_owner_gap",
        "account_signal_hotspots" | "account_hotspots" => "account_signal_hotspots",
        "nps_follow_up_queue" | "nps_follow_up" => "nps_follow_up_queue",
        _ if text.contains("_missing_") && text.contains("_owner") => "missing_owners",
        _ if text.contains("_unowned_") && text.contains("_signal") => "missing_owners",
        _ if text.contains("_stale_") => "stale_signals",
        _ if text.contains("_supersed") => "superseded_records",
        _ if text.contains("_source_") && text.contains("_depend") => "source_dependency_risk",
        _ if text.contains("_source_") && text.contains("_owner_") && text.contains("_gap_") => {
            "source_owner_gap"
        }
        _ if text.contains("_owner_") && text.contains("_concentration") => {
            "owner_concentration_risk"
        }
        _ if text.contains("_duplicate_") => "possible_duplicate_signals",
        _ if text.contains("_tag_") && text.contains("_hygiene_") => "metadata_hygiene_gap",
        _ if text.contains("_account_") && text.contains("_hotspot_") => "account_signal_hotspots",
        _ if text.contains("_nps_") => "nps_follow_up_queue",
        _ => return None,
    };

    Some(key.to_string())
}

fn merge_enrichment(base: &[Insight], updates: Vec<InsightUpdate>) -> (Vec<Insight>, usize) {
    let mut by_category = HashMap::new();
    let mut unmatched = Vec::new();
    for update in updates {
        if let Some(category) = canonical_category_key(&update.category) {
            by_category.insert(category, update);
        } else {
            unmatched.push(update);
        }
    }

    let mut applied = 0usize;
    let mut merged = base.to_vec();
    for insight in &mut merged {
        let key =
            canonical_category_key(&insight.category).unwrap_or_else(|| insight.category.clone());
        if let Some(update) = by_category.remove(&key) {
            apply_update_to_insight(insight, update);
            applied += 1;
        }
    }

    if applied == 0 && !unmatched.is_empty() && unmatched.len() == merged.len() {
        for (insight, update) in merged.iter_mut().zip(unmatched.into_iter()) {
            apply_update_to_insight(insight, update);
            applied += 1;
        }
    }

    (merged, applied)
}

fn apply_update_to_insight(insight: &mut Insight, update: InsightUpdate) {
    if let Some(priority) = normalize_priority(update.priority) {
        insight.priority = priority;
    }
    if let Some(title) = normalize_short_text(update.title, 120) {
        insight.title = title;
    }
    if let Some(recommendation) = normalize_short_text(update.recommendation, 240) {
        insight.recommendation = recommendation;
    }
    if let Some(owner_role) = normalize_short_text(update.owner_role, 80) {
        insight.owner_role = owner_role;
    }
    if let Some(days) = update.due_in_days {
        insight.due_in_days = days.clamp(1, 30);
    }
    if let Some(confidence) = update.confidence {
        insight.confidence = confidence.clamp(0.35, 0.99);
    }
    insight.confidence_explanation = normalize_short_text(update.confidence_explanation, 180);
    insight.rationale = normalize_short_text(update.rationale, 240);
    insight.evidence = update
        .evidence
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| normalize_short_text(Some(item), 180))
        .take(3)
        .collect();
    if let Some(steps) = update.playbook_steps {
        let cleaned = steps
            .into_iter()
            .filter_map(|step| normalize_short_text(Some(step), 140))
            .take(3)
            .collect::<Vec<_>>();
        if !cleaned.is_empty() {
            insight.playbook_steps = cleaned;
        }
    }
    insight.generated_by = "llm+rules".to_string();
}

fn normalize_priority(priority: Option<String>) -> Option<String> {
    let value = priority?.trim().to_ascii_lowercase();
    match value.as_str() {
        "high" | "medium" | "low" => Some(value),
        _ => None,
    }
}

fn normalize_short_text(value: Option<String>, max_chars: usize) -> Option<String> {
    let text = value?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(trim_text(&text, max_chars))
}

fn trim_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

fn strip_code_fences(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
    {
        return inner.trim();
    }
    if let Some(inner) = trimmed
        .strip_prefix("```")
        .and_then(|value| value.strip_suffix("```"))
    {
        return inner.trim();
    }
    trimmed
}

fn status_label(status: &DecisionStatus) -> &'static str {
    match status {
        DecisionStatus::Proposed => "proposed",
        DecisionStatus::Approved => "approved",
        DecisionStatus::Superseded => "superseded",
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        canonical_category_key, ensure_v1_base_path, merge_enrichment, parse_enrichment_response,
        strip_code_fences, strip_v1_base_path, uses_v1_base_path, InsightEnrichmentResponse,
        InsightUpdate,
    };
    use crate::domain::models::Insight;

    #[test]
    fn merges_updates_and_marks_generated_by_llm() {
        let insight = Insight {
            category: "missing_owners".to_string(),
            audience: "manager".to_string(),
            priority: "high".to_string(),
            title: "100 signal(s) have no owner".to_string(),
            recommendation: "Assign owners.".to_string(),
            metric: Some("100% unowned".to_string()),
            related_signal_ids: vec![Uuid::new_v4()],
            owner_role: "CS Manager".to_string(),
            due_in_days: 2,
            confidence: 0.93,
            confidence_explanation: None,
            rationale: None,
            evidence: Vec::new(),
            generated_by: "rules".to_string(),
            playbook_steps: vec!["Step 1".to_string()],
        };

        let updates = InsightEnrichmentResponse {
            updates: vec![InsightUpdate {
                category: "missing_owners".to_string(),
                priority: Some("medium".to_string()),
                title: Some("Ownerless signals are blocking follow-through".to_string()),
                recommendation: Some(
                    "Assign each strategic account signal to an owner by segment.".to_string(),
                ),
                owner_role: Some("CS Operations".to_string()),
                due_in_days: Some(4),
                confidence: Some(0.78),
                confidence_explanation: Some("Coverage is complete and recent.".to_string()),
                rationale: Some(
                    "Most unowned signals sit in renewal-critical accounts.".to_string(),
                ),
                evidence: Some(vec![
                    "Northstar onboarding drop was unassigned in the last 48h.".to_string(),
                    "Orbit support escalation has no accountable owner.".to_string(),
                ]),
                playbook_steps: Some(vec![
                    "Auto-route by account segment.".to_string(),
                    "Escalate unassigned strategic accounts daily.".to_string(),
                ]),
            }],
        };

        let (merged, applied) = merge_enrichment(&[insight], updates.updates);
        let first = merged.first().expect("expected one insight");

        assert_eq!(applied, 1);
        assert_eq!(first.priority, "medium");
        assert_eq!(first.owner_role, "CS Operations");
        assert_eq!(first.generated_by, "llm+rules");
        assert_eq!(first.playbook_steps.len(), 2);
        assert_eq!(first.evidence.len(), 2);
        assert_eq!(
            first.confidence_explanation.as_deref(),
            Some("Coverage is complete and recent.")
        );
    }

    #[test]
    fn strips_markdown_fences_from_json() {
        let input = "```json\n{\"updates\":[]}\n```";
        assert_eq!(strip_code_fences(input), "{\"updates\":[]}");
    }

    #[test]
    fn normalizes_v1_base_url_helpers() {
        assert!(uses_v1_base_path("https://example.com/v1"));
        assert!(uses_v1_base_path("https://example.com/v1/"));
        assert_eq!(
            strip_v1_base_path("https://example.com/v1"),
            "https://example.com"
        );
        assert_eq!(
            strip_v1_base_path("https://example.com"),
            "https://example.com"
        );
        assert_eq!(
            ensure_v1_base_path("https://example.com"),
            "https://example.com/v1"
        );
        assert_eq!(
            ensure_v1_base_path("https://example.com/v1"),
            "https://example.com/v1"
        );
    }

    #[test]
    fn parses_wrapped_and_nonstandard_enrichment_payload() {
        let raw = r#"Here you go:
        {
          "updates": [
            {
              "missing_owners": {
                "title": "Ownerless signals need assignment",
                "recommendation": "Assign owners this week"
              }
            }
          ]
        }"#;

        let parsed = parse_enrichment_response(raw).expect("expected enrichment payload");
        assert_eq!(parsed.updates.len(), 1);
        assert_eq!(parsed.updates[0].category, "missing_owners");
        assert_eq!(
            parsed.updates[0].title.as_deref(),
            Some("Ownerless signals need assignment")
        );
    }

    #[test]
    fn canonicalizes_category_aliases() {
        assert_eq!(
            canonical_category_key("Ownerless Signals"),
            Some("missing_owners".to_string())
        );
        assert_eq!(
            canonical_category_key("source concentration"),
            Some("source_dependency_risk".to_string())
        );
        assert_eq!(
            canonical_category_key("NPS follow up"),
            Some("nps_follow_up_queue".to_string())
        );
    }
}
