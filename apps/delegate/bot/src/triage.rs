use anyhow::Result;
use tracing::debug;

use crate::event::DelegateEvent;
use crate::models::{CompleteOptions, ModelClient};

/// Returns a cheap/fast model for triage based on the provider.
fn triage_model(client: &ModelClient) -> &'static str {
    match client {
        ModelClient::Anthropic { .. } => "claude-haiku-4-5-20251001",
        ModelClient::OpenAI { .. } => "gpt-4.1-nano",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageLabel {
    Ignore,
    Queue,
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

/// Tier 0: Fast pattern matching. Returns Some(label) if classifiable without LLM.
pub fn tier0_classify(
    event: &DelegateEvent,
    bot_user_id: &str,
) -> Option<TriageLabel> {
    // Ignore messages from the bot itself
    if event.user == bot_user_id {
        return Some(TriageLabel::Ignore);
    }

    // Always process app_mentions (someone directly @mentioned the bot)
    if event.event_type == "app_mention" {
        return Some(TriageLabel::ActNow);
    }

    // Ignore reactions (for now)
    if event.event_type == "reaction_added" {
        return Some(TriageLabel::Ignore);
    }

    // Bot noise patterns — hard filter
    let lower = event.content.to_lowercase();
    if lower.contains("has joined the channel") || lower.contains("has left the channel") {
        return Some(TriageLabel::Ignore);
    }

    // Everything else in watched channels → let Tier 1 (LLM) decide
    None
}

/// Tier 1: Model-based classification using a cheap/fast model.
pub async fn tier1_classify(
    event: &DelegateEvent,
    intent_summary: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<(TriageLabel, String)> {
    let system = format!(
        r#"You are an event triage system for a software engineering team. You classify incoming events into three categories:

- **ignore**: Only use for pure noise: automated bot notifications, CI build logs, channel join/leave messages, or messages that are clearly not directed at anyone.
- **queue**: Routine updates that don't need a response right now.
- **act-now**: DEFAULT CHOICE. Use this for almost everything — questions, comments, discussions, someone talking to or about the bot, greetings, requests, opinions, complaints, anything where a human PM might notice and potentially engage. The bot has tools to choose its response level (emoji, reply, or no_action), so err heavily toward act-now and let the main model decide what to do.

Current team priorities (compressed):
{intent_summary}

Respond with EXACTLY this format:
LABEL: <ignore|queue|act-now>
REASONING: <one sentence explanation>"#
    );

    let event_description = format!(
        "Type: {}\nChannel: {}\nUser: {}\nContent: {}",
        event.event_type, event.channel, event.user, event.content
    );

    let response = client
        .complete(CompleteOptions {
            system,
            prompt: format!("Classify this event:\n{event_description}"),
            model: Some(
                model
                    .unwrap_or_else(|| triage_model(client))
                    .to_string(),
            ),
            max_tokens: Some(500),
            temperature: None,
            tools: None,
        })
        .await?;

    let (label, reasoning) = parse_triage_response(&response.content);
    debug!(label = %label, reasoning = %reasoning, "Tier 1 triage result");

    Ok((label, reasoning))
}

fn parse_triage_response(text: &str) -> (TriageLabel, String) {
    let label = regex::Regex::new(r"(?i)LABEL:\s*(ignore|queue|act-now)")
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|caps| caps.get(1))
        .map(|m| match m.as_str().to_lowercase().as_str() {
            "ignore" => TriageLabel::Ignore,
            "act-now" => TriageLabel::ActNow,
            _ => TriageLabel::Queue,
        })
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
            let trimmed = text.trim();
            if trimmed.len() > 200 {
                format!("{}...", &trimmed[..200])
            } else {
                trimmed.to_string()
            }
        });

    (label, reasoning)
}
