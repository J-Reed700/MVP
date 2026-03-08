use serde_json::Value;
use tracing::warn;

use crate::budget::TokenBudget;
use crate::dynamic_registry::DynamicRegistry;
use crate::event::DelegateEvent;
use crate::messenger::Messenger;
use crate::models::{ChatOptions, ModelClient, ModelResponse};
use crate::tools::{self, ToolContext};
use crate::workspace::Workspace;

/// Configuration for a multi-turn tool loop.
pub struct ToolLoopConfig {
    pub system: String,
    pub model: Option<String>,
    pub tools: Vec<Value>,
    pub max_turns: usize,
    pub max_tokens: u32,
    pub temperature: f64,
}

/// Result of a completed tool loop.
pub struct ToolLoopOutcome {
    pub final_content: String,
    pub total_tokens: u64,
}

/// Run a multi-turn tool loop: execute tool calls, feed results back to the LLM, repeat.
///
/// Used by heartbeat and cron jobs. The event handler has its own loop due to
/// approval workflow and reply tracking.
pub async fn run_tool_loop(
    initial_response: ModelResponse,
    initial_prompt: &str,
    client: &ModelClient,
    messenger: &dyn Messenger,
    ws: &Workspace,
    event: &DelegateEvent,
    thread_ts: &str,
    budget: &TokenBudget,
    config: &ToolLoopConfig,
    dynamic_registry: Option<&DynamicRegistry>,
) -> ToolLoopOutcome {
    let ctx = ToolContext {
        messenger,
        ws,
        event,
        thread_ts,
    };
    let mut conversation: Vec<Value> = vec![
        serde_json::json!({"role": "user", "content": initial_prompt}),
    ];
    let mut resp = initial_response;
    let mut total_tokens: u64 = 0;
    let mut turn = 0;

    while !resp.tool_calls.is_empty() && turn < config.max_turns {
        conversation.push(resp.raw_assistant_message.clone());

        for call in &resp.tool_calls {
            // Dispatch: try skill-defined tools first, then static tools
            let result = if let Some(reg) = dynamic_registry {
                if let Some(skill_tool) = reg.get_skill_tool(&call.name).await {
                    let cred_store = reg.get_credential_store().await;
                    crate::dynamic_registry::execute_skill_tool(
                        &skill_tool,
                        &call.arguments,
                        ws.path(),
                        cred_store.as_ref().map(|s| s.as_ref()),
                    ).await
                } else {
                    tools::execute_tool(call, &ctx).await
                }
            } else {
                tools::execute_tool(call, &ctx).await
            };
            conversation.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result
            }));
        }

        if !budget.is_available().await {
            break;
        }

        match client
            .chat(ChatOptions {
                system: config.system.clone(),
                messages: conversation.clone(),
                model: config.model.clone(),
                max_tokens: Some(config.max_tokens),
                temperature: Some(config.temperature),
                tools: Some(config.tools.clone()),
            })
            .await
        {
            Ok(r) => {
                let tokens = r.input_tokens + r.output_tokens;
                total_tokens += tokens;
                budget.record(tokens).await;
                resp = r;
            }
            Err(e) => {
                warn!("Tool loop LLM call failed: {e}");
                break;
            }
        }

        turn += 1;
    }

    ToolLoopOutcome {
        final_content: resp.content,
        total_tokens,
    }
}
