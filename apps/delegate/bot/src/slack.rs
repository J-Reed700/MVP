use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};
#[allow(unused_imports)]
use tracing as _;

/// Slack Socket Mode client.
/// Connects via WebSocket, receives events, sends acknowledgments.
pub struct SlackSocket {
    pub app_token: String,
    pub bot_token: String,
    http: reqwest::Client,
    user_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl SlackSocket {
    pub fn new(app_token: String, bot_token: String) -> Self {
        Self {
            app_token,
            bot_token,
            http: reqwest::Client::new(),
            user_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Open a Socket Mode connection and forward events to the channel.
    /// Reconnects automatically on disconnect.
    pub async fn run(&self, tx: mpsc::Sender<Value>) -> Result<()> {
        let mut backoff_secs = 1u64;
        loop {
            match self.connect_and_listen(&tx).await {
                Ok(()) => {
                    info!("Socket Mode connection closed, reconnecting...");
                    backoff_secs = 1; // Reset on clean disconnect
                }
                Err(e) => {
                    error!("Socket Mode error: {e}, reconnecting in {backoff_secs}s...");
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60); // Exponential backoff, max 60s
                }
            }
        }
    }

    async fn connect_and_listen(&self, tx: &mpsc::Sender<Value>) -> Result<()> {
        let wss_url = self.get_wss_url().await?;
        info!("Connecting to Slack Socket Mode...");

        let (ws_stream, _) = tokio_tungstenite::connect_async(&wss_url).await?;
        let (mut write, mut read) = ws_stream.split();

        info!("Connected to Slack via Socket Mode");

        while let Some(msg) = read.next().await {
            let msg = msg?;
            match msg {
                Message::Text(text) => {
                    info!("Raw WS message: {}", &text[..text.len().min(500)]);
                    let payload: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Failed to parse Socket Mode message: {e}");
                            continue;
                        }
                    };

                    // Acknowledge the envelope
                    if let Some(envelope_id) = payload.get("envelope_id").and_then(|v| v.as_str())
                    {
                        let ack = serde_json::json!({ "envelope_id": envelope_id });
                        write.send(Message::Text(ack.to_string().into())).await?;
                        debug!("Acknowledged envelope: {envelope_id}");
                    }

                    // Check payload type
                    let payload_type = payload
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");

                    match payload_type {
                        "events_api" => {
                            if let Err(e) = tx.send(payload.clone()).await {
                                error!("Failed to forward event: {e}");
                            }
                        }
                        "hello" => {
                            info!("Received hello from Slack");
                        }
                        "disconnect" => {
                            info!("Slack requested disconnect, will reconnect");
                            return Ok(());
                        }
                        other => {
                            debug!("Ignoring Socket Mode payload type: {other}");
                        }
                    }
                }
                Message::Ping(data) => {
                    write.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => {
                    info!("WebSocket closed by server");
                    return Ok(());
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Request a WebSocket URL from Slack's apps.connections.open endpoint.
    async fn get_wss_url(&self) -> Result<String> {
        let resp = self
            .http
            .post("https://slack.com/api/apps.connections.open")
            .header("Authorization", format!("Bearer {}", self.app_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;

        let body: Value = resp.json().await?;

        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("apps.connections.open failed: {err}"));
        }

        body.get("url")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("No URL in apps.connections.open response"))
    }

    /// Post a message to a Slack channel.
    pub async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<Value> {
        let mut body = serde_json::json!({
            "channel": channel,
            "text": text,
        });

        if let Some(ts) = thread_ts {
            body["thread_ts"] = serde_json::json!(ts);
        }

        let resp = self
            .http
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let result: Value = resp.json().await?;

        if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            error!("chat.postMessage failed: {err}");
            return Err(anyhow!("chat.postMessage failed: {err}"));
        }

        Ok(result)
    }

    /// Update an existing message.
    pub async fn update_message(
        &self,
        channel: &str,
        timestamp: &str,
        text: &str,
    ) -> Result<()> {
        let body = serde_json::json!({
            "channel": channel,
            "ts": timestamp,
            "text": text,
        });

        let resp = self
            .http
            .post("https://slack.com/api/chat.update")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let result: Value = resp.json().await?;

        if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            error!("chat.update failed: {err}");
            return Err(anyhow!("chat.update failed: {err}"));
        }

        Ok(())
    }

    /// Fetch thread replies for a message.
    pub async fn get_thread(&self, channel: &str, thread_ts: &str) -> Result<Vec<Value>> {
        let resp = self
            .http
            .get("https://slack.com/api/conversations.replies")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .query(&[("channel", channel), ("ts", thread_ts), ("limit", "20")])
            .send()
            .await?;

        let body: Value = resp.json().await?;

        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            warn!("conversations.replies failed: {err}");
            return Ok(Vec::new());
        }

        Ok(body["messages"].as_array().cloned().unwrap_or_default())
    }

    /// Delete a message.
    pub async fn delete_message(&self, channel: &str, timestamp: &str) -> Result<()> {
        let body = serde_json::json!({
            "channel": channel,
            "ts": timestamp,
        });

        let resp = self
            .http
            .post("https://slack.com/api/chat.delete")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let result: Value = resp.json().await?;

        if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            warn!("chat.delete failed: {err}");
        }

        Ok(())
    }

    /// Add a reaction to a message.
    pub async fn add_reaction(
        &self,
        channel: &str,
        timestamp: &str,
        emoji: &str,
    ) -> Result<()> {
        let body = serde_json::json!({
            "channel": channel,
            "timestamp": timestamp,
            "name": emoji,
        });

        let resp = self
            .http
            .post("https://slack.com/api/reactions.add")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let result: Value = resp.json().await?;

        if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            // Don't fail on already_reacted
            if err != "already_reacted" {
                warn!("reactions.add failed: {err}");
            }
        }

        Ok(())
    }

    /// Resolve a user ID to a display name. Results are cached.
    pub async fn get_user_name(&self, user_id: &str) -> String {
        // Check cache first
        {
            let cache = self.user_cache.lock().await;
            if let Some(name) = cache.get(user_id) {
                return name.clone();
            }
        }

        // Call users.info API
        let resp = self
            .http
            .get("https://slack.com/api/users.info")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .query(&[("user", user_id)])
            .send()
            .await;

        let name = match resp {
            Ok(r) => {
                let body: Value = r.json().await.unwrap_or_default();
                if body["ok"].as_bool() == Some(true) {
                    body["user"]["profile"]["display_name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .or_else(|| body["user"]["real_name"].as_str())
                        .unwrap_or(user_id)
                        .to_string()
                } else {
                    user_id.to_string()
                }
            }
            Err(_) => user_id.to_string(),
        };

        // Cache it
        {
            let mut cache = self.user_cache.lock().await;
            cache.insert(user_id.to_string(), name.clone());
        }

        name
    }
}
