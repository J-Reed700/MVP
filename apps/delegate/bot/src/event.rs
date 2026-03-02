use serde_json::Value;

/// Normalized event from any messaging platform. All incoming payloads get mapped to this.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
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
