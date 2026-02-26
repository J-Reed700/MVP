use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use crate::compiler::{compile, to_prompt, AudienceProfile, CompileOptions, TaskType, Trigger};
use crate::models::{estimate_tokens, CompleteOptions, ModelClient, ModelResponse};
use crate::triage::{triage_batch, TriageEvent, TriageLabel, TriageMetrics, TriageResult};

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub hypothesis: String,
    pub trigger: Option<ScenarioTrigger>,
    pub task: Option<String>,
    pub variants: Option<Vec<Variant>>,
    #[serde(rename = "intentSummary")]
    pub intent_summary: Option<String>,
    pub events: Option<serde_json::Value>,
    pub evaluation: Evaluation,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioTrigger {
    pub r#type: String,
    pub content: String,
    pub channel: Option<String>,
    pub user: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Variant {
    pub id: String,
    pub label: String,
    pub description: String,
    #[serde(rename = "includeIntents")]
    pub include_intents: bool,
    #[serde(rename = "retrievalBias")]
    pub retrieval_bias: Vec<String>,
    #[serde(rename = "retrievalTerms")]
    pub retrieval_terms: Option<Vec<String>>,
    pub audience: AudienceData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudienceData {
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Evaluation {
    pub criteria: Option<Vec<String>>,
    pub metrics: Option<serde_json::Value>,
    #[serde(rename = "killCondition")]
    pub kill_condition: String,
}

#[derive(Debug, Clone)]
pub struct VariantResult {
    pub variant_id: String,
    pub variant_label: String,
    pub response: ModelResponse,
    pub compiled_context: ContextStats,
}

#[derive(Debug, Clone)]
pub struct ContextStats {
    pub identity_tokens: usize,
    pub intents_tokens: usize,
    pub memory_tokens: usize,
    pub retrieved_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct LifecycleStep {
    pub event_id: u32,
    pub event_description: String,
    pub response: ModelResponse,
    pub updated_intents: Option<String>,
}

pub struct ScenarioResult {
    pub scenario_id: String,
    pub scenario_name: String,
    pub hypothesis: String,
    pub timestamp: String,
    pub variant_results: Option<Vec<VariantResult>>,
    pub triage_results: Option<Vec<TriageResult>>,
    pub triage_metrics: Option<TriageMetrics>,
    pub lifecycle_steps: Option<Vec<LifecycleStep>>,
}

/// Load a scenario from a JSONC file.
pub async fn load_scenario(path: &Path) -> Result<Scenario> {
    let raw = tokio::fs::read_to_string(path).await?;
    // Strip JSONC comments
    let json = strip_jsonc_comments(&raw);
    let scenario: Scenario = serde_json::from_str(&json)?;
    Ok(scenario)
}

fn strip_jsonc_comments(input: &str) -> String {
    // Remove single-line comments
    let re_single = regex::Regex::new(r"//.*$").unwrap();
    let without_single: String = input
        .lines()
        .map(|line| re_single.replace_all(line, "").to_string())
        .collect::<Vec<_>>()
        .join("\n");

    // Remove multi-line comments
    let re_multi = regex::Regex::new(r"(?s)/\*.*?\*/").unwrap();
    re_multi.replace_all(&without_single, "").to_string()
}

/// Run a standard A/B or multi-variant scenario.
pub async fn run_variant_scenario(
    scenario: &Scenario,
    workspace: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<ScenarioResult> {
    let mut results = Vec::new();

    for variant in scenario.variants.as_deref().unwrap_or(&[]) {
        println!("  Running variant: {}...", variant.label);

        let trigger_data = scenario.trigger.as_ref().unwrap();
        let search_terms = variant.retrieval_terms.clone().unwrap_or_default();

        let task_type = if scenario.id.contains("audience") {
            TaskType::Update
        } else {
            TaskType::Respond
        };

        let compiled = compile(CompileOptions {
            trigger: Trigger {
                r#type: trigger_data.r#type.clone(),
                content: trigger_data.content.clone(),
                channel: trigger_data.channel.clone(),
                user: trigger_data.user.clone(),
                timestamp: trigger_data.timestamp.clone(),
            },
            workspace: workspace.to_string(),
            include_intents: variant.include_intents,
            retrieval_terms: search_terms,
            retrieval_bias: variant.retrieval_bias.clone(),
            audience: AudienceProfile {
                name: variant.audience.name.clone(),
                instructions: variant.audience.instructions.clone(),
            },
            task_type,
            task: scenario.task.clone().unwrap_or_default(),
            token_budget: 8000,
        })
        .await?;

        let (system, prompt) = to_prompt(&compiled);

        let response = client
            .complete(CompleteOptions {
                system: system.clone(),
                prompt: prompt.clone(),
                model: model.map(|s| s.to_string()),
                max_tokens: Some(2048),
                temperature: None,
            })
            .await?;

        results.push(VariantResult {
            variant_id: variant.id.clone(),
            variant_label: variant.label.clone(),
            response,
            compiled_context: ContextStats {
                identity_tokens: estimate_tokens(&compiled.identity),
                intents_tokens: estimate_tokens(&compiled.intents),
                memory_tokens: estimate_tokens(&compiled.memory),
                retrieved_tokens: estimate_tokens(&compiled.retrieved),
                total_tokens: estimate_tokens(&format!("{system}{prompt}")),
            },
        });
    }

    Ok(ScenarioResult {
        scenario_id: scenario.id.clone(),
        scenario_name: scenario.name.clone(),
        hypothesis: scenario.hypothesis.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        variant_results: Some(results),
        triage_results: None,
        triage_metrics: None,
        lifecycle_steps: None,
    })
}

/// Run the triage classification scenario.
pub async fn run_triage_scenario(
    scenario: &Scenario,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<ScenarioResult> {
    let events: Vec<TriageEvent> =
        serde_json::from_value(scenario.events.clone().unwrap_or_default())?;
    let intent_summary = scenario.intent_summary.as_deref().unwrap_or("");

    println!("  Running triage on {} events...", events.len());

    let (results, metrics) = triage_batch(&events, intent_summary, client, model).await?;

    Ok(ScenarioResult {
        scenario_id: scenario.id.clone(),
        scenario_name: scenario.name.clone(),
        hypothesis: scenario.hypothesis.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        variant_results: None,
        triage_results: Some(results),
        triage_metrics: Some(metrics),
        lifecycle_steps: None,
    })
}

#[derive(Debug, Deserialize)]
struct LifecycleEvent {
    id: u32,
    description: String,
    content: String,
    #[serde(rename = "expectedSignal")]
    expected_signal: String,
}

/// Run the intent lifecycle scenario.
pub async fn run_lifecycle_scenario(
    scenario: &Scenario,
    workspace: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<ScenarioResult> {
    let events: Vec<LifecycleEvent> =
        serde_json::from_value(scenario.events.clone().unwrap_or_default())?;

    let intents_path = Path::new(workspace).join("INTENTS.md");
    let mut current_intents = tokio::fs::read_to_string(&intents_path).await?;

    let mut steps = Vec::new();

    for event in &events {
        println!("  Event {}: {}...", event.id, event.description);

        let mut compiled = compile(CompileOptions {
            trigger: Trigger {
                r#type: "event".to_string(),
                content: event.content.clone(),
                channel: None,
                user: None,
                timestamp: None,
            },
            workspace: workspace.to_string(),
            include_intents: true,
            retrieval_bias: vec![
                "billing migration".to_string(),
                "March 15".to_string(),
                "Sarah".to_string(),
                "API team".to_string(),
            ],
            retrieval_terms: vec![],
            audience: AudienceProfile {
                name: "system".to_string(),
                instructions: String::new(),
            },
            task_type: TaskType::Lifecycle,
            task: scenario.task.clone().unwrap_or_default(),
            token_budget: 6000,
        })
        .await?;

        // Override intents with current version (may have been updated)
        compiled.intents = current_intents.clone();

        let (system, prompt) = to_prompt(&compiled);

        let response = client
            .complete(CompleteOptions {
                system,
                prompt,
                model: model.map(|s| s.to_string()),
                max_tokens: Some(3000),
                temperature: None,
            })
            .await?;

        // Check if the response contains an updated INTENTS.md
        let mut updated_intents: Option<String> = None;
        if response.content.contains("# Active Intents")
            || response.content.contains("# INTENTS")
            || response.content.contains("## 1.")
            || response.content.contains("**Priority:**")
            || response.content.contains("Priority: Critical")
            || response.content.contains("Priority: High")
        {
            // Try multiple heading patterns models might use
            let re = regex::Regex::new(
                r"(#\s*(?:Active Intents|INTENTS(?:\.md)?)\s*\n[\s\S]*)$"
            ).unwrap();
            if let Some(caps) = re.captures(&response.content) {
                let extracted = caps[1].trim().to_string();
                updated_intents = Some(extracted.clone());
                current_intents = extracted;
            } else if let Some(caps) = regex::Regex::new(r"(##\s*1\.[\s\S]*)$")
                .unwrap()
                .captures(&response.content)
            {
                // Fallback: model started with "## 1." without a top-level heading
                let extracted = caps[1].trim().to_string();
                updated_intents = Some(extracted.clone());
                current_intents = extracted;
            }
        }

        steps.push(LifecycleStep {
            event_id: event.id,
            event_description: event.description.clone(),
            response,
            updated_intents,
        });
    }

    Ok(ScenarioResult {
        scenario_id: scenario.id.clone(),
        scenario_name: scenario.name.clone(),
        hypothesis: scenario.hypothesis.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        variant_results: None,
        triage_results: None,
        triage_metrics: None,
        lifecycle_steps: Some(steps),
    })
}
