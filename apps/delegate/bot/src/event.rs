use serde_json::Value;

use crate::messenger::{ChannelId, MessageTs, UserId};

/// Normalized event from any messaging platform. All incoming payloads get mapped to this.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DelegateEvent {
    pub id: String,
    pub event_type: String,
    pub channel: ChannelId,
    pub user: UserId,
    pub content: String,
    pub timestamp: MessageTs,
    pub thread_ts: Option<MessageTs>,
    pub raw: Value,
}
