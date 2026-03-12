//! Scenario runner — sets up temp workspace, calls LLM, runs tool loop.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::budget::TokenBudget;
use crate::context::{self, TaskType};
use crate::dynamic_registry::{self, DynamicRegistry};
use crate::event::DelegateEvent;
use crate::messenger::{ChannelId, MessageTs, UserId};
use crate::models::{ChatOptions, CompleteOptions, ModelClient};
use crate::oauth::{CredentialStore, OAuthProviderConfig};
use crate::registry::ToolScope;
use crate::tools::{self, ToolContext};
use crate::workspace::Workspace;

use super::mock::MockMessenger;

/// Dummy provider configs so `configured_providers()` returns all 4 providers.
/// This lets the 3-tier credential filter properly skip skills whose provider
/// isn't connected (rather than falling through to env-var mode).
fn eval_provider_configs() -> HashMap<String, OAuthProviderConfig> {
    ["atlassian", "linear", "notion", "google"]
        .iter()
        .map(|name| {
            (
                name.to_string(),
                OAuthProviderConfig {
                    name: name.to_string(),
                    client_id: "eval-dummy".to_string(),
                    client_secret: "eval-dummy".to_string(),
                    auth_url: String::new(),
                    token_url: String::new(),
                    scopes: Vec::new(),
                    extra_auth_params: HashMap::new(),
                },
            )
        })
        .collect()
}

// ── Types ────────────────────────────────────────────────────────────────

pub(crate) struct Scenario {
    pub name: &'static str,
    pub workspace_files: &'static [(&'static str, &'static str)],
    pub trigger: &'static str,
    pub correct_answer: &'static str,
    pub expected_tools: &'static [&'static str],
}

pub(crate) struct EvalResult {
    pub response: String,
    /// Text sent via reply/post tools (the model often answers via reply, not content)
    pub reply_text: String,
    pub tools_called: Vec<String>,
    pub tokens_used: u64,
    pub duration_ms: u64,
}

// ── Runner ───────────────────────────────────────────────────────────────

pub(crate) async fn run_scenario(scenario: &Scenario) -> Result<EvalResult> {
    let start = std::time::Instant::now();
    let tmp = tempfile::TempDir::new()?;
    let ws_path = tmp.path();

    // Write workspace files
    for (path, content) in scenario.workspace_files {
        let full = ws_path.join(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content).await?;
    }

    let ws = Workspace::new(ws_path.to_str().unwrap());
    let messenger = MockMessenger::new();

    let event = DelegateEvent {
        id: "eval-1".to_string(),
        event_type: "message".to_string(),
        channel: ChannelId::from("C_EVAL"),
        user: UserId::from("U_EVAL"),
        content: scenario.trigger.to_string(),
        timestamp: MessageTs::from("1000000000.000000"),
        thread_ts: None,
        raw: serde_json::json!({}),
    };

    // Initialize credential store with dummy provider configs
    let cred_store = Arc::new(CredentialStore::new(ws_path, eval_provider_configs()));
    cred_store.load_all().await;
    let connected = cred_store.connected_providers().await;
    let configured = cred_store.configured_providers();

    // Initialize dynamic registry with credential-aware filtering
    let registry = DynamicRegistry::new();
    registry.set_credential_store(cred_store.clone()).await;
    registry.refresh_with_filter(&ws_path.join("skills"), Some(&connected)).await;

    // Compile context
    let mut compiled = context::compile(
        &event,
        ws.path(),
        TaskType::Respond,
        "",     // no recent logs
        8000,   // token budget
        Some("eval-channel"),
        false,
        ToolScope::Event,
    )
    .await?;

    // Override skills to match credential-filtered registry
    compiled.skills = context::load_skills_filtered(
        &ws_path.join("skills"),
        Some(&connected),
        Some(&configured),
    )
    .await;

    let (system, prompt) = context::to_prompt(&compiled, ToolScope::Event);
    // Use dynamic registry for tools — includes skill-defined tools
    let tools = registry.tool_schemas(ToolScope::Event).await;

    // Build model client from env
    // Load .env if present (tests don't go through main.rs which calls dotenvy)
    let _ = dotenvy::dotenv();
    let provider = std::env::var("DELEGATE_PROVIDER").unwrap_or_else(|_| "zhipu".to_string());
    let client = ModelClient::new(&provider)?;

    // Initial LLM call
    let response = client
        .complete(CompleteOptions {
            system: system.clone(),
            prompt: prompt.clone(),
            model: std::env::var("DELEGATE_MODEL").ok(),
            max_tokens: Some(2048),
            temperature: None,
            tools: Some(tools.clone()),
        })
        .await?;

    let budget = TokenBudget::new(100_000);
    budget.record(response.input_tokens + response.output_tokens).await;

    // Run tool loop manually to track which tools are called
    let ctx = ToolContext {
        messenger: &messenger,
        ws: &ws,
        event: &event,
        thread_ts: "1000000000.000000",
    };

    let mut conversation: Vec<Value> = vec![serde_json::json!({"role": "user", "content": prompt})];
    let mut resp = response;
    let mut total_tokens: u64 = resp.input_tokens + resp.output_tokens;
    let mut tools_called: Vec<String> = Vec::new();
    let mut reply_texts: Vec<String> = Vec::new();
    let mut turn = 0;
    const MAX_TURNS: usize = 5;
    // Track whether the previous turn had an info tool — if so, allow one
    // follow-up turn for the model to respond with action tools (reply, react).
    let mut prev_had_info = false;

    while !resp.tool_calls.is_empty() && turn < MAX_TURNS {
        conversation.push(resp.raw_assistant_message.clone());

        let mut has_info_tool = false;
        for call in &resp.tool_calls {
            tools_called.push(call.name.clone());
            // Capture text from reply tool calls — the model often answers via reply
            if matches!(call.name.as_str(), "reply" | "post" | "dm_user" | "group_dm") {
                if let Some(text) = call.arguments["text"].as_str() {
                    reply_texts.push(text.to_string());
                }
            }
            if registry.is_information_tool(&call.name).await {
                has_info_tool = true;
            }
            // Dispatch: skill-defined tools first, then static tools
            let result = if let Some(skill_tool) = registry.get_skill_tool(&call.name).await {
                dynamic_registry::execute_skill_tool(&skill_tool, &call.arguments, ws.path(), Some(&cred_store)).await
            } else {
                tools::execute_tool(call, &ctx).await
            };
            conversation.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result
            }));
        }

        // Continue if this turn had an info tool (needs follow-up reasoning)
        // OR if the previous turn had an info tool (model is responding to results).
        // Break only when neither this nor previous turn had info tools.
        if !has_info_tool && !prev_had_info {
            break;
        }
        prev_had_info = has_info_tool;

        if !budget.is_available().await {
            break;
        }

        resp = client
            .chat(ChatOptions {
                system: system.clone(),
                messages: conversation.clone(),
                model: std::env::var("DELEGATE_MODEL").ok(),
                max_tokens: Some(2048),
                temperature: None,
                tools: Some(tools.clone()),
            })
            .await?;

        total_tokens += resp.input_tokens + resp.output_tokens;
        budget.record(resp.input_tokens + resp.output_tokens).await;
        turn += 1;
    }

    // Collect any remaining tool calls from the final turn (only if we
    // exited the loop without processing them — i.e. MAX_TURNS hit).
    if turn >= MAX_TURNS {
        for call in &resp.tool_calls {
            tools_called.push(call.name.clone());
            if matches!(call.name.as_str(), "reply" | "post" | "dm_user" | "group_dm") {
                if let Some(text) = call.arguments["text"].as_str() {
                    reply_texts.push(text.to_string());
                }
            }
            // Dispatch skill tools in final turn too
            if let Some(skill_tool) = registry.get_skill_tool(&call.name).await {
                dynamic_registry::execute_skill_tool(&skill_tool, &call.arguments, ws.path(), Some(&cred_store)).await;
            } else {
                tools::execute_tool(call, &ctx).await;
            }
        }
    }

    Ok(EvalResult {
        response: resp.content,
        reply_text: reply_texts.join("\n"),
        tools_called,
        tokens_used: total_tokens,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
