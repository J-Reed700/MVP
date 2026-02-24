use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Proposed,
    Approved,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub title: String,
    pub summary: String,
    pub owner: Option<String>,
    pub status: DecisionStatus,
    pub source_systems: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDecisionInput {
    pub title: String,
    pub summary: String,
    pub owner: Option<String>,
    pub source_systems: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub status: Option<DecisionStatus>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Insight {
    pub category: String,
    pub audience: String,
    pub priority: String,
    pub title: String,
    pub recommendation: String,
    pub metric: Option<String>,
    pub related_signal_ids: Vec<Uuid>,
    pub owner_role: String,
    pub due_in_days: u16,
    pub confidence: f32,
    pub confidence_explanation: Option<String>,
    pub rationale: Option<String>,
    pub evidence: Vec<String>,
    pub generated_by: String,
    pub playbook_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}
