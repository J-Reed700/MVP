//! MockMessenger — records all outbound calls without a real Slack connection.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;

use crate::messenger::{ChatMessage, Messenger, SentMessage};

/// Records all outbound calls. No real Slack connection needed.
pub(crate) struct MockMessenger {
    log: Arc<Mutex<Vec<String>>>,
}

impl MockMessenger {
    pub fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn push(&self, entry: String) {
        self.log.lock().unwrap().push(entry);
    }
}

#[async_trait]
impl Messenger for MockMessenger {
    async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SentMessage> {
        self.push(format!("post_message({channel}, {text:?}, {thread_ts:?})"));
        Ok(SentMessage {
            channel: channel.to_string(),
            timestamp: "1000000000.000001".to_string(),
        })
    }

    async fn send_dm(&self, user_id: &str, text: &str) -> Result<SentMessage> {
        self.push(format!("send_dm({user_id}, {text:?})"));
        Ok(SentMessage {
            channel: "D_MOCK".to_string(),
            timestamp: "1000000000.000002".to_string(),
        })
    }

    async fn add_reaction(&self, channel: &str, timestamp: &str, emoji: &str) -> Result<()> {
        self.push(format!("add_reaction({channel}, {timestamp}, {emoji})"));
        Ok(())
    }

    async fn update_message(&self, channel: &str, timestamp: &str, text: &str) -> Result<()> {
        self.push(format!("update_message({channel}, {timestamp}, {text:?})"));
        Ok(())
    }

    async fn delete_message(&self, channel: &str, timestamp: &str) -> Result<()> {
        self.push(format!("delete_message({channel}, {timestamp})"));
        Ok(())
    }

    async fn get_thread(&self, _channel: &str, _thread_ts: &str) -> Result<Vec<ChatMessage>> {
        Ok(vec![])
    }

    async fn get_channel_history(&self, _channel: &str, _limit: u32) -> Result<Vec<ChatMessage>> {
        Ok(vec![])
    }

    async fn get_user_name(&self, _user_id: &str) -> String {
        "test-user".to_string()
    }

    async fn get_channel_name(&self, _channel_id: &str) -> String {
        "test-channel".to_string()
    }

    async fn resolve_channel_id(&self, _channel_name: &str) -> Option<String> {
        Some("C_TEST".to_string())
    }

    async fn find_user_by_name(&self, _query: &str) -> Result<Vec<(String, String)>> {
        Ok(vec![("U_TEST".to_string(), "Test User".to_string())])
    }

    async fn create_channel(&self, name: &str, purpose: Option<&str>) -> Result<SentMessage> {
        self.push(format!("create_channel({name}, {purpose:?})"));
        Ok(SentMessage {
            channel: format!("C_NEW_{}", name.to_uppercase().replace('-', "_")),
            timestamp: String::new(),
        })
    }

    async fn invite_to_channel(&self, channel: &str, user_ids: &[String]) -> Result<()> {
        self.push(format!("invite_to_channel({channel}, {user_ids:?})"));
        Ok(())
    }

    async fn send_group_dm(&self, user_ids: &[String], text: &str) -> Result<SentMessage> {
        self.push(format!("send_group_dm({user_ids:?}, {text:?})"));
        Ok(SentMessage {
            channel: "G_MOCK".to_string(),
            timestamp: "1000000000.000003".to_string(),
        })
    }
}
