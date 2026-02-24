mod application;
mod domain;
mod infrastructure;
mod presentation;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use sea_orm::Database;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    application::{decision_service::DecisionService, ports::InsightAnalytics},
    infrastructure::{
        integrations::{
            gong_client::GongClient, insight_llm::InsightLlmClient, jira_client::JiraClient,
        },
        postgres_repo::PostgresDecisionRepository,
    },
    presentation::http::build_router,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "signalops_backend=debug,tower_http=debug".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL must be set (example: postgres://postgres:postgres@localhost:5432/signalops)",
    )?;
    let database = Database::connect(&database_url)
        .await
        .context("failed to connect to postgres database")?;

    let repository = Arc::new(PostgresDecisionRepository::new(database));
    repository.ensure_schema().await?;
    repository.seed_if_empty().await?;

    let llm_client = InsightLlmClient::from_env()?;
    let insight_analytics = llm_client
        .clone()
        .map(|client| Arc::new(client) as Arc<dyn InsightAnalytics>);
    let service = Arc::new(DecisionService::with_analytics(
        repository,
        insight_analytics,
    ));
    let jira_client = JiraClient::from_env()?;
    let gong_client = GongClient::from_env()?;
    let gong_ingest_key = std::env::var("SIGNALOPS_WEBHOOK_INGEST_KEY")
        .or_else(|_| std::env::var("GONG_WEBHOOK_INGEST_KEY"))
        .or_else(|_| std::env::var("PENDO_WEBHOOK_INGEST_KEY"))
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());

    let app = build_router(
        service,
        jira_client,
        gong_client,
        gong_ingest_key,
        llm_client,
    );

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind to {address}"))?;

    tracing::info!(%address, "signalops backend listening");
    axum::serve(listener, app)
        .await
        .context("backend server terminated unexpectedly")?;

    Ok(())
}
