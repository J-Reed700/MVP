use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set, Statement,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    application::ports::{DecisionRepository, UpsertOutcome},
    domain::models::{Decision, DecisionStatus, NewDecisionInput},
    infrastructure::entities::decision,
};

pub struct PostgresDecisionRepository {
    db: DatabaseConnection,
}

impl PostgresDecisionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn ensure_schema(&self) -> Result<()> {
        let statement = Statement::from_string(
            DbBackend::Postgres,
            r#"
            CREATE TABLE IF NOT EXISTS decisions (
                id UUID PRIMARY KEY,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                owner TEXT NULL,
                status TEXT NOT NULL,
                source_systems JSONB NOT NULL DEFAULT '[]'::jsonb,
                tags JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL
            )
            "#
            .to_string(),
        );

        self.db
            .execute(statement)
            .await
            .context("failed to ensure decisions table exists")?;

        Ok(())
    }

    pub async fn seed_if_empty(&self) -> Result<()> {
        let count = decision::Entity::find().count(&self.db).await?;
        if count > 0 {
            return Ok(());
        }

        let samples = [
            NewDecisionInput {
                title: "Adopt centralized incident taxonomy".to_string(),
                summary: "Standardize incident tagging across support and engineering tools."
                    .to_string(),
                owner: Some("Security Operations".to_string()),
                source_systems: vec!["Jira".to_string(), "Notion".to_string()],
                tags: vec!["incident".to_string(), "governance".to_string()],
                status: Some(DecisionStatus::Approved),
                created_at: None,
                updated_at: None,
            },
            NewDecisionInput {
                title: "Retire legacy CRM fields".to_string(),
                summary:
                    "Consolidate duplicate customer lifecycle fields into one canonical model."
                        .to_string(),
                owner: None,
                source_systems: vec!["Salesforce".to_string(), "Looker".to_string()],
                tags: vec!["crm".to_string(), "data_quality".to_string()],
                status: Some(DecisionStatus::Proposed),
                created_at: None,
                updated_at: None,
            },
        ];

        for sample in samples {
            self.create(sample).await?;
        }

        Ok(())
    }

    fn to_domain(model: decision::Model) -> Result<Decision> {
        Ok(Decision {
            id: model.id,
            title: model.title,
            summary: model.summary,
            owner: model.owner,
            status: parse_status(&model.status)?,
            source_systems: serde_json::from_value(model.source_systems)
                .context("invalid source_systems payload")?,
            tags: serde_json::from_value(model.tags).context("invalid tags payload")?,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        })
    }
}

#[async_trait]
impl DecisionRepository for PostgresDecisionRepository {
    async fn list(&self) -> Result<Vec<Decision>> {
        let rows = decision::Entity::find()
            .order_by_desc(decision::Column::UpdatedAt)
            .all(&self.db)
            .await
            .context("failed to list decisions")?;

        rows.into_iter()
            .map(PostgresDecisionRepository::to_domain)
            .collect::<Result<Vec<_>>>()
    }

    async fn create(&self, input: NewDecisionInput) -> Result<Decision> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let status = input.status.unwrap_or(DecisionStatus::Proposed);
        let created_at = input.created_at.unwrap_or(now);
        let mut updated_at = input.updated_at.unwrap_or(now);
        if updated_at < created_at {
            updated_at = created_at;
        }

        let active_model = decision::ActiveModel {
            id: Set(id),
            title: Set(input.title),
            summary: Set(input.summary),
            owner: Set(input.owner),
            status: Set(status_to_db(&status)),
            source_systems: Set(json!(input.source_systems)),
            tags: Set(json!(input.tags)),
            created_at: Set(created_at.into()),
            updated_at: Set(updated_at.into()),
        };

        let inserted = active_model
            .insert(&self.db)
            .await
            .context("failed to insert decision")?;

        PostgresDecisionRepository::to_domain(inserted)
    }

    async fn delete_all(&self) -> Result<usize> {
        let result = decision::Entity::delete_many()
            .exec(&self.db)
            .await
            .context("failed to delete all decisions")?;
        Ok(result.rows_affected as usize)
    }

    async fn upsert_by_title(&self, input: NewDecisionInput) -> Result<UpsertOutcome> {
        let existing = decision::Entity::find()
            .filter(decision::Column::Title.eq(input.title.clone()))
            .one(&self.db)
            .await
            .context("failed to query decision by title for upsert")?;

        let Some(existing) = existing else {
            let _ = self.create(input).await?;
            return Ok(UpsertOutcome::Created);
        };

        let now = Utc::now();
        let mut active_model: decision::ActiveModel = existing.clone().into_active_model();
        let existing_created_at = existing.created_at.with_timezone(&Utc);
        let existing_updated_at = existing.updated_at.with_timezone(&Utc);
        let existing_tags: Vec<String> =
            serde_json::from_value(existing.tags).context("invalid tags payload")?;
        let existing_sources: Vec<String> = serde_json::from_value(existing.source_systems)
            .context("invalid source_systems payload")?;
        let existing_status = parse_status(&existing.status)?;

        let mut merged_sources = existing_sources;
        for source in input.source_systems {
            if !merged_sources.iter().any(|item| item == &source) {
                merged_sources.push(source);
            }
        }

        let mut merged_tags = existing_tags;
        for tag in input.tags {
            if !merged_tags
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&tag))
            {
                merged_tags.push(tag);
            }
        }

        let merged_owner = match (existing.owner, input.owner) {
            (Some(current), _) => Some(current),
            (None, candidate) => candidate,
        };
        let merged_status = input.status.unwrap_or(existing_status);
        let merged_created_at = input
            .created_at
            .map(|candidate| candidate.min(existing_created_at))
            .unwrap_or(existing_created_at);
        let merged_updated_at = input
            .updated_at
            .map(|candidate| candidate.max(existing_updated_at))
            .unwrap_or(existing_updated_at)
            .max(now);

        active_model.summary = Set(input.summary);
        active_model.owner = Set(merged_owner);
        active_model.status = Set(status_to_db(&merged_status));
        active_model.source_systems = Set(json!(merged_sources));
        active_model.tags = Set(json!(merged_tags));
        active_model.created_at = Set(merged_created_at.into());
        active_model.updated_at = Set(merged_updated_at.into());

        active_model
            .update(&self.db)
            .await
            .context("failed to update decision during upsert")?;

        Ok(UpsertOutcome::Updated)
    }

    async fn bulk_assign_owner(
        &self,
        ids: Vec<Uuid>,
        owner: String,
        only_if_owner_missing: bool,
    ) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let mut query = decision::Entity::update_many()
            .col_expr(decision::Column::Owner, Expr::value(owner))
            .col_expr(decision::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(decision::Column::Id.is_in(ids));

        if only_if_owner_missing {
            query = query.filter(decision::Column::Owner.is_null());
        }

        let result = query
            .exec(&self.db)
            .await
            .context("failed to bulk assign decision owners")?;

        Ok(result.rows_affected as usize)
    }

    async fn bulk_set_status(&self, ids: Vec<Uuid>, status: DecisionStatus) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let result = decision::Entity::update_many()
            .col_expr(decision::Column::Status, Expr::value(status_to_db(&status)))
            .col_expr(decision::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(decision::Column::Id.is_in(ids))
            .exec(&self.db)
            .await
            .context("failed to bulk set decision status")?;

        Ok(result.rows_affected as usize)
    }

    async fn bulk_add_tag(&self, ids: Vec<Uuid>, tag: String) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let rows = decision::Entity::find()
            .filter(decision::Column::Id.is_in(ids))
            .all(&self.db)
            .await
            .context("failed to load decisions for bulk tag update")?;

        let mut updated = 0usize;
        for row in rows {
            let mut tags: Vec<String> =
                serde_json::from_value(row.tags.clone()).context("invalid tags payload")?;
            if tags
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&tag))
            {
                continue;
            }
            tags.push(tag.clone());

            let mut active_model: decision::ActiveModel = row.into_active_model();
            active_model.tags = Set(json!(tags));
            active_model.updated_at = Set(Utc::now().into());
            active_model
                .update(&self.db)
                .await
                .context("failed to update decision tags")?;
            updated += 1;
        }

        Ok(updated)
    }
}

fn status_to_db(status: &DecisionStatus) -> String {
    match status {
        DecisionStatus::Proposed => "proposed",
        DecisionStatus::Approved => "approved",
        DecisionStatus::Superseded => "superseded",
    }
    .to_string()
}

fn parse_status(value: &str) -> Result<DecisionStatus> {
    match value {
        "proposed" => Ok(DecisionStatus::Proposed),
        "approved" => Ok(DecisionStatus::Approved),
        "superseded" => Ok(DecisionStatus::Superseded),
        "" => bail!("empty status value in database"),
        other => Err(anyhow!("unknown decision status '{other}'")),
    }
}
