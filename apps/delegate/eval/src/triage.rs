use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::{CompleteOptions, ModelClient, ModelResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriageLabel {
    Ignore,
    Queue,
    #[serde(rename = "act-now")]
    ActNow,
}

impl std::fmt::Display for TriageLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore => write!(f, "ignore"),
            Self::Queue => write!(f, "queue"),
            Self::ActNow => write!(f, "act-now"),
        }
    }
}

impl TriageLabel {
    pub fn all() -> [TriageLabel; 3] {
        [Self::Ignore, Self::Queue, Self::ActNow]
    }

    fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "ignore" => Self::Ignore,
            "act-now" => Self::ActNow,
            _ => Self::Queue,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriageEvent {
    pub id: u32,
    pub r#type: String,
    pub channel: Option<String>,
    pub ticket: Option<String>,
    pub action: Option<String>,
    pub user: String,
    pub content: String,
    pub label: Option<TriageLabel>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TriageResult {
    pub event_id: u32,
    pub human_label: TriageLabel,
    pub model_label: TriageLabel,
    pub model_reasoning: String,
    pub correct: bool,
    pub response: ModelResponse,
}

#[derive(Debug, Clone)]
pub struct TriageMetrics {
    pub total: usize,
    pub correct: usize,
    pub overall_agreement: f64,
    pub act_now_events: usize,
    pub missed_act_now: usize,
    pub false_negative_rate: f64,
    pub false_act_now: usize,
    pub false_positive_rate: f64,
    pub confusion_matrix: HashMap<TriageLabel, HashMap<TriageLabel, usize>>,
}

/// Tier 0: Fast pattern matching on event structure.
pub fn tier0_classify(event: &TriageEvent) -> Option<TriageLabel> {
    if event.user.starts_with("bot:") {
        if event.content.to_lowercase().contains("passed") && event.r#type == "slack" {
            return Some(TriageLabel::Ignore);
        }
        if event.content.contains("has joined") || event.content.contains("has left") {
            return Some(TriageLabel::Ignore);
        }
        if event.channel.as_deref() == Some("#general")
            || event.channel.as_deref() == Some("#random")
        {
            return Some(TriageLabel::Ignore);
        }
    }

    if event.channel.as_deref() == Some("#random") {
        return Some(TriageLabel::Ignore);
    }

    None
}

/// Tier 1: Model-based classification using a cheap model.
pub async fn tier1_classify(
    event: &TriageEvent,
    intent_summary: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<(TriageLabel, String, ModelResponse)> {
    let system = format!(
        r#"You are an event triage system for a software engineering team. You classify incoming events into three categories:

- **ignore**: No relevance to team priorities. Bot noise, social chat, routine notifications, trivial PRs.
- **queue**: Relevant to team work but not time-sensitive. Track for next digest. Routine updates, code reviews in progress, informational posts.
- **act-now**: Requires immediate attention. Blockers on critical work, production incidents, stakeholder escalations, risks materializing, team members blocked.

Current team priorities (compressed):
{intent_summary}

Respond with EXACTLY this format:
LABEL: <ignore|queue|act-now>
REASONING: <one sentence explanation>"#
    );

    let event_description = format_event(event);

    let response = client
        .complete(CompleteOptions {
            system,
            prompt: format!("Classify this event:\n{event_description}"),
            model: Some(
                model
                    .unwrap_or("claude-haiku-4-5-20251001")
                    .to_string(),
            ),
            max_tokens: Some(500),
            temperature: None,
        })
        .await?;

    let (label, reasoning) = parse_triage_response(&response.content);

    Ok((label, reasoning, response))
}

/// Run triage on a batch of events. Uses Tier 0 first, then Tier 1 for remaining.
pub async fn triage_batch(
    events: &[TriageEvent],
    intent_summary: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<(Vec<TriageResult>, TriageMetrics)> {
    let mut results = Vec::new();

    for event in events {
        let human_label = event.label.unwrap_or(TriageLabel::Ignore);

        let (model_label, model_reasoning, response) = match tier0_classify(event) {
            Some(label) => (
                label,
                "Tier 0 pattern match".to_string(),
                ModelResponse {
                    content: format!("LABEL: {label}\nREASONING: Tier 0 pattern match"),
                    model: "tier0".to_string(),
                    input_tokens: 0,
                    output_tokens: 0,
                    duration_ms: 0,
                },
            ),
            None => tier1_classify(event, intent_summary, client, model).await?,
        };

        results.push(TriageResult {
            event_id: event.id,
            human_label,
            model_label,
            model_reasoning,
            correct: human_label == model_label,
            response,
        });
    }

    let metrics = compute_metrics(&results);
    Ok((results, metrics))
}

fn compute_metrics(results: &[TriageResult]) -> TriageMetrics {
    let total = results.len();
    let correct = results.iter().filter(|r| r.correct).count();

    let mut matrix: HashMap<TriageLabel, HashMap<TriageLabel, usize>> = HashMap::new();
    for label in TriageLabel::all() {
        let mut row = HashMap::new();
        for col in TriageLabel::all() {
            row.insert(col, 0);
        }
        matrix.insert(label, row);
    }

    for r in results {
        *matrix
            .get_mut(&r.human_label)
            .unwrap()
            .get_mut(&r.model_label)
            .unwrap() += 1;
    }

    let act_now_events = results.iter().filter(|r| r.human_label == TriageLabel::ActNow).count();
    let missed_act_now = results
        .iter()
        .filter(|r| r.human_label == TriageLabel::ActNow && r.model_label != TriageLabel::ActNow)
        .count();
    let non_act_now = total - act_now_events;
    let false_act_now = results
        .iter()
        .filter(|r| r.human_label != TriageLabel::ActNow && r.model_label == TriageLabel::ActNow)
        .count();

    TriageMetrics {
        total,
        correct,
        overall_agreement: if total > 0 { correct as f64 / total as f64 } else { 0.0 },
        act_now_events,
        missed_act_now,
        false_negative_rate: if act_now_events > 0 {
            missed_act_now as f64 / act_now_events as f64
        } else {
            0.0
        },
        false_act_now,
        false_positive_rate: if non_act_now > 0 {
            false_act_now as f64 / non_act_now as f64
        } else {
            0.0
        },
        confusion_matrix: matrix,
    }
}

fn format_event(event: &TriageEvent) -> String {
    let mut parts = Vec::new();
    parts.push(format!("Type: {}", event.r#type));
    if let Some(ref channel) = event.channel {
        parts.push(format!("Channel: {channel}"));
    }
    if let Some(ref ticket) = event.ticket {
        parts.push(format!("Ticket: {ticket}"));
    }
    if let Some(ref action) = event.action {
        parts.push(format!("Action: {action}"));
    }
    parts.push(format!("User: {}", event.user));
    parts.push(format!("Content: {}", event.content));
    parts.join("\n")
}

fn parse_triage_response(text: &str) -> (TriageLabel, String) {
    // Try structured format first: "LABEL: <label>"
    let label = regex::Regex::new(r"(?i)LABEL:\s*(ignore|queue|act-now)")
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|caps| caps.get(1))
        .map(|m| TriageLabel::parse(m.as_str()))
        // Fallback: look for the label anywhere in the text (handles models that
        // don't follow the exact format, e.g. "**act-now**" or just "act-now")
        .or_else(|| {
            let lower = text.to_lowercase();
            if lower.contains("act-now") || lower.contains("act now") {
                Some(TriageLabel::ActNow)
            } else if lower.contains("ignore") {
                Some(TriageLabel::Ignore)
            } else {
                None
            }
        })
        .unwrap_or(TriageLabel::Queue);

    let reasoning = regex::Regex::new(r"(?i)REASONING:\s*(.+)")
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| {
            // Use the full text as reasoning if no REASONING: prefix found,
            // but truncate to keep reports readable.
            let trimmed = text.trim();
            if trimmed.len() > 200 {
                format!("{}...", &trimmed[..200])
            } else {
                trimmed.to_string()
            }
        });

    (label, reasoning)
}
