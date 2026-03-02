use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::event::DelegateEvent;

// ── Newtypes ───────────────────────────────────────────────────────────

/// A channel ID (e.g. "C012345" on Slack).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub struct ChannelId(pub String);

/// A user ID (e.g. "U012345" on Slack).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub struct UserId(pub String);

/// A message timestamp (e.g. "1234567890.123456" on Slack).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub struct MessageTs(pub String);

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for MessageTs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[allow(dead_code)]
impl ChannelId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[allow(dead_code)]
impl UserId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate that this looks like a valid user ID.
    /// Platform-specific validation should go through `Transport::is_valid_user_id()`.
    pub fn is_valid_id(&self) -> bool {
        !self.0.is_empty()
    }
}

#[allow(dead_code)]
impl MessageTs {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Allow &str → newtype conversions for ergonomic use at boundaries
impl From<&str> for ChannelId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ChannelId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MessageTs {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for MessageTs {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ── Message types ──────────────────────────────────────────────────────

/// A sent message reference — enough to react to it, thread on it, or track it.
#[derive(Debug, Clone)]
pub struct SentMessage {
    pub channel: String,
    pub timestamp: String,
}

/// A message retrieved from history or a thread.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub user_id: String,
    pub text: String,
    pub timestamp: String,
    /// Raw platform payload — preserves attachments, files, reactions, blocks
    /// that the structured fields don't capture.
    #[allow(dead_code)]
    pub raw: Option<Value>,
}

// ── Messenger trait ────────────────────────────────────────────────────

/// Platform-agnostic messaging interface.
///
/// Every operation the bot performs against a messaging platform goes through
/// this trait. Implementations exist for Slack (SlackSocket), and can be added
/// for Teams, Email, Discord, or a test harness.
#[async_trait]
pub trait Messenger: Send + Sync {
    async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SentMessage>;

    async fn send_dm(&self, user_id: &str, text: &str) -> Result<SentMessage>;

    async fn add_reaction(
        &self,
        channel: &str,
        timestamp: &str,
        emoji: &str,
    ) -> Result<()>;

    #[allow(dead_code)]
    async fn update_message(
        &self,
        channel: &str,
        timestamp: &str,
        text: &str,
    ) -> Result<()>;

    #[allow(dead_code)]
    async fn delete_message(&self, channel: &str, timestamp: &str) -> Result<()>;

    async fn get_thread(
        &self,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Vec<ChatMessage>>;

    async fn get_channel_history(
        &self,
        channel: &str,
        limit: u32,
    ) -> Result<Vec<ChatMessage>>;

    async fn get_user_name(&self, user_id: &str) -> String;
    async fn get_channel_name(&self, channel_id: &str) -> String;
    async fn resolve_channel_id(&self, channel_name: &str) -> Option<String>;
    async fn find_user_by_name(&self, query: &str) -> Result<Vec<(String, String)>>;
}

// ── Transport trait ───────────────────────────────────────────────────

/// Platform-specific event transport: identity, event normalization, mention
/// detection, and the listener loop.
///
/// Separate from `Messenger` (which handles outbound operations) to avoid
/// Rust's `dyn` coercion limitations. A single concrete type (e.g. SlackSocket)
/// implements both traits independently.
#[async_trait]
pub trait Transport: Send + Sync {
    /// The bot's own user ID on this platform.
    fn bot_user_id(&self) -> &str;

    /// Returns true if the given user ID is the bot itself.
    fn is_self_message(&self, user_id: &str) -> bool {
        user_id == self.bot_user_id()
    }

    /// Returns true if the event text contains a mention of the bot.
    fn is_mention(&self, text: &str) -> bool;

    /// Strip bot mentions from message text, replacing with "@Delegate".
    fn strip_mentions(&self, text: &str) -> String;

    /// Returns true if the given channel ID represents a direct message.
    fn is_dm_channel(&self, channel_id: &str) -> bool;

    /// Normalize a raw platform envelope into a DelegateEvent.
    fn normalize_event(&self, raw: &Value) -> Option<DelegateEvent>;

    /// Returns true if the given string is a valid user ID on this platform.
    fn is_valid_user_id(&self, id: &str) -> bool;

    /// Start listening for events and forward them to the channel.
    async fn listen(&self, tx: mpsc::Sender<Value>) -> Result<()>;
}
