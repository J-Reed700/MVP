use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::event::DelegateEvent;

// ── Newtypes ───────────────────────────────────────────────────────────
//
// These wrap platform-specific identifiers in distinct types so the compiler
// catches mix-ups (e.g. passing a user ID where a channel ID is expected).
// They implement Display, AsRef<str>, From<&str>, From<String>, and Serde
// traits for ergonomic use at boundaries.

macro_rules! newtype_string {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str { &self.0 }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str { &self.0 }
        }

        impl $name {
            pub fn as_str(&self) -> &str { &self.0 }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self { Self(s.to_string()) }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self { Self(s) }
        }
    };
}

newtype_string! {
    /// A channel ID (e.g. "C012345" on Slack).
    ChannelId
}

newtype_string! {
    /// A user ID (e.g. "U012345" on Slack).
    UserId
}

newtype_string! {
    /// A message timestamp (e.g. "1234567890.123456" on Slack).
    MessageTs
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

    /// Create a new channel. Returns the channel ID and name.
    async fn create_channel(&self, name: &str, purpose: Option<&str>) -> Result<SentMessage>;

    /// Invite one or more users to a channel.
    async fn invite_to_channel(&self, channel: &str, user_ids: &[String]) -> Result<()>;

    /// Open a group DM with multiple users and send a message.
    async fn send_group_dm(&self, user_ids: &[String], text: &str) -> Result<SentMessage>;

    /// Upload a file to a channel (optionally in a thread).
    /// Returns the permalink or a confirmation string.
    async fn upload_file(
        &self,
        channel: &str,
        filename: &str,
        content: &[u8],
        thread_ts: Option<&str>,
        initial_comment: Option<&str>,
    ) -> Result<String>;
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
