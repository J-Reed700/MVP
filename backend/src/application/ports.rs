use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::models::{Decision, DecisionStatus, Insight, NewDecisionInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Created,
    Updated,
}

#[async_trait]
pub trait DecisionRepository: Send + Sync {
    async fn list(&self) -> anyhow::Result<Vec<Decision>>;
    async fn create(&self, input: NewDecisionInput) -> anyhow::Result<Decision>;
    async fn delete_all(&self) -> anyhow::Result<usize>;
    async fn upsert_by_title(&self, input: NewDecisionInput) -> anyhow::Result<UpsertOutcome>;
    async fn bulk_assign_owner(
        &self,
        ids: Vec<Uuid>,
        owner: String,
        only_if_owner_missing: bool,
    ) -> anyhow::Result<usize>;
    async fn bulk_set_status(
        &self,
        ids: Vec<Uuid>,
        status: DecisionStatus,
    ) -> anyhow::Result<usize>;
    async fn bulk_add_tag(&self, ids: Vec<Uuid>, tag: String) -> anyhow::Result<usize>;
}

#[async_trait]
pub trait InsightAnalytics: Send + Sync {
    async fn enrich_insights(
        &self,
        decisions: &[Decision],
        insights: &[Insight],
    ) -> anyhow::Result<Vec<Insight>>;
}
