use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};
#[allow(unused_imports)]
use tracing as _;

use crate::event::DelegateEvent;
use crate::messenger::{ChannelId, ChatMessage, Messenger, MessageTs, SentMessage, Transport, UserId};

/// Slack Socket Mode client.
/// Connects via WebSocket, receives events, sends acknowledgments.
/// Clone is cheap — the HTTP client and user cache are shared via Arc.
#[derive(Clone)]
pub struct SlackSocket {
    pub app_token: String,
    pub bot_token: String,
    pub bot_user_id: String,
    http: reqwest::Client,
    user_cache: Arc<Mutex<HashMap<String, String>>>,
    channel_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl SlackSocket {
    pub fn new(app_token: String, bot_token: String, bot_user_id: String) -> Self {
        Self {
            app_token,
            bot_token,
            bot_user_id,
            http: reqwest::Client::new(),
            user_cache: Arc::new(Mutex::new(HashMap::new())),
            channel_cache: Arc::new(Mutex::new(HashMap::new())),
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
                    backoff_secs = 1;
                }
                Err(e) => {
                    error!("Socket Mode error: {e}, reconnecting in {backoff_secs}s...");
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60);
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

                    if let Some(envelope_id) = payload.get("envelope_id").and_then(|v| v.as_str())
                    {
                        let ack = serde_json::json!({ "envelope_id": envelope_id });
                        write.send(Message::Text(ack.to_string().into())).await?;
                        debug!("Acknowledged envelope: {envelope_id}");
                    }

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

    // ── Internal Slack API helpers with retry ──────────────────────────

    /// Core retry loop for Slack API calls. Handles exponential backoff,
    /// Retry-After headers, and retryable error classification.
    async fn api_call(
        &self,
        method: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<Value> {
        // We need to be able to rebuild the request on retry. Since RequestBuilder
        // is not Clone, we do the retry loop at the caller level — but the response
        // handling is shared. For the first attempt we use the provided builder;
        // callers pass a closure for rebuilding on retry.
        //
        // Actually, reqwest::RequestBuilder can be built into a Request that is cloneable
        // if the body is cloneable. Let's use try_clone on the built request.
        let built = request.build()?;
        let mut last_err = None;

        for attempt in 0..3u32 {
            let req = built.try_clone().ok_or_else(|| anyhow!("Cannot retry {method}: request body not cloneable"))?;

            let resp = match self.http.execute(req).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(attempt = attempt + 1, method = %method, error = %e, "Slack API call failed, retrying");
                    last_err = Some(anyhow::Error::from(e));
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                    continue;
                }
            };

            let retry_after_header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            let result: Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    warn!(attempt = attempt + 1, method = %method, error = %e, "Slack API response parse failed, retrying");
                    last_err = Some(anyhow::Error::from(e));
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                    continue;
                }
            };

            if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                return Ok(result);
            }

            let err = result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");

            if matches!(err, "ratelimited" | "internal_error" | "service_unavailable") {
                let retry_after = retry_after_header.unwrap_or(2u64.pow(attempt));
                warn!(attempt = attempt + 1, method = %method, err = %err, "retrying after {retry_after}s");
                last_err = Some(anyhow!("{method} failed: {err}"));
                tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;
                continue;
            }

            return Err(anyhow!("{method} failed: {err}"));
        }

        Err(last_err.unwrap_or_else(|| anyhow!("{method} failed after retries")))
    }

    async fn api_post(&self, method: &str, body: &Value) -> Result<Value> {
        let url = format!("https://slack.com/api/{method}");
        let request = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(body);
        self.api_call(method, request).await
    }

    async fn api_get(&self, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = format!("https://slack.com/api/{method}");
        let request = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .query(params);
        self.api_call(method, request).await
    }

    /// Extract SentMessage from a Slack chat.postMessage / chat.update response.
    fn parse_sent(result: &Value) -> SentMessage {
        SentMessage {
            channel: result["channel"].as_str().unwrap_or("").to_string(),
            timestamp: result["ts"]
                .as_str()
                .or_else(|| result["message"]["ts"].as_str())
                .unwrap_or("")
                .to_string(),
        }
    }

    /// Convert a Slack message JSON object to a ChatMessage.
    fn to_chat_message(msg: &Value) -> ChatMessage {
        ChatMessage {
            user_id: msg["user"].as_str().unwrap_or("unknown").to_string(),
            text: msg["text"].as_str().unwrap_or("").to_string(),
            timestamp: msg["ts"].as_str().unwrap_or("").to_string(),
            raw: Some(msg.clone()),
        }
    }
}

// ── Messenger trait implementation ─────────────────────────────────────

#[async_trait]
impl Messenger for SlackSocket {
    async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SentMessage> {
        let mut body = serde_json::json!({
            "channel": channel,
            "text": text,
        });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = serde_json::json!(ts);
        }
        let result = self.api_post("chat.postMessage", &body).await?;
        Ok(Self::parse_sent(&result))
    }

    async fn send_dm(&self, user_id: &str, text: &str) -> Result<SentMessage> {
        let open_body = serde_json::json!({"users": user_id});
        let open_result = self.api_post("conversations.open", &open_body).await;

        let body = match open_result {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("missing_scope") {
                    error!("conversations.open requires im:write scope — add it at api.slack.com/apps");
                }
                return Err(e);
            }
        };

        let dm_channel = body["channel"]["id"]
            .as_str()
            .ok_or_else(|| anyhow!("No channel ID in conversations.open response"))?;

        self.post_message(dm_channel, text, None).await
    }

    async fn add_reaction(
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
        match self.api_post("reactions.add", &body).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("already_reacted") {
                    warn!("reactions.add failed: {e}");
                }
                Ok(())
            }
        }
    }

    async fn update_message(
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
        self.api_post("chat.update", &body).await?;
        Ok(())
    }

    async fn delete_message(&self, channel: &str, timestamp: &str) -> Result<()> {
        let body = serde_json::json!({
            "channel": channel,
            "ts": timestamp,
        });
        match self.api_post("chat.delete", &body).await {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("chat.delete failed: {e}");
                Ok(())
            }
        }
    }

    async fn get_thread(
        &self,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Vec<ChatMessage>> {
        let result = self
            .api_get(
                "conversations.replies",
                &[("channel", channel), ("ts", thread_ts), ("limit", "20")],
            )
            .await;
        match result {
            Ok(body) => {
                let messages = body["messages"].as_array().cloned().unwrap_or_default();
                Ok(messages.iter().map(Self::to_chat_message).collect())
            }
            Err(e) => {
                warn!("conversations.replies failed: {e}");
                Ok(Vec::new())
            }
        }
    }

    async fn get_channel_history(
        &self,
        channel: &str,
        limit: u32,
    ) -> Result<Vec<ChatMessage>> {
        let limit_str = limit.to_string();
        let result = self
            .api_get(
                "conversations.history",
                &[("channel", channel), ("limit", &limit_str)],
            )
            .await;
        match result {
            Ok(body) => {
                let messages = body["messages"].as_array().cloned().unwrap_or_default();
                Ok(messages.iter().map(Self::to_chat_message).collect())
            }
            Err(e) => {
                warn!("conversations.history failed: {e}");
                Ok(Vec::new())
            }
        }
    }

    async fn get_user_name(&self, user_id: &str) -> String {
        // Check cache first
        {
            let cache = self.user_cache.lock().await;
            if let Some(name) = cache.get(user_id) {
                return name.clone();
            }
        }

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

        {
            let mut cache = self.user_cache.lock().await;
            cache.insert(user_id.to_string(), name.clone());
        }

        name
    }

    async fn get_channel_name(&self, channel_id: &str) -> String {
        {
            let cache = self.channel_cache.lock().await;
            if let Some(name) = cache.get(channel_id) {
                return name.clone();
            }
        }

        let resp = self
            .http
            .get("https://slack.com/api/conversations.info")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .query(&[("channel", channel_id)])
            .send()
            .await;

        let name = match resp {
            Ok(r) => {
                let body: Value = r.json().await.unwrap_or_default();
                if body["ok"].as_bool() == Some(true) {
                    body["channel"]["name"]
                        .as_str()
                        .unwrap_or(channel_id)
                        .to_string()
                } else {
                    channel_id.to_string()
                }
            }
            Err(_) => channel_id.to_string(),
        };

        {
            let mut cache = self.channel_cache.lock().await;
            cache.insert(channel_id.to_string(), name.clone());
        }

        name
    }

    async fn resolve_channel_id(&self, channel_name: &str) -> Option<String> {
        {
            let cache = self.channel_cache.lock().await;
            for (id, name) in cache.iter() {
                if name == channel_name {
                    return Some(id.clone());
                }
            }
        }

        let resp = self
            .http
            .get("https://slack.com/api/conversations.list")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .query(&[("types", "public_channel,private_channel"), ("limit", "200")])
            .send()
            .await
            .ok()?;

        let body: Value = resp.json().await.ok()?;
        if body["ok"].as_bool() != Some(true) {
            return None;
        }

        if let Some(channels) = body["channels"].as_array() {
            let mut cache = self.channel_cache.lock().await;
            for ch in channels {
                let id = ch["id"].as_str().unwrap_or("").to_string();
                let name = ch["name"].as_str().unwrap_or("").to_string();
                if !id.is_empty() && !name.is_empty() {
                    cache.insert(id.clone(), name.clone());
                    if name == channel_name {
                        return Some(id);
                    }
                }
            }
        }

        None
    }

    async fn find_user_by_name(&self, query: &str) -> Result<Vec<(String, String)>> {
        let resp = self
            .http
            .get("https://slack.com/api/users.list")
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .send()
            .await?;

        let body: Value = resp.json().await?;
        if body["ok"].as_bool() != Some(true) {
            let err = body["error"].as_str().unwrap_or("unknown error");
            return Err(anyhow!("users.list failed: {err}"));
        }

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        if let Some(members) = body["members"].as_array() {
            let mut cache = self.user_cache.lock().await;
            for member in members {
                if member["deleted"].as_bool() == Some(true)
                    || member["is_bot"].as_bool() == Some(true)
                {
                    continue;
                }

                let id = member["id"].as_str().unwrap_or("").to_string();
                let display_name = member["profile"]["display_name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| member["profile"]["real_name"].as_str())
                    .unwrap_or("")
                    .to_string();
                let real_name = member["real_name"].as_str().unwrap_or("");
                let username = member["name"].as_str().unwrap_or("");

                if !id.is_empty() && !display_name.is_empty() {
                    cache.insert(id.clone(), display_name.clone());
                }

                if display_name.to_lowercase().contains(&query_lower)
                    || real_name.to_lowercase().contains(&query_lower)
                    || username.to_lowercase().contains(&query_lower)
                {
                    matches.push((id, display_name));
                }
            }
        }

        Ok(matches)
    }
}

// ── Transport trait implementation ─────────────────────────────────────

#[async_trait]
impl Transport for SlackSocket {
    fn bot_user_id(&self) -> &str {
        &self.bot_user_id
    }

    fn is_mention(&self, text: &str) -> bool {
        text.contains(&format!("<@{}>", self.bot_user_id))
    }

    fn strip_mentions(&self, text: &str) -> String {
        text.replace(&format!("<@{}>", self.bot_user_id), "@Delegate")
            .trim()
            .to_string()
    }

    fn is_dm_channel(&self, channel_id: &str) -> bool {
        channel_id.starts_with('D')
    }

    fn normalize_event(&self, envelope: &Value) -> Option<DelegateEvent> {
        let event = envelope
            .get("payload")
            .and_then(|p| p.get("event"))
            .or_else(|| envelope.get("event"))?;
        let event_type = event.get("type")?.as_str()?.to_string();

        match event_type.as_str() {
            "message" | "app_mention" => self.normalize_message(event, &event_type),
            "reaction_added" => Self::normalize_reaction(event),
            _ => None,
        }
    }

    fn is_valid_user_id(&self, id: &str) -> bool {
        (id.starts_with('U') || id.starts_with('W'))
            && id.len() > 1
            && id.chars().skip(1).all(|c| c.is_alphanumeric())
    }

    async fn listen(&self, tx: mpsc::Sender<Value>) -> Result<()> {
        self.run(tx).await
    }
}

// ── Slack-specific normalization helpers ───────────────────────────────

impl SlackSocket {
    fn normalize_message(&self, event: &Value, event_type: &str) -> Option<DelegateEvent> {
        // Skip message subtypes that are noise
        if let Some(subtype) = event.get("subtype").and_then(|s| s.as_str()) {
            match subtype {
                "message_changed" | "message_deleted" | "channel_join" | "channel_leave"
                | "bot_message" | "bot_add" | "bot_remove" => {
                    return None
                }
                _ => {}
            }
        }

        let channel: ChannelId = event.get("channel")?.as_str()?.into();
        let user: UserId = event
            .get("user")
            .and_then(|u| u.as_str())
            .unwrap_or("unknown")
            .into();
        let text = event
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let ts: MessageTs = event
            .get("ts")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .into();
        let thread_ts: Option<MessageTs> = event
            .get("thread_ts")
            .and_then(|t| t.as_str())
            .map(|s| s.into());

        Some(DelegateEvent {
            id: ts.0.clone(),
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
        let user: UserId = event.get("user")?.as_str()?.into();
        let reaction = event
            .get("reaction")
            .and_then(|r| r.as_str())
            .unwrap_or("");
        let item = event.get("item")?;
        let channel: ChannelId = item
            .get("channel")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .into();
        let ts: MessageTs = item
            .get("ts")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .into();

        Some(DelegateEvent {
            id: format!("reaction-{}", ts),
            event_type: "reaction_added".to_string(),
            channel,
            user,
            content: format!(":{reaction}:"),
            timestamp: ts,
            thread_ts: None,
            raw: event.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transport() -> SlackSocket {
        SlackSocket::new(
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            "UBOT123".to_string(),
        )
    }

    #[test]
    fn is_mention_detects_bot_mention() {
        let t = make_transport();
        assert!(t.is_mention("hey <@UBOT123> what's up?"));
        assert!(!t.is_mention("hey <@UOTHER> what's up?"));
    }

    #[test]
    fn strip_mentions_replaces_bot_id() {
        let t = make_transport();
        assert_eq!(t.strip_mentions("<@UBOT123> status?"), "@Delegate status?");
    }

    #[test]
    fn is_dm_channel_detects_d_prefix() {
        let t = make_transport();
        assert!(t.is_dm_channel("D0123456789"));
        assert!(!t.is_dm_channel("C0123456789"));
    }

    #[test]
    fn is_valid_user_id_checks_prefix() {
        let t = make_transport();
        assert!(t.is_valid_user_id("U012345"));
        assert!(t.is_valid_user_id("W012345"));
        assert!(!t.is_valid_user_id("josh"));
        assert!(!t.is_valid_user_id(""));
        assert!(!t.is_valid_user_id("U"));
    }

    #[test]
    fn normalize_message_event() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "message",
                    "channel": "C123",
                    "user": "U456",
                    "text": "hello world",
                    "ts": "1234567890.123456"
                }
            }
        });
        let event = t.normalize_event(&envelope).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.channel.as_str(), "C123");
        assert_eq!(event.user.as_str(), "U456");
        assert_eq!(event.content, "hello world");
    }

    #[test]
    fn normalize_app_mention() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "app_mention",
                    "channel": "C123",
                    "user": "U789",
                    "text": "<@UBOT> status?",
                    "ts": "1234567890.000"
                }
            }
        });
        let event = t.normalize_event(&envelope).unwrap();
        assert_eq!(event.event_type, "app_mention");
    }

    #[test]
    fn normalize_skips_bot_messages() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "message",
                    "subtype": "bot_message",
                    "channel": "C123",
                    "text": "CI build passed",
                    "ts": "1234567890.000"
                }
            }
        });
        assert!(t.normalize_event(&envelope).is_none());
    }

    #[test]
    fn normalize_skips_message_changed() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "message",
                    "subtype": "message_changed",
                    "channel": "C123",
                    "ts": "1234567890.000"
                }
            }
        });
        assert!(t.normalize_event(&envelope).is_none());
    }

    #[test]
    fn normalize_reaction_event() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "reaction_added",
                    "user": "U789",
                    "reaction": "thumbsup",
                    "item": {
                        "channel": "C123",
                        "ts": "1234567890.000"
                    }
                }
            }
        });
        let event = t.normalize_event(&envelope).unwrap();
        assert_eq!(event.event_type, "reaction_added");
        assert_eq!(event.content, ":thumbsup:");
    }

    #[test]
    fn normalize_preserves_thread_ts() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "message",
                    "channel": "C123",
                    "user": "U456",
                    "text": "threaded reply",
                    "ts": "9999.000",
                    "thread_ts": "1111.000"
                }
            }
        });
        let event = t.normalize_event(&envelope).unwrap();
        assert_eq!(event.thread_ts.as_ref().map(|ts| ts.as_str()), Some("1111.000"));
    }

    #[test]
    fn normalize_unknown_type_returns_none() {
        let t = make_transport();
        let envelope = serde_json::json!({
            "payload": {
                "event": {
                    "type": "channel_created",
                    "channel": {"id": "C123"}
                }
            }
        });
        assert!(t.normalize_event(&envelope).is_none());
    }
}
