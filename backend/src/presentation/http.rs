use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use axum::{
    extract::State,
    http::header::HeaderMap,
    http::Method,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    application::{
        decision_service::DecisionService,
        errors::ApplicationError,
        ports::{InsightAnalytics, UpsertOutcome},
    },
    domain::models::{DecisionStatus, NewDecisionInput},
    infrastructure::integrations::{
        gong,
        gong_client::GongClient,
        insight_llm::{InsightLlmClient, LlmRuntimeSettings},
        jira_client::JiraClient,
    },
};

#[derive(Clone)]
struct AppState {
    signal_service: Arc<DecisionService>,
    jira_client: Option<JiraClient>,
    gong_client: Option<GongClient>,
    gong_ingest_key: Option<String>,
    llm_client: Arc<RwLock<Option<InsightLlmClient>>>,
    llm_settings: Arc<RwLock<LlmRuntimeSettings>>,
    llm_settings_file: Option<PathBuf>,
}

pub fn build_router(
    signal_service: Arc<DecisionService>,
    jira_client: Option<JiraClient>,
    gong_client: Option<GongClient>,
    gong_ingest_key: Option<String>,
    llm_client: Option<InsightLlmClient>,
) -> Router {
    let llm_settings_file = resolve_llm_settings_file();
    let (initial_client, initial_settings) =
        initialize_llm_state(llm_client, llm_settings_file.as_deref());
    let initial_analytics = initial_client
        .clone()
        .map(|client| Arc::new(client) as Arc<dyn InsightAnalytics>);
    signal_service.set_insight_analytics(initial_analytics);
    let state = AppState {
        signal_service,
        jira_client,
        gong_client,
        gong_ingest_key,
        llm_client: Arc::new(RwLock::new(initial_client)),
        llm_settings: Arc::new(RwLock::new(initial_settings)),
        llm_settings_file,
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/decisions", get(list_signals).post(create_signal))
        .route("/api/signals", get(list_signals).post(create_signal))
        .route("/api/signals/actions", post(apply_signal_action))
        .route("/api/dev/story/reset", post(reset_story_dataset))
        .route("/api/graph", get(get_graph_snapshot))
        .route("/api/insights", get(get_insights))
        .route(
            "/api/settings/llm",
            get(get_llm_settings).post(update_llm_settings),
        )
        .route("/api/settings/llm/models", post(list_llm_models))
        .route("/api/settings/llm/warmup", post(warmup_llm_model))
        .route("/api/integrations/jira/sync", post(sync_jira_issues))
        .route("/api/integrations/gong/sync", post(sync_gong_events))
        .route("/api/integrations/gong/webhook", post(ingest_gong_webhook))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_methods([Method::GET, Method::POST]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct JiraSyncRequest {
    jql: Option<String>,
    limit: Option<usize>,
    default_owner: Option<String>,
}

#[derive(Debug, Serialize)]
struct JiraSyncResponse {
    source: String,
    fetched: usize,
    created: usize,
    updated: usize,
    skipped: usize,
}

#[derive(Debug, Serialize)]
struct GongWebhookResponse {
    source: String,
    received: usize,
    created: usize,
    skipped: usize,
}

#[derive(Debug, Deserialize)]
struct GongSyncRequest {
    limit: Option<usize>,
    default_owner: Option<String>,
}

#[derive(Debug, Serialize)]
struct GongSyncResponse {
    source: String,
    fetched: usize,
    created: usize,
    updated: usize,
    skipped: usize,
}

#[derive(Debug, Deserialize)]
struct SignalActionRequest {
    action: String,
    signal_ids: Vec<uuid::Uuid>,
    owner: Option<String>,
    status: Option<String>,
    tag: Option<String>,
    only_if_owner_missing: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SignalActionResponse {
    action: String,
    updated: usize,
}

#[derive(Debug, Serialize)]
struct StoryDatasetResponse {
    loaded: usize,
    message: String,
}

#[derive(Debug, Deserialize)]
struct LlmModelListRequest {
    settings: Option<LlmRuntimeSettings>,
}

#[derive(Debug, Deserialize)]
struct LlmWarmupRequest {
    settings: Option<LlmRuntimeSettings>,
}

#[derive(Debug, Serialize)]
struct LlmModelListResponse {
    provider: String,
    models: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LlmWarmupResponse {
    provider: String,
    model: String,
    message: String,
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_signals(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let signals = state.signal_service.list_decisions().await?;
    Ok(Json(signals))
}

async fn create_signal(
    State(state): State<AppState>,
    Json(payload): Json<NewDecisionInput>,
) -> Result<impl IntoResponse, ApiError> {
    let created = state.signal_service.create_decision(payload).await?;
    Ok(Json(created))
}

async fn get_graph_snapshot(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let graph = state.signal_service.get_graph_snapshot().await?;
    Ok(Json(graph))
}

async fn get_insights(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let insights = state.signal_service.get_insights().await?;
    Ok(Json(insights))
}

async fn get_llm_settings(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let settings = read_llm_settings(&state)?;
    Ok(Json(settings))
}

async fn update_llm_settings(
    State(state): State<AppState>,
    Json(payload): Json<LlmRuntimeSettings>,
) -> Result<impl IntoResponse, ApiError> {
    let client = InsightLlmClient::from_settings(payload.clone())
        .map_err(|error| ApiError(ApplicationError::Validation(error.to_string())))?;

    let stored_settings = client
        .as_ref()
        .map(InsightLlmClient::settings)
        .unwrap_or(payload);

    {
        let mut guard = state.llm_settings.write().map_err(|_| {
            ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to update llm settings due to poisoned lock"
            )))
        })?;
        *guard = stored_settings;
    }

    {
        let mut guard = state.llm_client.write().map_err(|_| {
            ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to update llm client due to poisoned lock"
            )))
        })?;
        *guard = client.clone();
    }

    let analytics = client.map(|value| Arc::new(value) as Arc<dyn InsightAnalytics>);
    state.signal_service.set_insight_analytics(analytics);

    let settings = read_llm_settings(&state)?;
    persist_llm_settings_if_configured(&state, &settings)?;
    Ok(Json(settings))
}

async fn list_llm_models(
    State(state): State<AppState>,
    Json(payload): Json<LlmModelListRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client = resolve_llm_client(&state, payload.settings)?;
    let provider = client.settings().provider;
    let models = client.list_models().await.map_err(|error| {
        ApiError(ApplicationError::Unavailable(format!(
            "failed to fetch models from llm provider: {error}"
        )))
    })?;
    Ok(Json(LlmModelListResponse { provider, models }))
}

async fn warmup_llm_model(
    State(state): State<AppState>,
    Json(payload): Json<LlmWarmupRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client = resolve_llm_client(&state, payload.settings)?;
    let settings = client.settings();
    let message = client.warmup().await.map_err(|error| {
        ApiError(ApplicationError::Unavailable(format!(
            "failed to warm up llm model: {error}"
        )))
    })?;
    Ok(Json(LlmWarmupResponse {
        provider: settings.provider,
        model: settings.model,
        message,
    }))
}

async fn sync_jira_issues(
    State(state): State<AppState>,
    Json(payload): Json<JiraSyncRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client = state.jira_client.as_ref().ok_or_else(|| {
        ApplicationError::Unavailable(
            "jira integration not configured; set JIRA_BASE_URL, JIRA_USER_EMAIL, and JIRA_API_TOKEN"
                .to_string(),
        )
    })?;

    let jql = payload
        .jql
        .unwrap_or_else(|| "project is not EMPTY ORDER BY updated DESC".to_string());
    if jql.trim().is_empty() {
        return Err(ApiError(ApplicationError::Validation(
            "jql must not be empty".to_string(),
        )));
    }

    let limit = payload.limit.unwrap_or(250).clamp(1, 1000);
    let default_owner = normalize_optional_text(payload.default_owner);

    let issues = client
        .search_issues(jql.trim(), limit)
        .await
        .map_err(|error| {
            ApiError(ApplicationError::Unexpected(
                error.context("jira sync request failed"),
            ))
        })?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let skipped = 0usize;

    for issue in &issues {
        let input = map_jira_issue_to_signal(issue, default_owner.clone());
        match state.signal_service.upsert_decision_by_title(input).await? {
            UpsertOutcome::Created => created += 1,
            UpsertOutcome::Updated => updated += 1,
        }
    }

    Ok(Json(JiraSyncResponse {
        source: "jira".to_string(),
        fetched: issues.len(),
        created,
        updated,
        skipped,
    }))
}

async fn sync_gong_events(
    State(state): State<AppState>,
    Json(payload): Json<GongSyncRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client = state.gong_client.as_ref().ok_or_else(|| {
        ApplicationError::Unavailable(
            "gong pull integration not configured; set GONG_EVENTS_URL or GONG_BASE_URL"
                .to_string(),
        )
    })?;

    let limit = payload.limit.unwrap_or(500).clamp(1, 5000);
    let default_owner = normalize_optional_text(payload.default_owner);

    let events = client.fetch_events(limit).await.map_err(|error| {
        let error_text = format!("{error:#}");
        if error_text.contains("failed to lookup address information")
            || error_text.contains("dns error")
        {
            return ApiError(ApplicationError::Unavailable(
                "gong sync could not resolve host. If backend runs on host machine, use GONG_EVENTS_URL=http://localhost:18082/events. If backend runs in docker compose, use http://gong-mock:8080/events or http://signalops-gong-mock:8080/events.".to_string(),
            ));
        }

        ApiError(ApplicationError::Unexpected(
            error.context("gong sync request failed"),
        ))
    })?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let skipped = 0usize;

    for event in &events {
        let input = map_gong_event_to_signal(event, default_owner.clone());
        match state.signal_service.upsert_decision_by_title(input).await? {
            UpsertOutcome::Created => created += 1,
            UpsertOutcome::Updated => updated += 1,
        }
    }

    Ok(Json(GongSyncResponse {
        source: "gong".to_string(),
        fetched: events.len(),
        created,
        updated,
        skipped,
    }))
}

async fn apply_signal_action(
    State(state): State<AppState>,
    Json(payload): Json<SignalActionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let action = payload.action.trim().to_ascii_lowercase();
    if payload.signal_ids.is_empty() {
        return Err(ApiError(ApplicationError::Validation(
            "signal_ids must include at least one id".to_string(),
        )));
    }

    let updated = match action.as_str() {
        "assign_owner" => {
            let owner = normalize_optional_text(payload.owner).ok_or_else(|| {
                ApiError(ApplicationError::Validation(
                    "owner is required for assign_owner".to_string(),
                ))
            })?;
            state
                .signal_service
                .bulk_assign_owner(
                    payload.signal_ids,
                    owner,
                    payload.only_if_owner_missing.unwrap_or(false),
                )
                .await?
        }
        "set_status" => {
            let status_text = normalize_optional_text(payload.status).ok_or_else(|| {
                ApiError(ApplicationError::Validation(
                    "status is required for set_status".to_string(),
                ))
            })?;
            let status = parse_signal_status(&status_text)?;
            state
                .signal_service
                .bulk_set_status(payload.signal_ids, status)
                .await?
        }
        "add_tag" => {
            let tag = normalize_optional_text(payload.tag).ok_or_else(|| {
                ApiError(ApplicationError::Validation(
                    "tag is required for add_tag".to_string(),
                ))
            })?;
            state
                .signal_service
                .bulk_add_tag(payload.signal_ids, tag)
                .await?
        }
        _ => {
            return Err(ApiError(ApplicationError::Validation(format!(
                "unsupported action '{}' (expected assign_owner, set_status, or add_tag)",
                payload.action
            ))));
        }
    };

    Ok(Json(SignalActionResponse { action, updated }))
}

async fn reset_story_dataset(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let loaded = state.signal_service.load_story_dataset().await?;
    Ok(Json(StoryDatasetResponse {
        loaded,
        message: "story dataset loaded; timeline now reflects curated customer narratives"
            .to_string(),
    }))
}

async fn ingest_gong_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    validate_gong_ingest_key(&state, &headers)?;

    let events = gong::extract_events(&payload);
    if events.is_empty() {
        return Err(ApiError(ApplicationError::Validation(
            "no recognizable gong events found in payload".to_string(),
        )));
    }

    let existing = state.signal_service.list_decisions().await?;
    let mut existing_titles = existing
        .into_iter()
        .map(|decision| decision.title.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut created = 0usize;
    let mut skipped = 0usize;

    for event in &events {
        let input = map_gong_event_to_signal(event, None);
        let dedupe_key = input.title.to_ascii_lowercase();

        if existing_titles.contains(&dedupe_key) {
            skipped += 1;
            continue;
        }

        state.signal_service.create_decision(input).await?;
        existing_titles.insert(dedupe_key);
        created += 1;
    }

    Ok(Json(GongWebhookResponse {
        source: "gong".to_string(),
        received: events.len(),
        created,
        skipped,
    }))
}

fn validate_gong_ingest_key(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected_key) = &state.gong_ingest_key else {
        return Ok(());
    };

    let received = headers
        .get("x-signalops-ingest-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();

    if received != expected_key {
        return Err(ApiError(ApplicationError::Unauthorized(
            "invalid ingest key for gong webhook".to_string(),
        )));
    }

    Ok(())
}

fn map_jira_issue_to_signal(
    issue: &crate::infrastructure::integrations::jira_client::JiraIssue,
    default_owner: Option<String>,
) -> NewDecisionInput {
    let mut summary = format!("Imported from Jira issue {}.", issue.key);

    if let Some(status) = &issue.status {
        summary.push_str(&format!(" Status: {status}."));
    }

    if let Some(updated) = &issue.updated {
        summary.push_str(&format!(" Updated: {updated}."));
    }

    if let Some(description) = &issue.description {
        summary.push_str(" Description excerpt: ");
        summary.push_str(&truncate_chars(description, 500));
    }

    let mut tags = vec!["jira".to_string()];

    if let Some(project_key) = &issue.project_key {
        tags.push(format!("project:{}", project_key.to_ascii_lowercase()));
    }

    tags.extend(issue.labels.iter().map(|label| label.to_ascii_lowercase()));

    NewDecisionInput {
        title: format!("Jira {}: {}", issue.key, issue.summary),
        summary,
        owner: default_owner,
        source_systems: vec!["Jira".to_string()],
        tags,
        status: map_jira_status_to_signal_status(issue.status.as_deref()),
        created_at: None,
        updated_at: issue.updated.as_deref().and_then(parse_external_timestamp),
    }
}

fn parse_signal_status(value: &str) -> Result<DecisionStatus, ApiError> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "_");
    match normalized.as_str() {
        "proposed" | "todo" | "to_do" => Ok(DecisionStatus::Proposed),
        "approved" | "in_progress" => Ok(DecisionStatus::Approved),
        "superseded" | "done" | "closed" => Ok(DecisionStatus::Superseded),
        _ => Err(ApiError(ApplicationError::Validation(format!(
            "invalid status '{}'; expected proposed, approved, or superseded",
            value
        )))),
    }
}

fn map_gong_event_to_signal(
    event: &gong::GongEvent,
    default_owner: Option<String>,
) -> NewDecisionInput {
    let identity = event
        .call_id
        .clone()
        .or_else(|| event.timestamp.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut summary = format!("Captured from Gong call intelligence event '{}'.", event.event_name);

    if let Some(account_id) = &event.account_id {
        summary.push_str(&format!(" Account: {account_id}."));
    }
    if let Some(account_name) = &event.account_name {
        summary.push_str(&format!(" Account Name: {account_name}."));
    }
    if let Some(industry) = &event.industry {
        summary.push_str(&format!(" Industry: {industry}."));
    }
    if let Some(segment) = &event.segment {
        summary.push_str(&format!(" Segment: {segment}."));
    }
    if let Some(region) = &event.region {
        summary.push_str(&format!(" Region: {region}."));
    }
    if let Some(arr) = &event.arr {
        summary.push_str(&format!(" ARR: {arr}."));
    }
    if let Some(renewal_window) = &event.renewal_window {
        summary.push_str(&format!(" Renewal Window: {renewal_window}."));
    }
    if let Some(lifecycle) = &event.lifecycle {
        summary.push_str(&format!(" Lifecycle: {lifecycle}."));
    }
    if let Some(owner_name) = &event.owner_name {
        summary.push_str(&format!(" Owner: {owner_name}."));
    }
    if let Some(title) = &event.call_title {
        summary.push_str(&format!(" Call: {title}."));
    }

    if !event.participants.is_empty() {
        let participants = event
            .participants
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        summary.push_str(&format!(" Participants: {participants}."));
    }

    if let Some(timestamp) = &event.timestamp {
        summary.push_str(&format!(" Timestamp: {timestamp}."));
    }
    if let Some(sentiment) = &event.sentiment {
        summary.push_str(&format!(" Sentiment: {sentiment}."));
    }
    if let Some(outcome) = &event.outcome {
        summary.push_str(&format!(" Outcome: {outcome}."));
    }

    if let Some(score) = event.nps_score {
        summary.push_str(&format!(" NPS Score: {score}."));
    }
    if let Some(talk_ratio_rep) = event.talk_ratio_rep {
        summary.push_str(&format!(
            " Rep Talk Ratio: {:.0}%.",
            talk_ratio_rep * 100.0
        ));
    }
    if !event.next_steps.is_empty() {
        let next_steps = event
            .next_steps
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ");
        summary.push_str(&format!(" Next Steps: {next_steps}."));
    }
    if let Some(transcript_excerpt) = &event.transcript_excerpt {
        summary.push_str(" Transcript Excerpt: ");
        summary.push_str(&truncate_chars(transcript_excerpt, 500));
    }

    let mut tags = vec![
        "gong".to_string(),
        format!("event:{}", sanitize_tag(&event.event_name)),
        "source:call_intelligence".to_string(),
    ];
    if let Some(account_id) = &event.account_id {
        tags.push(format!("account:{}", sanitize_tag(account_id)));
    }
    if let Some(account_name) = &event.account_name {
        tags.push(format!("account_name:{}", sanitize_tag(account_name)));
    }
    if let Some(industry) = &event.industry {
        tags.push(format!("industry:{}", sanitize_tag(industry)));
    }
    if let Some(segment) = &event.segment {
        tags.push(format!("segment:{}", sanitize_tag(segment)));
    }
    if let Some(region) = &event.region {
        tags.push(format!("region:{}", sanitize_tag(region)));
    }
    if let Some(arr) = &event.arr {
        tags.push(format!("arr:{}", sanitize_tag(arr)));
    }
    if let Some(renewal_window) = &event.renewal_window {
        tags.push(format!("renewal_window:{}", sanitize_tag(renewal_window)));
    }
    if let Some(lifecycle) = &event.lifecycle {
        tags.push(format!("lifecycle:{}", sanitize_tag(lifecycle)));
    }
    if let Some(call_id) = &event.call_id {
        tags.push(format!("call_id:{}", sanitize_tag(call_id)));
    }
    if let Some(owner_name) = &event.owner_name {
        tags.push(format!("owner_hint:{}", sanitize_tag(owner_name)));
    }
    for topic in &event.topics {
        let normalized = sanitize_tag(topic);
        if !normalized.is_empty() {
            tags.push(format!("topic:{normalized}"));
        }
    }
    for risk in &event.risk_flags {
        let normalized = sanitize_tag(risk);
        if !normalized.is_empty() {
            tags.push(format!("risk:{normalized}"));
        }
    }
    if let Some(sentiment) = &event.sentiment {
        tags.push(format!("sentiment:{}", sanitize_tag(sentiment)));
    }
    if let Some(outcome) = &event.outcome {
        tags.push(format!("call_outcome:{}", sanitize_tag(outcome)));
    }
    if let Some(score) = event.nps_score {
        tags.push(format!("nps_score:{score}"));
        tags.push("event:nps_submitted".to_string());
    }
    if let Some(talk_ratio_rep) = event.talk_ratio_rep {
        if talk_ratio_rep >= 0.65 {
            tags.push("risk:discovery_gap".to_string());
        }
    }

    if let Some(score) = event.nps_score {
        if score <= 6 {
            tags.push("nps:detractor".to_string());
            tags.push("risk:renewal".to_string());
            tags.push("risk:sentiment".to_string());
        } else if score <= 8 {
            tags.push("nps:passive".to_string());
        } else {
            tags.push("nps:promoter".to_string());
            tags.push("opportunity:advocacy".to_string());
        }
    }

    if let Some(transcript_excerpt) = &event.transcript_excerpt {
        let transcript = transcript_excerpt.to_ascii_lowercase();
        if transcript.contains("competitor")
            || transcript.contains("switch")
            || transcript.contains("alternatives")
        {
            tags.push("risk:competitive".to_string());
            tags.push("risk:renewal".to_string());
        }
        if transcript.contains("security")
            || transcript.contains("compliance")
            || transcript.contains("audit")
        {
            tags.push("risk:security".to_string());
            tags.push("risk:compliance".to_string());
        }
        if transcript.contains("outage")
            || transcript.contains("downtime")
            || transcript.contains("incident")
            || transcript.contains("latency")
        {
            tags.push("risk:stability".to_string());
            tags.push("severity:sev2".to_string());
        }
        if transcript.contains("budget")
            || transcript.contains("cost")
            || transcript.contains("cuts")
            || transcript.contains("procurement")
        {
            tags.push("risk:renewal".to_string());
            tags.push("risk:value".to_string());
        }
        if transcript.contains("onboarding")
            || transcript.contains("adoption")
            || transcript.contains("activation")
        {
            tags.push("risk:adoption".to_string());
        }
    }

    if let Some(outcome) = &event.outcome {
        let normalized = sanitize_tag(outcome);
        if normalized == "at_risk" || normalized == "escalated" || normalized == "blocked" {
            tags.push("risk:renewal".to_string());
        } else if normalized == "expansion" || normalized == "positive" || normalized == "won" {
            tags.push("opportunity:expansion".to_string());
            tags.push("trend:improving".to_string());
        }
    }

    tags.sort();
    tags.dedup();

    NewDecisionInput {
        title: match event.call_title.as_deref() {
            Some(call_title) if !call_title.trim().is_empty() => {
                format!("Gong call {identity}: {call_title}")
            }
            _ => format!("Gong call insight ({identity})"),
        },
        summary,
        owner: default_owner.or_else(|| event.owner_name.clone()),
        source_systems: vec!["Gong".to_string()],
        tags,
        status: Some(DecisionStatus::Proposed),
        created_at: None,
        updated_at: event
            .timestamp
            .as_deref()
            .and_then(parse_external_timestamp),
    }
}

fn map_jira_status_to_signal_status(status: Option<&str>) -> Option<DecisionStatus> {
    let normalized = status
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_");

    match normalized.as_str() {
        "done" | "closed" | "resolved" => Some(DecisionStatus::Superseded),
        "in_progress" | "selected_for_development" | "review" => Some(DecisionStatus::Approved),
        "to_do" | "open" | "backlog" => Some(DecisionStatus::Proposed),
        _ => None,
    }
}

fn parse_external_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }

    DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn sanitize_tag(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '/', ':'], "_")
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn read_llm_settings(state: &AppState) -> Result<LlmRuntimeSettings, ApiError> {
    state
        .llm_settings
        .read()
        .map(|guard| guard.clone())
        .map_err(|_| {
            ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to read llm settings due to poisoned lock"
            )))
        })
}

fn resolve_llm_settings_file() -> Option<PathBuf> {
    std::env::var("SIGNALOPS_LLM_SETTINGS_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn initialize_llm_state(
    fallback_client: Option<InsightLlmClient>,
    settings_file: Option<&Path>,
) -> (Option<InsightLlmClient>, LlmRuntimeSettings) {
    if let Some(path) = settings_file {
        if let Some(settings) = load_llm_settings_from_file(path) {
            match InsightLlmClient::from_settings(settings.clone()) {
                Ok(client) => {
                    let normalized = client
                        .as_ref()
                        .map(InsightLlmClient::settings)
                        .unwrap_or(settings);
                    tracing::info!(
                        path = %path.display(),
                        provider = %normalized.provider,
                        enabled = normalized.enabled,
                        "loaded llm settings from disk"
                    );
                    return (client, normalized);
                }
                Err(error) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %error,
                        "failed to initialize llm from persisted settings; falling back to env defaults"
                    );
                }
            }
        }
    }

    let initial_settings = fallback_client
        .as_ref()
        .map(InsightLlmClient::settings)
        .unwrap_or_default();
    (fallback_client, initial_settings)
}

fn load_llm_settings_from_file(path: &Path) -> Option<LlmRuntimeSettings> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to read persisted llm settings file"
            );
            return None;
        }
    };

    match serde_json::from_slice::<LlmRuntimeSettings>(&bytes) {
        Ok(settings) => Some(settings),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to parse persisted llm settings file"
            );
            None
        }
    }
}

fn persist_llm_settings_if_configured(
    state: &AppState,
    settings: &LlmRuntimeSettings,
) -> Result<(), ApiError> {
    let Some(path) = state.llm_settings_file.as_ref() else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
                "failed to create llm settings directory '{}': {error}",
                parent.display()
            )))
        })?;
    }

    let serialized = serde_json::to_vec_pretty(settings).map_err(|error| {
        ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to serialize llm settings: {error}"
        )))
    })?;

    fs::write(path, serialized).map_err(|error| {
        ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to persist llm settings '{}': {error}",
            path.display()
        )))
    })?;

    Ok(())
}

fn resolve_llm_client(
    state: &AppState,
    settings_override: Option<LlmRuntimeSettings>,
) -> Result<InsightLlmClient, ApiError> {
    if let Some(settings) = settings_override {
        let client = InsightLlmClient::from_settings(settings)
            .map_err(|error| ApiError(ApplicationError::Validation(error.to_string())))?;
        return client.ok_or_else(|| {
            ApiError(ApplicationError::Validation(
                "llm settings are disabled; enable provider before using this action".to_string(),
            ))
        });
    }

    let guard = state.llm_client.read().map_err(|_| {
        ApiError(ApplicationError::Unexpected(anyhow::anyhow!(
            "failed to read llm client due to poisoned lock"
        )))
    })?;
    guard.clone().ok_or_else(|| {
        ApiError(ApplicationError::Unavailable(
            "llm analytics is not configured; save llm settings first".to_string(),
        ))
    })
}

struct ApiError(ApplicationError);

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!(error = ?self.0, "request failed");
        let message = self.0.to_string();
        let (status, code) = match &self.0 {
            ApplicationError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            ApplicationError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApplicationError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            ApplicationError::Unexpected(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
        };

        (
            status,
            Json(json!({
                "error": code,
                "message": message
            })),
        )
            .into_response()
    }
}
