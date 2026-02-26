use serde_json::Value;

/// Normalized event from Slack. All incoming payloads get mapped to this.
#[derive(Debug, Clone)]
pub struct DelegateEvent {
    pub id: String,
    pub event_type: String,
    pub channel: String,
    pub user: String,
    pub content: String,
    pub timestamp: String,
    pub thread_ts: Option<String>,
    pub raw: Value,
}

/// Parse a Slack Socket Mode envelope into a DelegateEvent.
/// Socket Mode wraps events inside payload.event.
pub fn normalize(envelope: &Value) -> Option<DelegateEvent> {
    let event = envelope
        .get("payload")
        .and_then(|p| p.get("event"))
        .or_else(|| envelope.get("event"))?;
    let event_type = event.get("type")?.as_str()?.to_string();

    match event_type.as_str() {
        "message" => normalize_message(event, &event_type),
        "app_mention" => normalize_message(event, &event_type),
        "reaction_added" => normalize_reaction(event),
        _ => None,
    }
}

fn normalize_message(event: &Value, event_type: &str) -> Option<DelegateEvent> {
    // Skip message subtypes like message_changed, message_deleted, bot_message
    if let Some(subtype) = event.get("subtype").and_then(|s| s.as_str()) {
        match subtype {
            "message_changed" | "message_deleted" | "channel_join" | "channel_leave" => {
                return None
            }
            _ => {}
        }
    }

    let channel = event.get("channel")?.as_str()?.to_string();
    let user = event
        .get("user")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown")
        .to_string();
    let text = event
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let ts = event
        .get("ts")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let thread_ts = event
        .get("thread_ts")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    Some(DelegateEvent {
        id: ts.clone(),
        event_type: event_type.to_string(),
        channel,
        user,
        content: text,
        timestamp: ts,
        thread_ts,
        raw: event.clone(),
    })
}

fn normalize_reaction(event: &Value) -> Option<DelegateEvent> {
    let user = event.get("user")?.as_str()?.to_string();
    let reaction = event
        .get("reaction")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    let item = event.get("item")?;
    let channel = item
        .get("channel")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let ts = item
        .get("ts")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Some(DelegateEvent {
        id: format!("reaction-{ts}"),
        event_type: "reaction_added".to_string(),
        channel,
        user,
        content: format!(":{reaction}:"),
        timestamp: ts,
        thread_ts: None,
        raw: event.clone(),
    })
}
