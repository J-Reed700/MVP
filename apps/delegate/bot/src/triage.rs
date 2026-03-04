use anyhow::Result;
use tracing::debug;

use crate::event::DelegateEvent;
use crate::messenger::Transport;
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
///
/// `watched_channels` is an optional set of channel IDs. If provided and non-empty,
/// messages from channels NOT in the set are ignored. If None, all channels are processed.
pub fn tier0_classify(
    event: &DelegateEvent,
    transport: &dyn Transport,
    watched_channels: Option<&std::collections::HashSet<String>>,
) -> Option<TriageLabel> {
    // Ignore messages from the bot itself
    if transport.is_self_message(event.user.as_str()) {
        return Some(TriageLabel::Ignore);
    }

    // Always process app_mentions (someone directly @mentioned the bot).
    if event.event_type == "app_mention"
        || transport.is_mention(&event.content)
    {
        return Some(TriageLabel::ActNow);
    }

    // Reactions pass through to main handler (for approval workflow)
    if event.event_type == "reaction_added" {
        return None;
    }

    // Channel watch-list filtering: if a watch list is configured, only process
    // events from channels in the set. Direct @mentions already bypassed above.
    if let Some(watched) = watched_channels {
        if !watched.is_empty() && !watched.contains(event.channel.as_str()) {
            return Some(TriageLabel::Ignore);
        }
    }

    // Empty messages (file uploads with no text, etc.)
    if event.content.trim().is_empty() {
        return Some(TriageLabel::Ignore);
    }

    // Bot noise patterns — hard filter
    let lower = event.content.to_lowercase();
    if lower.contains("has joined the channel") || lower.contains("has left the channel") {
        return Some(TriageLabel::Ignore);
    }

    // Everything else → let Tier 1 (LLM) decide
    None
}

/// Tier 1: Model-based classification using a cheap/fast model.
/// Returns (label, reasoning, tokens_used).
pub async fn tier1_classify(
    event: &DelegateEvent,
    intent_summary: &str,
    client: &ModelClient,
    model: Option<&str>,
) -> Result<(TriageLabel, String, u64)> {
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

    let tokens_used = response.input_tokens + response.output_tokens;
    let (label, reasoning) = parse_triage_response(&response.content);
    debug!(label = %label, reasoning = %reasoning, tokens = tokens_used, "Tier 1 triage result");

    Ok((label, reasoning, tokens_used))
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
        .unwrap_or(TriageLabel::ActNow); // Default to act-now per triage prompt guidance

    let reasoning = regex::Regex::new(r"(?i)REASONING:\s*(.+)")
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| {
            let trimmed = text.trim();
            if trimmed.len() > 200 {
                // Find a char boundary at or before 200 bytes
                let mut end = 200;
                while end > 0 && !trimmed.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &trimmed[..end])
            } else {
                trimmed.to_string()
            }
        });

    (label, reasoning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use async_trait::async_trait;
    use serde_json::Value;
    use tokio::sync::mpsc;
    use crate::event::DelegateEvent;

    struct MockTransport {
        bot_id: String,
    }

    impl MockTransport {
        fn new(bot_id: &str) -> Self {
            Self { bot_id: bot_id.to_string() }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        fn bot_user_id(&self) -> &str { &self.bot_id }
        fn is_mention(&self, text: &str) -> bool {
            text.contains(&format!("<@{}>", self.bot_id))
        }
        fn strip_mentions(&self, text: &str) -> String {
            text.replace(&format!("<@{}>", self.bot_id), "@Delegate").trim().to_string()
        }
        fn is_dm_channel(&self, channel_id: &str) -> bool { channel_id.starts_with('D') }
        fn normalize_event(&self, _raw: &Value) -> Option<DelegateEvent> { unimplemented!() }
        fn is_valid_user_id(&self, _id: &str) -> bool { unimplemented!() }
        async fn listen(&self, _tx: mpsc::Sender<Value>) -> anyhow::Result<()> { unimplemented!() }
    }

    fn make_event(event_type: &str, channel: &str, user: &str, content: &str) -> DelegateEvent {
        use crate::messenger::{ChannelId, UserId, MessageTs};
        DelegateEvent {
            id: "1234".to_string(),
            event_type: event_type.to_string(),
            channel: ChannelId::from(channel),
            user: UserId::from(user),
            content: content.to_string(),
            timestamp: MessageTs::from("1234567890.123456"),
            thread_ts: None,
            raw: serde_json::Value::Null,
        }
    }

    #[test]
    fn tier0_ignores_bot_own_messages() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C123", "U_BOT", "hello");
        assert_eq!(tier0_classify(&event, &t, None), Some(TriageLabel::Ignore));
    }

    #[test]
    fn tier0_act_now_on_mention() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("app_mention", "C123", "U999", "hey <@U_BOT>");
        assert_eq!(tier0_classify(&event, &t, None), Some(TriageLabel::ActNow));
    }

    #[test]
    fn tier0_act_now_on_inline_mention() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C123", "U999", "hey <@U_BOT> what's up?");
        assert_eq!(tier0_classify(&event, &t, None), Some(TriageLabel::ActNow));
    }

    #[test]
    fn tier0_ignores_empty_content() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C123", "U999", "   ");
        assert_eq!(tier0_classify(&event, &t, None), Some(TriageLabel::Ignore));
    }

    #[test]
    fn tier0_ignores_join_messages() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C123", "U999", "Alice has joined the channel");
        assert_eq!(tier0_classify(&event, &t, None), Some(TriageLabel::Ignore));
    }

    #[test]
    fn tier0_passes_reactions_through() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("reaction_added", "C123", "U999", ":thumbsup:");
        assert_eq!(tier0_classify(&event, &t, None), None);
    }

    #[test]
    fn tier0_respects_watch_list() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C_UNWATCHED", "U999", "hello");
        let mut watched = HashSet::new();
        watched.insert("C_WATCHED".to_string());
        assert_eq!(
            tier0_classify(&event, &t, Some(&watched)),
            Some(TriageLabel::Ignore)
        );
    }

    #[test]
    fn tier0_allows_watched_channel() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C_WATCHED", "U999", "hello");
        let mut watched = HashSet::new();
        watched.insert("C_WATCHED".to_string());
        assert_eq!(tier0_classify(&event, &t, Some(&watched)), None);
    }

    #[test]
    fn tier0_empty_watch_list_allows_all() {
        let t = MockTransport::new("U_BOT");
        let event = make_event("message", "C123", "U999", "hello");
        let watched = HashSet::new();
        assert_eq!(tier0_classify(&event, &t, Some(&watched)), None);
    }

    #[test]
    fn parse_triage_structured_response() {
        let (label, reasoning) = parse_triage_response(
            "LABEL: act-now\nREASONING: User asked a direct question about the deploy."
        );
        assert_eq!(label, TriageLabel::ActNow);
        assert!(reasoning.contains("direct question"));
    }

    #[test]
    fn parse_triage_ignore() {
        let (label, _) = parse_triage_response("LABEL: ignore\nREASONING: Bot notification.");
        assert_eq!(label, TriageLabel::Ignore);
    }

    #[test]
    fn parse_triage_queue() {
        let (label, _) = parse_triage_response("LABEL: queue\nREASONING: Routine status update.");
        assert_eq!(label, TriageLabel::Queue);
    }

    #[test]
    fn parse_triage_fallback_defaults_to_act_now() {
        let (label, _) = parse_triage_response("I'm not sure what to do with this.");
        assert_eq!(label, TriageLabel::ActNow);
    }

    #[test]
    fn parse_triage_fuzzy_match() {
        let (label, _) = parse_triage_response("This looks like it should be act-now because someone asked.");
        assert_eq!(label, TriageLabel::ActNow);
    }
}
