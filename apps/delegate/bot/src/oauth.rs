use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

use crate::dynamic_registry::DynamicRegistry;

// ── Data structures ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredential {
    pub provider: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub extra: HashMap<String, String>,
    pub connected_at: String,
    pub connected_by: String,
}

#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub extra_auth_params: HashMap<String, String>,
}

pub struct CredentialStore {
    credentials_dir: PathBuf,
    cache: RwLock<HashMap<String, OAuthCredential>>,
    providers: HashMap<String, OAuthProviderConfig>,
    refresh_locks: HashMap<String, Mutex<()>>,
}

// ── CSRF state ──

#[derive(Debug, Serialize, Deserialize)]
struct CsrfState {
    provider: String,
    slack_user: String,
    created_at: String,
}

// ── CredentialStore implementation ──

impl CredentialStore {
    pub fn new(workspace_path: &Path, providers: HashMap<String, OAuthProviderConfig>) -> Self {
        let credentials_dir = workspace_path.join("credentials");
        let refresh_locks: HashMap<String, Mutex<()>> = providers
            .keys()
            .map(|k| (k.clone(), Mutex::new(())))
            .collect();
        Self {
            credentials_dir,
            cache: RwLock::new(HashMap::new()),
            providers,
            refresh_locks,
        }
    }

    pub async fn load_all(&self) {
        let _ = tokio::fs::create_dir_all(&self.credentials_dir).await;
        let mut entries = match tokio::fs::read_dir(&self.credentials_dir).await {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut cache = self.cache.write().await;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // Skip CSRF state files
            if name.starts_with('_') {
                continue;
            }
            if let Ok(data) = tokio::fs::read_to_string(&path).await {
                if let Ok(cred) = serde_json::from_str::<OAuthCredential>(&data) {
                    info!(provider = %cred.provider, "Loaded credential");
                    cache.insert(cred.provider.clone(), cred);
                }
            }
        }
    }

    /// Atomic write: write to .tmp then rename.
    pub async fn store(&self, cred: &OAuthCredential) -> Result<()> {
        let _ = tokio::fs::create_dir_all(&self.credentials_dir).await;
        let path = self.credentials_dir.join(format!("{}.json", cred.provider));
        let tmp = self
            .credentials_dir
            .join(format!("{}.json.tmp", cred.provider));
        let json = serde_json::to_string_pretty(cred)?;
        tokio::fs::write(&tmp, &json).await?;
        tokio::fs::rename(&tmp, &path).await?;
        self.cache
            .write()
            .await
            .insert(cred.provider.clone(), cred.clone());
        info!(provider = %cred.provider, "Stored credential");
        Ok(())
    }

    /// Get a valid access token, auto-refreshing if within 5 minutes of expiry.
    pub async fn get_token(&self, provider: &str) -> Option<String> {
        let cred = self.cache.read().await.get(provider)?.clone();

        // Check if refresh needed
        if let Some(ref expires_at) = cred.expires_at {
            if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                let now = chrono::Utc::now();
                let buffer = chrono::Duration::minutes(5);
                if now + buffer >= expiry {
                    // Need refresh
                    if let Err(e) = self.refresh_token(provider).await {
                        warn!(provider, error = %e, "Token refresh failed, returning stale token");
                    }
                    // Return whatever we have now (may be refreshed)
                    return self
                        .cache
                        .read()
                        .await
                        .get(provider)
                        .map(|c| c.access_token.clone());
                }
            }
        }

        Some(cred.access_token)
    }

    pub async fn is_connected(&self, provider: &str) -> bool {
        self.cache.read().await.contains_key(provider)
    }

    pub async fn connected_providers(&self) -> HashSet<String> {
        self.cache.read().await.keys().cloned().collect()
    }

    /// Map skill env var names to credential values.
    ///
    /// For Authorization headers, returns the full header value (e.g. "Bearer <token>").
    /// For base URLs, constructs from cloud_id.
    /// Falls back to env vars with backward-compat formatting:
    ///   - JIRA_AUTHORIZATION: OAuth → "Bearer <token>", env var fallback → "Basic <JIRA_AUTH>"
    ///   - CONFLUENCE_AUTHORIZATION: same pattern
    pub async fn resolve_env_var(&self, var_name: &str) -> Option<String> {
        let cache = self.cache.read().await;
        match var_name {
            // Full Authorization header values (OAuth = Bearer, env var fallback = Basic)
            "JIRA_AUTHORIZATION" => {
                if let Some(c) = cache.get("atlassian") {
                    Some(format!("Bearer {}", c.access_token))
                } else {
                    // Backward compat: old JIRA_AUTH env var with Basic prefix
                    std::env::var("JIRA_AUTH").ok().map(|v| format!("Basic {v}"))
                }
            }
            "CONFLUENCE_AUTHORIZATION" => {
                if let Some(c) = cache.get("atlassian") {
                    Some(format!("Bearer {}", c.access_token))
                } else {
                    std::env::var("CONFLUENCE_AUTH").ok().map(|v| format!("Basic {v}"))
                }
            }
            "JIRA_BASE_URL" => cache.get("atlassian").and_then(|c| {
                c.extra
                    .get("cloud_id")
                    .map(|id| format!("https://api.atlassian.com/ex/jira/{id}"))
            }),
            "CONFLUENCE_BASE_URL" => cache.get("atlassian").and_then(|c| {
                c.extra
                    .get("cloud_id")
                    .map(|id| format!("https://api.atlassian.com/ex/confluence/{id}"))
            }),
            "LINEAR_API_KEY" => cache.get("linear").map(|c| c.access_token.clone()),
            "NOTION_API_KEY" => cache.get("notion").map(|c| c.access_token.clone()),
            "GOOGLE_ACCESS_TOKEN" => {
                drop(cache); // Release read lock before potentially refreshing
                self.get_token("google").await
            }
            "GITHUB_TOKEN" => cache.get("github").map(|c| c.access_token.clone()),
            "FIGMA_ACCESS_TOKEN" => cache.get("figma").map(|c| c.access_token.clone()),
            "GONG_AUTHORIZATION" => {
                if let Some(c) = cache.get("gong") {
                    Some(format!("Bearer {}", c.access_token))
                } else if let (Ok(key), Ok(secret)) = (
                    std::env::var("GONG_ACCESS_KEY"),
                    std::env::var("GONG_ACCESS_KEY_SECRET"),
                ) {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD
                        .encode(format!("{key}:{secret}"));
                    Some(format!("Basic {encoded}"))
                } else {
                    None
                }
            }
            // Base URL defaults — env var override takes priority via substitute_template fallback
            "LINEAR_BASE_URL" => Some(std::env::var("LINEAR_BASE_URL")
                .unwrap_or_else(|_| "https://api.linear.app".to_string())),
            "NOTION_BASE_URL" => Some(std::env::var("NOTION_BASE_URL")
                .unwrap_or_else(|_| "https://api.notion.com".to_string())),
            "GITHUB_BASE_URL" => Some(std::env::var("GITHUB_BASE_URL")
                .unwrap_or_else(|_| "https://api.github.com".to_string())),
            "FIGMA_BASE_URL" => Some(std::env::var("FIGMA_BASE_URL")
                .unwrap_or_else(|_| "https://api.figma.com".to_string())),
            "GONG_BASE_URL" => Some(std::env::var("GONG_BASE_URL")
                .unwrap_or_else(|_| "https://api.gong.io".to_string())),
            "GOOGLE_BASE_URL" => Some(std::env::var("GOOGLE_BASE_URL")
                .unwrap_or_else(|_| "https://www.googleapis.com".to_string())),
            _ => None,
        }
    }

    /// Return the set of configured provider names (have client_id/secret).
    pub fn configured_providers(&self) -> HashSet<String> {
        self.providers.keys().cloned().collect()
    }

    /// Refresh an OAuth token using the provider's refresh token flow.
    async fn refresh_token(&self, provider: &str) -> Result<()> {
        let lock = self
            .refresh_locks
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {provider}"))?;
        let _guard = lock.lock().await;

        // Re-check after acquiring lock (another task may have refreshed)
        let cred = self
            .cache
            .read()
            .await
            .get(provider)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No credential for {provider}"))?;

        if let Some(ref expires_at) = cred.expires_at {
            if let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                let now = chrono::Utc::now();
                let buffer = chrono::Duration::minutes(5);
                if now + buffer < expiry {
                    return Ok(()); // Already refreshed by another task
                }
            }
        }

        let refresh_token = cred
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No refresh token for {provider}"))?;

        let config = self
            .providers
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("No provider config for {provider}"))?;

        let client = reqwest::Client::new();

        let (new_access, new_refresh, expires_in) = match provider {
            "atlassian" => {
                let resp = client
                    .post(&config.token_url)
                    .json(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "client_id": config.client_id,
                        "client_secret": config.client_secret,
                        "refresh_token": refresh_token,
                    }))
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<serde_json::Value>()
                    .await?;
                (
                    resp["access_token"].as_str().unwrap_or("").to_string(),
                    resp["refresh_token"].as_str().map(|s| s.to_string()),
                    resp["expires_in"].as_u64().unwrap_or(3600),
                )
            }
            "google" => {
                let resp = client
                    .post(&config.token_url)
                    .form(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", &config.client_id),
                        ("client_secret", &config.client_secret),
                        ("refresh_token", refresh_token),
                    ])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<serde_json::Value>()
                    .await?;
                (
                    resp["access_token"].as_str().unwrap_or("").to_string(),
                    // Google doesn't return a new refresh token on refresh
                    None,
                    resp["expires_in"].as_u64().unwrap_or(3600),
                )
            }
            "linear" => {
                let resp = client
                    .post(&config.token_url)
                    .form(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", &config.client_id),
                        ("client_secret", &config.client_secret),
                        ("refresh_token", refresh_token),
                    ])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<serde_json::Value>()
                    .await?;
                (
                    resp["access_token"].as_str().unwrap_or("").to_string(),
                    resp["refresh_token"].as_str().map(|s| s.to_string()),
                    resp["expires_in"].as_u64().unwrap_or(86400),
                )
            }
            "notion" => {
                // Notion tokens don't expire — no refresh needed
                return Ok(());
            }
            _ => return Err(anyhow::anyhow!("Unknown provider: {provider}")),
        };

        if new_access.is_empty() {
            return Err(anyhow::anyhow!("Empty access token in refresh response"));
        }

        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
        let mut updated = cred.clone();
        updated.access_token = new_access;
        if let Some(rt) = new_refresh {
            updated.refresh_token = Some(rt);
        }
        updated.expires_at = Some(expires_at.to_rfc3339());

        self.store(&updated).await?;
        info!(provider, "Token refreshed successfully");
        Ok(())
    }

    /// Generate a CSRF state token and persist it.
    async fn create_csrf_state(&self, provider: &str, slack_user: &str) -> Result<String> {
        let state_id = uuid::Uuid::new_v4().to_string();
        let state = CsrfState {
            provider: provider.to_string(),
            slack_user: slack_user.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let path = self
            .credentials_dir
            .join(format!("_state_{state_id}.json"));
        let json = serde_json::to_string(&state)?;
        tokio::fs::write(&path, json).await?;
        Ok(state_id)
    }

    /// Validate and consume a CSRF state token.
    async fn validate_csrf_state(&self, state_id: &str) -> Option<CsrfState> {
        let path = self
            .credentials_dir
            .join(format!("_state_{state_id}.json"));
        let data = tokio::fs::read_to_string(&path).await.ok()?;
        let state: CsrfState = serde_json::from_str(&data).ok()?;
        // Delete the state file (one-time use)
        let _ = tokio::fs::remove_file(&path).await;
        Some(state)
    }
}

// ── Provider config loading ──

pub fn load_provider_configs() -> HashMap<String, OAuthProviderConfig> {
    let mut configs = HashMap::new();

    if let (Ok(id), Ok(secret)) = (
        std::env::var("ATLASSIAN_CLIENT_ID"),
        std::env::var("ATLASSIAN_CLIENT_SECRET"),
    ) {
        configs.insert(
            "atlassian".to_string(),
            OAuthProviderConfig {
                name: "atlassian".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://auth.atlassian.com/authorize".to_string(),
                token_url: "https://auth.atlassian.com/oauth/token".to_string(),
                scopes: vec![
                    "read:jira-work".to_string(),
                    "write:jira-work".to_string(),
                    "read:confluence-content.all".to_string(),
                    "write:confluence-content".to_string(),
                    "offline_access".to_string(),
                ],
                extra_auth_params: {
                    let mut m = HashMap::new();
                    m.insert("audience".to_string(), "api.atlassian.com".to_string());
                    m.insert("prompt".to_string(), "consent".to_string());
                    m
                },
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("LINEAR_CLIENT_ID"),
        std::env::var("LINEAR_CLIENT_SECRET"),
    ) {
        configs.insert(
            "linear".to_string(),
            OAuthProviderConfig {
                name: "linear".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://linear.app/oauth/authorize".to_string(),
                token_url: "https://api.linear.app/oauth/token".to_string(),
                scopes: vec!["read".to_string(), "write".to_string(), "issues:create".to_string()],
                extra_auth_params: HashMap::new(),
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("NOTION_CLIENT_ID"),
        std::env::var("NOTION_CLIENT_SECRET"),
    ) {
        configs.insert(
            "notion".to_string(),
            OAuthProviderConfig {
                name: "notion".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://api.notion.com/v1/oauth/authorize".to_string(),
                token_url: "https://api.notion.com/v1/oauth/token".to_string(),
                scopes: Vec::new(), // Notion uses owner-level permissions
                extra_auth_params: HashMap::new(),
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("GOOGLE_CLIENT_ID"),
        std::env::var("GOOGLE_CLIENT_SECRET"),
    ) {
        configs.insert(
            "google".to_string(),
            OAuthProviderConfig {
                name: "google".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scopes: vec![
                    "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                    "https://www.googleapis.com/auth/gmail.modify".to_string(),
                ],
                extra_auth_params: {
                    let mut m = HashMap::new();
                    m.insert("access_type".to_string(), "offline".to_string());
                    m.insert("prompt".to_string(), "consent".to_string());
                    m
                },
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("GITHUB_CLIENT_ID"),
        std::env::var("GITHUB_CLIENT_SECRET"),
    ) {
        configs.insert(
            "github".to_string(),
            OAuthProviderConfig {
                name: "github".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://github.com/login/oauth/authorize".to_string(),
                token_url: "https://github.com/login/oauth/access_token".to_string(),
                scopes: vec!["repo".to_string(), "read:org".to_string()],
                extra_auth_params: HashMap::new(),
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("FIGMA_CLIENT_ID"),
        std::env::var("FIGMA_CLIENT_SECRET"),
    ) {
        configs.insert(
            "figma".to_string(),
            OAuthProviderConfig {
                name: "figma".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://www.figma.com/oauth".to_string(),
                token_url: "https://api.figma.com/v1/oauth/token".to_string(),
                scopes: vec!["files:read".to_string(), "file_comments:write".to_string()],
                extra_auth_params: HashMap::new(),
            },
        );
    }

    if let (Ok(id), Ok(secret)) = (
        std::env::var("GONG_CLIENT_ID"),
        std::env::var("GONG_CLIENT_SECRET"),
    ) {
        configs.insert(
            "gong".to_string(),
            OAuthProviderConfig {
                name: "gong".to_string(),
                client_id: id,
                client_secret: secret,
                auth_url: "https://app.gong.io/oauth2/authorize".to_string(),
                token_url: "https://app.gong.io/oauth2/generate-customer-token".to_string(),
                scopes: vec![
                    "api:calls:read:extensive".to_string(),
                    "api:calls:read:transcript".to_string(),
                    "api:users:read".to_string(),
                ],
                extra_auth_params: HashMap::new(),
            },
        );
    }

    configs
}

// ── Axum routes ──

struct AppState {
    cred_store: Arc<CredentialStore>,
    callback_url: String,
    registry: Arc<DynamicRegistry>,
}

pub async fn serve(
    port: u16,
    cred_store: Arc<CredentialStore>,
    registry: Arc<DynamicRegistry>,
) {
    let callback_url =
        std::env::var("OAUTH_CALLBACK_URL").unwrap_or_else(|_| format!("http://localhost:{port}"));

    let state = Arc::new(AppState {
        cred_store,
        callback_url,
        registry,
    });

    let app = axum::Router::new()
        .route("/health", axum::routing::get(health))
        .route("/connect/{provider}", axum::routing::get(connect))
        .route("/oauth/callback/{provider}", axum::routing::get(oauth_callback))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind OAuth server on port {port}: {e}");
            return;
        }
    };

    info!(port, "OAuth server listening");
    if let Err(e) = axum::serve(listener, app).await {
        error!("OAuth server error: {e}");
    }
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
struct ConnectParams {
    user: Option<String>,
}

async fn connect(
    axum::extract::Path(provider): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<ConnectParams>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    let config = match state.cred_store.providers.get(&provider) {
        Some(c) => c,
        None => {
            return axum::response::Response::builder()
                .status(404)
                .body(axum::body::Body::from(format!(
                    "Unknown provider: {provider}. Available: {:?}",
                    state.cred_store.providers.keys().collect::<Vec<_>>()
                )))
                .unwrap();
        }
    };

    let slack_user = params.user.unwrap_or_else(|| "unknown".to_string());
    let csrf_state = match state
        .cred_store
        .create_csrf_state(&provider, &slack_user)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return axum::response::Response::builder()
                .status(500)
                .body(axum::body::Body::from(format!("Failed to create state: {e}")))
                .unwrap();
        }
    };

    let callback = format!("{}/oauth/callback/{}", state.callback_url, provider);
    let scopes = config.scopes.join(" ");

    let mut auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&state={}&scope={}",
        config.auth_url,
        urlencod(&config.client_id),
        urlencod(&callback),
        urlencod(&csrf_state),
        urlencod(&scopes),
    );

    for (key, val) in &config.extra_auth_params {
        auth_url.push_str(&format!("&{}={}", urlencod(key), urlencod(val)));
    }

    axum::response::Response::builder()
        .status(302)
        .header("Location", &auth_url)
        .body(axum::body::Body::empty())
        .unwrap()
}

#[derive(Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

async fn oauth_callback(
    axum::extract::Path(provider): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<CallbackParams>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    // Validate CSRF state
    let csrf = match state.cred_store.validate_csrf_state(&params.state).await {
        Some(s) => s,
        None => {
            return html_response(400, "Invalid or expired state parameter. Please try connecting again.");
        }
    };

    if csrf.provider != provider {
        return html_response(400, "Provider mismatch in state parameter.");
    }

    let config = match state.cred_store.providers.get(&provider) {
        Some(c) => c,
        None => return html_response(404, "Unknown provider."),
    };

    let callback_uri = format!("{}/oauth/callback/{}", state.callback_url, provider);
    let client = reqwest::Client::new();

    // Exchange code for tokens (provider-specific)
    let token_result = match provider.as_str() {
        "atlassian" => exchange_atlassian(&client, config, &params.code, &callback_uri).await,
        "linear" => exchange_linear(&client, config, &params.code, &callback_uri).await,
        "notion" => exchange_notion(&client, config, &params.code, &callback_uri).await,
        "google" => exchange_google(&client, config, &params.code, &callback_uri).await,
        _ => Err(anyhow::anyhow!("Unknown provider")),
    };

    match token_result {
        Ok(mut cred) => {
            cred.connected_by = csrf.slack_user;
            if let Err(e) = state.cred_store.store(&cred).await {
                error!(provider = %provider, error = %e, "Failed to store credential");
                return html_response(500, &format!("Failed to store credential: {e}"));
            }

            // Refresh the dynamic registry so new skills activate immediately
            let skills_dir = state.cred_store.credentials_dir.parent().unwrap().join("skills");
            let connected = state.cred_store.connected_providers().await;
            state.registry.refresh_with_filter(&skills_dir, Some(&connected)).await;
            info!(provider = %provider, "OAuth connected, registry refreshed");

            html_response(200, &format!(
                "<h2>Connected to {}!</h2><p>You can close this tab and return to Slack.</p>",
                provider
            ))
        }
        Err(e) => {
            error!(provider = %provider, error = %e, "OAuth token exchange failed");
            html_response(500, &format!("Authorization failed: {e}"))
        }
    }
}

// ── Provider-specific token exchange ──

async fn exchange_atlassian(
    client: &reqwest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthCredential> {
    let resp = client
        .post(&config.token_url)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": config.client_id,
            "client_secret": config.client_secret,
            "code": code,
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let access_token = resp["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token"))?
        .to_string();
    let refresh_token = resp["refresh_token"].as_str().map(|s| s.to_string());
    let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);

    // Get cloud_id from accessible resources
    let resources = client
        .get("https://api.atlassian.com/oauth/token/accessible-resources")
        .bearer_auth(&access_token)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let cloud_id = resources
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|r| r["id"].as_str())
        .unwrap_or("")
        .to_string();

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
    let mut extra = HashMap::new();
    extra.insert("cloud_id".to_string(), cloud_id);

    Ok(OAuthCredential {
        provider: "atlassian".to_string(),
        access_token,
        refresh_token,
        expires_at: Some(expires_at.to_rfc3339()),
        scopes: config.scopes.clone(),
        extra,
        connected_at: chrono::Utc::now().to_rfc3339(),
        connected_by: String::new(), // Set from CSRF state below
    })
}

async fn exchange_linear(
    client: &reqwest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthCredential> {
    let resp = client
        .post(&config.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let access_token = resp["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token"))?
        .to_string();
    let expires_in = resp["expires_in"].as_u64().unwrap_or(86400);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);

    Ok(OAuthCredential {
        provider: "linear".to_string(),
        access_token,
        refresh_token: resp["refresh_token"].as_str().map(|s| s.to_string()),
        expires_at: Some(expires_at.to_rfc3339()),
        scopes: config.scopes.clone(),
        extra: HashMap::new(),
        connected_at: chrono::Utc::now().to_rfc3339(),
        connected_by: String::new(),
    })
}

async fn exchange_notion(
    client: &reqwest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthCredential> {
    let resp = client
        .post(&config.token_url)
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let access_token = resp["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token"))?
        .to_string();

    Ok(OAuthCredential {
        provider: "notion".to_string(),
        access_token,
        refresh_token: None,
        expires_at: None, // Notion tokens don't expire
        scopes: Vec::new(),
        extra: HashMap::new(),
        connected_at: chrono::Utc::now().to_rfc3339(),
        connected_by: String::new(),
    })
}

async fn exchange_google(
    client: &reqwest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthCredential> {
    let resp = client
        .post(&config.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let access_token = resp["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token"))?
        .to_string();
    let refresh_token = resp["refresh_token"].as_str().map(|s| s.to_string());
    let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);

    Ok(OAuthCredential {
        provider: "google".to_string(),
        access_token,
        refresh_token,
        expires_at: Some(expires_at.to_rfc3339()),
        scopes: config.scopes.clone(),
        extra: HashMap::new(),
        connected_at: chrono::Utc::now().to_rfc3339(),
        connected_by: String::new(),
    })
}

// ── Helpers ──

/// Simple percent-encoding for URL query parameters.
fn urlencod(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn html_response(status: u16, body: &str) -> axum::response::Response {
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>Delegate OAuth</title>\
         <style>body{{font-family:system-ui;max-width:600px;margin:80px auto;text-align:center}}</style>\
         </head><body>{body}</body></html>"
    );
    axum::response::Response::builder()
        .status(status)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(axum::body::Body::from(html))
        .unwrap()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_roundtrip_serialization() {
        let cred = OAuthCredential {
            provider: "atlassian".to_string(),
            access_token: "test_token".to_string(),
            refresh_token: Some("refresh_123".to_string()),
            expires_at: Some("2026-03-06T12:00:00Z".to_string()),
            scopes: vec!["read:jira-work".to_string()],
            extra: {
                let mut m = HashMap::new();
                m.insert("cloud_id".to_string(), "abc-123".to_string());
                m
            },
            connected_at: "2026-03-06T10:00:00Z".to_string(),
            connected_by: "U12345".to_string(),
        };

        let json = serde_json::to_string(&cred).unwrap();
        let deserialized: OAuthCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider, "atlassian");
        assert_eq!(deserialized.access_token, "test_token");
        assert_eq!(deserialized.refresh_token.as_deref(), Some("refresh_123"));
        assert_eq!(deserialized.extra.get("cloud_id").unwrap(), "abc-123");
    }

    #[test]
    fn credential_minimal_serialization() {
        let cred = OAuthCredential {
            provider: "notion".to_string(),
            access_token: "ntn_token".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            extra: HashMap::new(),
            connected_at: "2026-03-06T10:00:00Z".to_string(),
            connected_by: "U12345".to_string(),
        };

        let json = serde_json::to_string(&cred).unwrap();
        // None fields should be skipped
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("expires_at"));

        let deserialized: OAuthCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider, "notion");
        assert!(deserialized.refresh_token.is_none());
    }

    #[tokio::test]
    async fn resolve_env_var_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path(), HashMap::new());

        // Store an Atlassian credential
        let cred = OAuthCredential {
            provider: "atlassian".to_string(),
            access_token: "atl_token_123".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            extra: {
                let mut m = HashMap::new();
                m.insert("cloud_id".to_string(), "cloud-xyz".to_string());
                m
            },
            connected_at: "2026-03-06T10:00:00Z".to_string(),
            connected_by: "U12345".to_string(),
        };
        store.store(&cred).await.unwrap();

        // JIRA_AUTHORIZATION should resolve to Bearer + access token
        assert_eq!(
            store.resolve_env_var("JIRA_AUTHORIZATION").await.as_deref(),
            Some("Bearer atl_token_123")
        );
        // JIRA_BASE_URL should use cloud_id
        assert_eq!(
            store.resolve_env_var("JIRA_BASE_URL").await.as_deref(),
            Some("https://api.atlassian.com/ex/jira/cloud-xyz")
        );
        // CONFLUENCE_AUTHORIZATION should resolve to Bearer + same token
        assert_eq!(
            store.resolve_env_var("CONFLUENCE_AUTHORIZATION").await.as_deref(),
            Some("Bearer atl_token_123")
        );
        // CONFLUENCE_BASE_URL should use cloud_id
        assert_eq!(
            store.resolve_env_var("CONFLUENCE_BASE_URL").await.as_deref(),
            Some("https://api.atlassian.com/ex/confluence/cloud-xyz")
        );
        // Unknown var returns None
        assert!(store.resolve_env_var("UNKNOWN_VAR").await.is_none());
    }

    #[tokio::test]
    async fn resolve_env_var_backward_compat_basic_auth() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path(), HashMap::new());
        // No OAuth credentials — should fall back to env var with Basic prefix
        std::env::set_var("JIRA_AUTH", "dXNlcjp0b2tlbg==");
        assert_eq!(
            store.resolve_env_var("JIRA_AUTHORIZATION").await.as_deref(),
            Some("Basic dXNlcjp0b2tlbg==")
        );
        std::env::remove_var("JIRA_AUTH");
    }

    #[tokio::test]
    async fn store_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path(), HashMap::new());

        let cred = OAuthCredential {
            provider: "linear".to_string(),
            access_token: "lin_token".to_string(),
            refresh_token: Some("lin_refresh".to_string()),
            expires_at: Some("2026-03-07T12:00:00Z".to_string()),
            scopes: vec!["read".to_string()],
            extra: HashMap::new(),
            connected_at: "2026-03-06T10:00:00Z".to_string(),
            connected_by: "U12345".to_string(),
        };
        store.store(&cred).await.unwrap();

        // Create a new store and load from disk
        let store2 = CredentialStore::new(dir.path(), HashMap::new());
        store2.load_all().await;

        assert!(store2.is_connected("linear").await);
        assert!(!store2.is_connected("google").await);

        let connected = store2.connected_providers().await;
        assert!(connected.contains("linear"));
        assert_eq!(connected.len(), 1);
    }

    #[test]
    fn urlencod_basic() {
        assert_eq!(urlencod("hello"), "hello");
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a=b&c=d"), "a%3Db%26c%3Dd");
    }

    #[test]
    fn load_provider_configs_returns_empty_without_env() {
        // Clear any test vars
        std::env::remove_var("ATLASSIAN_CLIENT_ID");
        std::env::remove_var("LINEAR_CLIENT_ID");
        std::env::remove_var("NOTION_CLIENT_ID");
        std::env::remove_var("GOOGLE_CLIENT_ID");

        let configs = load_provider_configs();
        // Should have no providers without env vars set
        assert!(!configs.contains_key("atlassian"));
        assert!(!configs.contains_key("linear"));
    }
}
