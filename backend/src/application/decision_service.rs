use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use chrono::{Duration, Utc};

use crate::{
    application::{
        errors::{ApplicationError, ApplicationResult},
        ports::{DecisionRepository, InsightAnalytics, UpsertOutcome},
    },
    domain::models::{
        Decision, DecisionStatus, GraphEdge, GraphNode, GraphSnapshot, Insight, NewDecisionInput,
    },
};

pub struct DecisionService {
    repository: Arc<dyn DecisionRepository>,
    insight_analytics: RwLock<Option<Arc<dyn InsightAnalytics>>>,
}

impl DecisionService {
    pub fn with_analytics(
        repository: Arc<dyn DecisionRepository>,
        insight_analytics: Option<Arc<dyn InsightAnalytics>>,
    ) -> Self {
        Self {
            repository,
            insight_analytics: RwLock::new(insight_analytics),
        }
    }

    pub fn set_insight_analytics(&self, insight_analytics: Option<Arc<dyn InsightAnalytics>>) {
        match self.insight_analytics.write() {
            Ok(mut guard) => {
                *guard = insight_analytics;
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed to update insight analytics due to poisoned lock"
                );
            }
        }
    }

    pub async fn list_decisions(&self) -> ApplicationResult<Vec<Decision>> {
        self.repository.list().await.map_err(ApplicationError::from)
    }

    pub async fn create_decision(&self, input: NewDecisionInput) -> ApplicationResult<Decision> {
        let normalized = normalize_new_decision_input(input)?;
        self.repository
            .create(normalized)
            .await
            .map_err(ApplicationError::from)
    }

    pub async fn upsert_decision_by_title(
        &self,
        input: NewDecisionInput,
    ) -> ApplicationResult<UpsertOutcome> {
        let normalized = normalize_new_decision_input(input)?;
        self.repository
            .upsert_by_title(normalized)
            .await
            .map_err(ApplicationError::from)
    }

    pub async fn bulk_assign_owner(
        &self,
        ids: Vec<uuid::Uuid>,
        owner: String,
        only_if_owner_missing: bool,
    ) -> ApplicationResult<usize> {
        let normalized_owner = owner.trim();
        if normalized_owner.is_empty() {
            return Err(ApplicationError::Validation(
                "owner is required for assign_owner action".to_string(),
            ));
        }
        if ids.is_empty() {
            return Err(ApplicationError::Validation(
                "at least one signal id is required".to_string(),
            ));
        }

        self.repository
            .bulk_assign_owner(ids, normalized_owner.to_string(), only_if_owner_missing)
            .await
            .map_err(ApplicationError::from)
    }

    pub async fn bulk_set_status(
        &self,
        ids: Vec<uuid::Uuid>,
        status: DecisionStatus,
    ) -> ApplicationResult<usize> {
        if ids.is_empty() {
            return Err(ApplicationError::Validation(
                "at least one signal id is required".to_string(),
            ));
        }

        self.repository
            .bulk_set_status(ids, status)
            .await
            .map_err(ApplicationError::from)
    }

    pub async fn bulk_add_tag(
        &self,
        ids: Vec<uuid::Uuid>,
        tag: String,
    ) -> ApplicationResult<usize> {
        let normalized_tag = tag.trim().to_ascii_lowercase();
        if normalized_tag.is_empty() {
            return Err(ApplicationError::Validation(
                "tag is required for add_tag action".to_string(),
            ));
        }
        if ids.is_empty() {
            return Err(ApplicationError::Validation(
                "at least one signal id is required".to_string(),
            ));
        }

        self.repository
            .bulk_add_tag(ids, normalized_tag)
            .await
            .map_err(ApplicationError::from)
    }

    pub async fn load_story_dataset(&self) -> ApplicationResult<usize> {
        let dataset = build_story_dataset();
        self.repository
            .delete_all()
            .await
            .map_err(ApplicationError::from)?;

        let mut loaded = 0usize;
        for input in dataset {
            self.create_decision(input).await?;
            loaded += 1;
        }

        Ok(loaded)
    }

    pub async fn get_graph_snapshot(&self) -> ApplicationResult<GraphSnapshot> {
        let decisions = self
            .repository
            .list()
            .await
            .map_err(ApplicationError::from)?;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for decision in &decisions {
            let signal_node_id = format!("signal:{}", decision.id);
            nodes.push(GraphNode {
                id: signal_node_id.clone(),
                label: decision.title.clone(),
                node_type: "signal".to_string(),
            });

            if let Some(owner) = &decision.owner {
                let owner_node_id = format!("owner:{owner}");
                nodes.push(GraphNode {
                    id: owner_node_id.clone(),
                    label: owner.clone(),
                    node_type: "owner".to_string(),
                });
                edges.push(GraphEdge {
                    from: signal_node_id.clone(),
                    to: owner_node_id,
                    relation: "owned_by".to_string(),
                });
            }

            for source in &decision.source_systems {
                let source_node_id = format!("source:{source}");
                nodes.push(GraphNode {
                    id: source_node_id.clone(),
                    label: source.clone(),
                    node_type: "source".to_string(),
                });
                edges.push(GraphEdge {
                    from: signal_node_id.clone(),
                    to: source_node_id,
                    relation: "supported_by".to_string(),
                });
            }
        }

        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        nodes.dedup_by(|a, b| a.id == b.id);

        Ok(GraphSnapshot { nodes, edges })
    }

    pub async fn get_insights(&self) -> ApplicationResult<Vec<Insight>> {
        let decisions = self
            .repository
            .list()
            .await
            .map_err(ApplicationError::from)?;
        let mut insights = Vec::new();
        if decisions.is_empty() {
            return Ok(insights);
        }

        let stale_threshold = Utc::now() - Duration::days(30);
        let total = decisions.len();

        let stale: Vec<_> = decisions
            .iter()
            .filter(|decision| decision.updated_at < stale_threshold)
            .map(|decision| decision.id)
            .collect();

        if !stale.is_empty() {
            insights.push(build_insight(
                "stale_signals",
                "manager",
                if stale.len() >= 20 { "high" } else { "medium" },
                format!("{} records need refresh", stale.len()),
                "Review these records and either update context or close them so teams are working from current information."
                    .to_string(),
                Some(format!("{:.0}% stale", percentage(stale.len(), total))),
                stale,
            ));
        }

        let unowned: Vec<_> = decisions
            .iter()
            .filter(|decision| decision.owner.is_none())
            .map(|decision| decision.id)
            .collect();

        if !unowned.is_empty() {
            insights.push(build_insight(
                "missing_owners",
                "manager",
                if unowned.len() >= 25 {
                    "high"
                } else {
                    "medium"
                },
                format!("{} records have no accountable owner", unowned.len()),
                "Assign one owner per record so follow-up has clear accountability.".to_string(),
                Some(format!("{:.0}% unowned", percentage(unowned.len(), total))),
                unowned,
            ));
        }

        let superseded: Vec<_> = decisions
            .iter()
            .filter(|decision| matches!(decision.status, DecisionStatus::Superseded))
            .map(|decision| decision.id)
            .collect();

        if !superseded.is_empty() {
            insights.push(build_insight(
                "superseded_records",
                "manager",
                "low",
                format!("{} outdated records are still in view", superseded.len()),
                "Archive outdated records from default views while keeping audit history."
                    .to_string(),
                Some(format!(
                    "{:.0}% superseded",
                    percentage(superseded.len(), total)
                )),
                superseded,
            ));
        }

        if let Some(owner_concentration) = build_owner_concentration_insight(&decisions) {
            insights.push(owner_concentration);
        }

        if let Some(source_concentration) = build_source_concentration_insight(&decisions) {
            insights.push(source_concentration);
        }

        if let Some(untagged_hygiene) = build_untagged_hygiene_insight(&decisions) {
            insights.push(untagged_hygiene);
        }

        if let Some(duplicate_titles) = build_duplicate_title_insight(&decisions) {
            insights.push(duplicate_titles);
        }

        if let Some(source_owner_gap) = build_source_owner_gap_insight(&decisions) {
            insights.push(source_owner_gap);
        }

        if let Some(account_hotspots) = build_account_hotspot_insight(&decisions) {
            insights.push(account_hotspots);
        }

        if let Some(nps_follow_up) = build_nps_follow_up_insight(&decisions) {
            insights.push(nps_follow_up);
        }

        if let Some(competitive_pressure) = build_competitive_pressure_insight(&decisions) {
            insights.push(competitive_pressure);
        }

        if let Some(discovery_quality_gap) = build_discovery_quality_gap_insight(&decisions) {
            insights.push(discovery_quality_gap);
        }

        if let Some(expansion_momentum) = build_expansion_momentum_insight(&decisions) {
            insights.push(expansion_momentum);
        }

        if let Some(renewal_window_risk) = build_renewal_window_risk_insight(&decisions) {
            insights.push(renewal_window_risk);
        }

        if let Some(arr_exposure_risk) = build_arr_exposure_risk_insight(&decisions) {
            insights.push(arr_exposure_risk);
        }

        if let Some(industry_cluster_risk) = build_industry_cluster_risk_insight(&decisions) {
            insights.push(industry_cluster_risk);
        }

        if let Some(severity_burden) = build_severity_burden_insight(&decisions) {
            insights.push(severity_burden);
        }

        if let Some(evidence_confidence_gap) = build_evidence_confidence_gap_insight(&decisions) {
            insights.push(evidence_confidence_gap);
        }

        enrich_rule_insights(&mut insights, &decisions);

        let analytics = self
            .insight_analytics
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(Arc::clone));

        if let Some(analytics) = analytics {
            match analytics.enrich_insights(&decisions, &insights).await {
                Ok(enriched) => return Ok(enriched),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        error_chain = %format!("{error:#}"),
                        "llm insight enrichment failed; returning deterministic insights"
                    );
                }
            }
        }

        Ok(insights)
    }
}

fn build_insight(
    category: &str,
    audience: &str,
    priority: &str,
    title: String,
    recommendation: String,
    metric: Option<String>,
    related_signal_ids: Vec<uuid::Uuid>,
) -> Insight {
    let action_profile = action_profile_for(category, audience, priority);
    Insight {
        category: category.to_string(),
        audience: audience.to_string(),
        priority: priority.to_string(),
        title,
        recommendation,
        metric,
        related_signal_ids: unique_uuids(related_signal_ids),
        owner_role: action_profile.owner_role.to_string(),
        due_in_days: action_profile.due_in_days,
        confidence: action_profile.confidence,
        confidence_explanation: None,
        rationale: None,
        evidence: Vec::new(),
        generated_by: "rules".to_string(),
        playbook_steps: action_profile.playbook_steps,
    }
}

struct ActionProfile {
    owner_role: &'static str,
    due_in_days: u16,
    confidence: f32,
    playbook_steps: Vec<String>,
}

fn action_profile_for(category: &str, audience: &str, priority: &str) -> ActionProfile {
    match category {
        "stale_signals" => ActionProfile {
            owner_role: "CS Operations",
            due_in_days: 5,
            confidence: 0.86,
            playbook_steps: vec![
                "Group stale records by account and revenue tier.".to_string(),
                "Update, close, or archive records older than 30 days.".to_string(),
                "Run a weekly owner-led cleanup routine.".to_string(),
            ],
        },
        "missing_owners" => ActionProfile {
            owner_role: "CS Manager",
            due_in_days: 2,
            confidence: 0.93,
            playbook_steps: vec![
                "Auto-assign owner by segment/book when possible.".to_string(),
                "Escalate unassigned strategic accounts to leadership.".to_string(),
                "Track ownership coverage in weekly operating review.".to_string(),
            ],
        },
        "superseded_records" => ActionProfile {
            owner_role: "CS Operations",
            due_in_days: 10,
            confidence: 0.82,
            playbook_steps: vec![
                "Move outdated records out of default views.".to_string(),
                "Link each outdated record to its replacement.".to_string(),
                "Review archive quality monthly.".to_string(),
            ],
        },
        "owner_concentration_risk" => ActionProfile {
            owner_role: "Head of Customer Success",
            due_in_days: 7,
            confidence: 0.88,
            playbook_steps: vec![
                "Redistribute high-load accounts across CSMs.".to_string(),
                "Document backup owner for top critical accounts.".to_string(),
                "Review load balancing in forecast/renewal cadences.".to_string(),
            ],
        },
        "source_dependency_risk" => ActionProfile {
            owner_role: "Product Operations",
            due_in_days: 14,
            confidence: 0.84,
            playbook_steps: vec![
                "Identify where one source is the only evidence path.".to_string(),
                "Prioritize the next integration for those workflows.".to_string(),
                "Set a minimum evidence-source standard per account.".to_string(),
            ],
        },
        "metadata_hygiene_gap" => ActionProfile {
            owner_role: "CS Operations",
            due_in_days: 10,
            confidence: 0.8,
            playbook_steps: vec![
                "Require baseline tags on all ingest paths.".to_string(),
                "Backfill missing tags for highest-value accounts first.".to_string(),
                "Alert when untagged record volume exceeds threshold.".to_string(),
            ],
        },
        "possible_duplicate_signals" => ActionProfile {
            owner_role: "CS Operations",
            due_in_days: 7,
            confidence: 0.72,
            playbook_steps: vec![
                "Review duplicate clusters during weekly triage.".to_string(),
                "Merge duplicates and link canonical records.".to_string(),
                "Add duplicate checks to sync and ingest flows.".to_string(),
            ],
        },
        "source_owner_gap" => ActionProfile {
            owner_role: "CS Manager",
            due_in_days: 3,
            confidence: 0.9,
            playbook_steps: vec![
                "Enforce owner assignment at ingest for affected sources.".to_string(),
                "Escalate ownerless strategic accounts to managers.".to_string(),
                "Track owner coverage by source every week.".to_string(),
            ],
        },
        "account_signal_hotspots" => ActionProfile {
            owner_role: "Customer Success Manager",
            due_in_days: 2,
            confidence: 0.87,
            playbook_steps: vec![
                "Review the latest timeline for affected accounts.".to_string(),
                "Schedule customer follow-up for risk or growth decisions.".to_string(),
                "Log next action, owner, and due date.".to_string(),
            ],
        },
        "nps_follow_up_queue" => ActionProfile {
            owner_role: "Customer Success Manager",
            due_in_days: 1,
            confidence: 0.9,
            playbook_steps: vec![
                "Contact detractors and confirm root cause within 24 hours.".to_string(),
                "Open cross-functional fixes for recurring feedback themes.".to_string(),
                "Close loop with customer and update health note.".to_string(),
            ],
        },
        "competitive_pressure_risk" => ActionProfile {
            owner_role: "Renewal Manager",
            due_in_days: 2,
            confidence: 0.89,
            playbook_steps: vec![
                "Create account-level competitive battlecards for impacted renewals.".to_string(),
                "Run value-recapture calls with economic buyer and champion.".to_string(),
                "Track win/loss risk changes weekly until renewal decision.".to_string(),
            ],
        },
        "discovery_quality_gap" => ActionProfile {
            owner_role: "Customer Success Enablement",
            due_in_days: 5,
            confidence: 0.84,
            playbook_steps: vec![
                "Coach CSMs on discovery talk-listen balance for risk calls.".to_string(),
                "Add mandatory customer voice checkpoints before proposing actions.".to_string(),
                "Audit next 10 strategic calls for discovery quality improvements.".to_string(),
            ],
        },
        "expansion_momentum" => ActionProfile {
            owner_role: "Account Director",
            due_in_days: 4,
            confidence: 0.87,
            playbook_steps: vec![
                "Convert positive momentum accounts into named expansion plans.".to_string(),
                "Attach quantified value outcomes to every expansion opportunity.".to_string(),
                "Schedule executive sponsor reviews to secure multi-team rollout.".to_string(),
            ],
        },
        "renewal_window_risk" => ActionProfile {
            owner_role: "Renewal Manager",
            due_in_days: 2,
            confidence: 0.92,
            playbook_steps: vec![
                "Run executive risk reviews for accounts inside the next 90 days.".to_string(),
                "Assign mitigation owners for each blocker with weekly checkpoints."
                    .to_string(),
                "Publish a customer-facing recovery timeline.".to_string(),
            ],
        },
        "arr_exposure_risk" => ActionProfile {
            owner_role: "VP Customer Success",
            due_in_days: 3,
            confidence: 0.9,
            playbook_steps: vec![
                "Prioritize highest revenue risk accounts in weekly operating review.".to_string(),
                "Allocate specialist resources to highest revenue clusters.".to_string(),
                "Track revenue risk trend and mitigation progress weekly.".to_string(),
            ],
        },
        "industry_cluster_risk" => ActionProfile {
            owner_role: "Product Operations",
            due_in_days: 5,
            confidence: 0.86,
            playbook_steps: vec![
                "Launch industry-specific response plans for repeated blockers.".to_string(),
                "Bundle top blockers into one cross-functional plan.".to_string(),
                "Measure recurrence by industry over the next 30 days.".to_string(),
            ],
        },
        "severity_burden" => ActionProfile {
            owner_role: "Platform Reliability",
            due_in_days: 1,
            confidence: 0.91,
            playbook_steps: vec![
                "Open incident command cadence for sev1/sev2 backlog.".to_string(),
                "Escalate unresolved sev1s to engineering leadership within same business day."
                    .to_string(),
                "Publish customer-facing status and ETA updates every 24h.".to_string(),
            ],
        },
        "evidence_confidence_gap" => ActionProfile {
            owner_role: "CS Operations",
            due_in_days: 4,
            confidence: 0.83,
            playbook_steps: vec![
                "Require at least one secondary source on high-risk signals.".to_string(),
                "Backfill CRM or support linkage for single-source records.".to_string(),
                "Track evidence quality score in weekly governance review.".to_string(),
            ],
        },
        _ => {
            let due_in_days = if priority.eq_ignore_ascii_case("high") {
                3
            } else if priority.eq_ignore_ascii_case("medium") {
                7
            } else {
                14
            };

            ActionProfile {
                owner_role: if audience.eq_ignore_ascii_case("csm") {
                    "Customer Success Manager"
                } else {
                    "CS Operations"
                },
                due_in_days,
                confidence: 0.7,
                playbook_steps: vec![
                    "Review the supporting records for this insight.".to_string(),
                    "Assign an accountable owner and due date.".to_string(),
                    "Track completion in weekly ops review.".to_string(),
                ],
            }
        }
    }
}

fn build_owner_concentration_insight(decisions: &[Decision]) -> Option<Insight> {
    if decisions.len() < 10 {
        return None;
    }

    let mut by_owner: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();
    for decision in decisions {
        if let Some(owner) = &decision.owner {
            by_owner.entry(owner.clone()).or_default().push(decision.id);
        }
    }

    let mut ranked = by_owner.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let (owner, ids) = ranked.into_iter().next()?;
    let share = percentage(ids.len(), decisions.len());
    if share < 45.0 {
        return None;
    }

    Some(build_insight(
        "owner_concentration_risk",
        "manager",
        "high",
        format!("Ownership is concentrated with {owner} ({share:.1}%)"),
        "Rebalance assignments across managers to reduce execution bottlenecks and key-person risk."
            .to_string(),
        Some(format!("{} of {} signals", ids.len(), decisions.len())),
        ids,
    ))
}

fn build_source_concentration_insight(decisions: &[Decision]) -> Option<Insight> {
    if decisions.len() < 12 {
        return None;
    }

    let mut source_map: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();
    for decision in decisions {
        let mut local = HashSet::new();
        for source in &decision.source_systems {
            let source_key = source.to_ascii_lowercase();
            if local.insert(source_key.clone()) {
                source_map.entry(source_key).or_default().push(decision.id);
            }
        }
    }

    let mut ranked = source_map.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let (source, ids) = ranked.into_iter().next()?;
    let share = percentage(ids.len(), decisions.len());
    if share < 55.0 {
        return None;
    }

    Some(build_insight(
        "source_dependency_risk",
        "manager",
        "medium",
        format!("{} is the only evidence source for {:.0}% of records", source.to_uppercase(), share),
        "Add a second evidence source for critical workflows to reduce blind spots."
            .to_string(),
        Some(format!("{} of {} signals", ids.len(), decisions.len())),
        ids,
    ))
}

fn build_untagged_hygiene_insight(decisions: &[Decision]) -> Option<Insight> {
    if decisions.len() < 8 {
        return None;
    }

    let untagged: Vec<_> = decisions
        .iter()
        .filter(|decision| decision.tags.is_empty())
        .map(|decision| decision.id)
        .collect();

    if untagged.is_empty() {
        return None;
    }

    let ratio = percentage(untagged.len(), decisions.len());
    if ratio < 25.0 {
        return None;
    }

    Some(build_insight(
        "metadata_hygiene_gap",
        "manager",
        "medium",
        format!("{:.0}% of records are hard to classify (missing tags)", ratio),
        "Enforce baseline tagging so teams can filter by account, risk type, and product area."
            .to_string(),
        Some(format!("{} untagged signals", untagged.len())),
        untagged,
    ))
}

fn build_duplicate_title_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut normalized: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();

    for decision in decisions {
        normalized
            .entry(normalize_title(&decision.title))
            .or_default()
            .push(decision.id);
    }

    let duplicate_groups: Vec<Vec<uuid::Uuid>> = normalized
        .into_values()
        .filter(|ids| ids.len() > 1)
        .collect();

    if duplicate_groups.is_empty() {
        return None;
    }

    let duplicate_count = duplicate_groups.iter().map(|ids| ids.len()).sum::<usize>();
    let related = duplicate_groups
        .into_iter()
        .flat_map(|ids| ids.into_iter())
        .collect::<Vec<_>>();

    Some(build_insight(
        "possible_duplicate_signals",
        "manager",
        "medium",
        format!("{} records may be duplicates", duplicate_count),
        "Merge or link duplicate records so teams do not execute conflicting actions."
            .to_string(),
        Some("Similarity based on normalized title".to_string()),
        related,
    ))
}

fn build_source_owner_gap_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut totals: HashMap<String, usize> = HashMap::new();
    let mut unowned: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();

    for decision in decisions {
        for source in &decision.source_systems {
            let source_key = source.to_ascii_lowercase();
            *totals.entry(source_key.clone()).or_default() += 1;
            if decision.owner.is_none() {
                unowned.entry(source_key).or_default().push(decision.id);
            }
        }
    }

    let mut problematic_sources = Vec::new();
    let mut related = Vec::new();
    let mut ranked_sources = totals.into_iter().collect::<Vec<_>>();
    ranked_sources.sort_by(|a, b| a.0.cmp(&b.0));
    for (source, total) in ranked_sources {
        let missing = unowned.get(&source).map_or(0, Vec::len);
        if total >= 5 && percentage(missing, total) >= 40.0 {
            problematic_sources.push(format!(
                "{} ({}/{} unowned)",
                source.to_uppercase(),
                missing,
                total
            ));
            if let Some(ids) = unowned.get(&source) {
                related.extend(ids.iter().copied());
            }
        }
    }

    if problematic_sources.is_empty() {
        return None;
    }

    Some(build_insight(
        "source_owner_gap",
        "manager",
        "high",
        "Some sources are creating too many unowned records".to_string(),
        format!(
            "Fix ownership assignment at ingest for: {}.",
            problematic_sources.join(", ")
        ),
        Some(format!(
            "{} systems exceed owner gap threshold",
            problematic_sources.len()
        )),
        related,
    ))
}

fn build_account_hotspot_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut per_account: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();

    for decision in decisions {
        for tag in &decision.tags {
            if let Some(account) = tag.strip_prefix("account:") {
                per_account
                    .entry(account.to_string())
                    .or_default()
                    .push(decision.id);
            }
        }
    }

    let mut ranked = per_account.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    let top = ranked.first()?;
    if top.1.len() < 5 {
        return None;
    }

    let top_accounts = ranked
        .iter()
        .take(3)
        .map(|(account, ids)| format!("{account} ({})", ids.len()))
        .collect::<Vec<_>>()
        .join(", ");

    let related = ranked
        .iter()
        .take(3)
        .flat_map(|(_, ids)| ids.iter().copied())
        .collect::<Vec<_>>();

    Some(build_insight(
        "account_signal_hotspots",
        "csm",
        "high",
        "Accounts that need immediate attention".to_string(),
        format!(
            "Start with {top_accounts}. Review timeline evidence, then run customer follow-ups with named owners and dates."
        ),
        Some(format!("top account {} has {} events", top.0, top.1.len())),
        related,
    ))
}

fn build_nps_follow_up_insight(decisions: &[Decision]) -> Option<Insight> {
    let nps_ids: Vec<_> = decisions
        .iter()
        .filter(|decision| decision.tags.iter().any(|tag| tag == "event:nps_submitted"))
        .map(|decision| decision.id)
        .collect();

    if nps_ids.is_empty() {
        return None;
    }

    Some(build_insight(
        "nps_follow_up_queue",
        "csm",
        "medium",
        format!("{} customer feedback events need follow-up", nps_ids.len()),
        "Confirm sentiment change, document root cause, and create follow-up tasks for at-risk accounts."
            .to_string(),
        None,
        nps_ids,
    ))
}

fn build_competitive_pressure_insight(decisions: &[Decision]) -> Option<Insight> {
    let related = decisions
        .iter()
        .filter(|decision| {
            decision
                .tags
                .iter()
                .any(|tag| tag == "risk:competitive" || tag == "topic:competition")
        })
        .map(|decision| decision.id)
        .collect::<Vec<_>>();

    if related.len() < 5 {
        return None;
    }

    let related_set = related.iter().copied().collect::<HashSet<_>>();
    let affected_accounts = decisions
        .iter()
        .filter(|decision| related_set.contains(&decision.id))
        .filter_map(|decision| tag_value(decision, "account:").map(str::to_string))
        .collect::<HashSet<_>>()
        .len();

    Some(build_insight(
        "competitive_pressure_risk",
        "manager",
        if affected_accounts >= 3 { "high" } else { "medium" },
        format!("{} accounts show active competitive pressure", affected_accounts),
        "Coordinate account-level competitive defense plans before procurement and renewal decisions are finalized."
            .to_string(),
        Some(format!("{} signals include competitive risk indicators", related.len())),
        related,
    ))
}

fn build_discovery_quality_gap_insight(decisions: &[Decision]) -> Option<Insight> {
    let related = decisions
        .iter()
        .filter(|decision| decision.tags.iter().any(|tag| tag == "risk:discovery_gap"))
        .map(|decision| decision.id)
        .collect::<Vec<_>>();

    if related.len() < 8 {
        return None;
    }

    Some(build_insight(
        "discovery_quality_gap",
        "csm",
        if related.len() >= 20 { "high" } else { "medium" },
        format!("{} calls show weak discovery quality", related.len()),
        "Improve discovery quality on high-risk accounts before committing to mitigation plans."
            .to_string(),
        Some("Elevated rep talk-ratio correlated with unresolved blockers".to_string()),
        related,
    ))
}

fn build_expansion_momentum_insight(decisions: &[Decision]) -> Option<Insight> {
    let related = decisions
        .iter()
        .filter(|decision| {
            decision.tags.iter().any(|tag| {
                tag == "opportunity:expansion"
                    || tag == "call_outcome:expansion"
                    || tag == "call_outcome:won"
                    || tag == "nps:promoter"
            })
        })
        .map(|decision| decision.id)
        .collect::<Vec<_>>();

    if related.len() < 6 {
        return None;
    }

    let related_set = related.iter().copied().collect::<HashSet<_>>();
    let account_count = decisions
        .iter()
        .filter(|decision| related_set.contains(&decision.id))
        .filter_map(|decision| tag_value(decision, "account:").map(str::to_string))
        .collect::<HashSet<_>>()
        .len();

    Some(build_insight(
        "expansion_momentum",
        "manager",
        if account_count >= 4 { "high" } else { "medium" },
        format!("{} accounts show clear expansion momentum", account_count),
        "Convert positive adoption and sentiment signals into named expansion plans with quantified value."
            .to_string(),
        Some(format!(
            "{} expansion-leaning signals are active",
            related.len()
        )),
        related,
    ))
}

fn build_renewal_window_risk_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut by_account: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();
    let mut near_term = 0usize;
    let mut related = Vec::new();

    for decision in decisions {
        if !is_risk_signal(decision) {
            continue;
        }
        let Some(window) = tag_value(decision, "renewal_window:") else {
            continue;
        };
        if window != "0-30" && window != "31-90" {
            continue;
        }
        if window == "0-30" {
            near_term += 1;
        }
        if let Some(account) = tag_value(decision, "account:") {
            by_account
                .entry(account.to_string())
                .or_default()
                .push(decision.id);
        }
        related.push(decision.id);
    }

    if by_account.is_empty() {
        return None;
    }

    let mut ranked = by_account.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let top_accounts = ranked
        .iter()
        .take(3)
        .map(|(account, ids)| format!("{account} ({})", ids.len()))
        .collect::<Vec<_>>()
        .join(", ");

    let priority = if near_term > 0 { "high" } else { "medium" };
    Some(build_insight(
        "renewal_window_risk",
        "manager",
        priority,
        format!("{} accounts have open risk in the next 90 days", ranked.len()),
        format!(
            "Run renewal risk huddles for: {top_accounts}. Assign mitigation owners and customer checkpoint dates this week."
        ),
        Some(format!(
            "{} risk signals in 0-90 day renewal window",
            related.len()
        )),
        related,
    ))
}

fn build_arr_exposure_risk_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut per_account: HashMap<String, (u64, Vec<uuid::Uuid>)> = HashMap::new();

    for decision in decisions {
        if !is_risk_signal(decision) {
            continue;
        }
        let Some(account) = tag_value(decision, "account:") else {
            continue;
        };
        let arr = tag_value(decision, "arr:")
            .and_then(parse_arr_to_dollars)
            .unwrap_or(0);
        let entry = per_account
            .entry(account.to_string())
            .or_insert((0, Vec::new()));
        entry.0 = entry.0.max(arr);
        entry.1.push(decision.id);
    }

    if per_account.len() < 2 {
        return None;
    }

    let mut ranked = per_account.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let total_arr = ranked.iter().map(|(_, (arr, _))| *arr).sum::<u64>();
    if total_arr < 500_000 {
        return None;
    }

    let top_accounts = ranked
        .iter()
        .take(3)
        .map(|(account, (arr, _))| format!("{account} ({})", format_compact_currency(*arr)))
        .collect::<Vec<_>>()
        .join(", ");

    let related = ranked
        .iter()
        .take(4)
        .flat_map(|(_, (_, ids))| ids.iter().copied())
        .collect::<Vec<_>>();

    Some(build_insight(
        "arr_exposure_risk",
        "manager",
        "high",
        format!("{} in ARR is currently exposed to risk", format_compact_currency(total_arr)),
        format!(
            "Prioritize mitigation for: {top_accounts}. Escalate blockers with direct revenue and renewal impact."
        ),
        Some(format!(
            "{} at-risk accounts with ARR metadata",
            ranked.len()
        )),
        related,
    ))
}

fn build_industry_cluster_risk_insight(decisions: &[Decision]) -> Option<Insight> {
    let mut by_industry: HashMap<String, Vec<uuid::Uuid>> = HashMap::new();
    for decision in decisions {
        if !is_risk_signal(decision) {
            continue;
        }
        let Some(industry) = tag_value(decision, "industry:") else {
            continue;
        };
        by_industry
            .entry(industry.to_string())
            .or_default()
            .push(decision.id);
    }

    let mut ranked = by_industry.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let (industry, ids) = ranked.into_iter().next()?;
    if ids.len() < 4 {
        return None;
    }

    Some(build_insight(
        "industry_cluster_risk",
        "manager",
        "medium",
        format!("Repeated blockers are concentrated in {}", industry.to_uppercase()),
        format!(
            "Launch a focused {} response plan to address recurring blockers and reduce repeat escalations.",
            industry
        ),
        Some(format!("{} risk signals mapped to industry", ids.len())),
        ids,
    ))
}

fn build_severity_burden_insight(decisions: &[Decision]) -> Option<Insight> {
    let recent_cutoff = Utc::now() - Duration::days(14);
    let related = decisions
        .iter()
        .filter(|decision| {
            decision.updated_at >= recent_cutoff
                && (decision.tags.iter().any(|tag| tag == "severity:sev1")
                    || decision.tags.iter().any(|tag| tag == "severity:sev2"))
        })
        .map(|decision| decision.id)
        .collect::<Vec<_>>();

    if related.len() < 3 {
        return None;
    }

    Some(build_insight(
        "severity_burden",
        "manager",
        "high",
        format!("{} high-severity issues in the last 14 days", related.len()),
        "Run incident command for repeated sev1/sev2 patterns and tie fixes to customer impact timelines."
            .to_string(),
        Some("Recent severity trend: elevated".to_string()),
        related,
    ))
}

fn build_evidence_confidence_gap_insight(decisions: &[Decision]) -> Option<Insight> {
    let related = decisions
        .iter()
        .filter(|decision| {
            is_risk_signal(decision)
                && (decision.source_systems.len() <= 1
                    || decision
                        .tags
                        .iter()
                        .any(|tag| tag == "evidence:single_source"))
        })
        .map(|decision| decision.id)
        .collect::<Vec<_>>();

    if related.len() < 4 {
        return None;
    }

    Some(build_insight(
        "evidence_confidence_gap",
        "manager",
        "medium",
        format!("{} risk records rely on thin evidence", related.len()),
        "Add corroborating sources (CRM, support, usage) before making irreversible prioritization decisions."
            .to_string(),
        Some("Evidence quality threshold not met".to_string()),
        related,
    ))
}

fn enrich_rule_insights(insights: &mut [Insight], decisions: &[Decision]) {
    if insights.is_empty() || decisions.is_empty() {
        return;
    }

    let decision_map = decisions
        .iter()
        .map(|decision| (decision.id, decision))
        .collect::<HashMap<_, _>>();
    let recent_cutoff = Utc::now() - Duration::days(14);
    let total = decisions.len();

    for insight in insights.iter_mut() {
        let related = insight
            .related_signal_ids
            .iter()
            .filter_map(|id| decision_map.get(id).copied())
            .collect::<Vec<_>>();

        if related.is_empty() {
            continue;
        }

        let recent_count = related
            .iter()
            .filter(|decision| decision.updated_at >= recent_cutoff)
            .count();
        let risk_count = related
            .iter()
            .filter(|decision| is_risk_signal(decision))
            .count();
        let source_count = unique_sources(&related);
        let top_sources = summarize_sources(&related, 3);
        let top_accounts = summarize_tag_dimension(&related, "account:", 3);
        let top_industries = summarize_tag_dimension(&related, "industry:", 2);
        let (total_arr_exposure, top_arr_accounts) = summarize_account_arr_exposure(&related, 3);

        if insight.rationale.is_none() {
            insight.rationale = Some(rule_rationale(
                &insight.category,
                related.len(),
                risk_count,
                &top_accounts,
            ));
        }

        if insight.confidence_explanation.is_none() {
            insight.confidence_explanation = Some(format!(
                "Rules confidence uses {} related signals ({:.0}% of active set), {:.0}% refreshed in last 14 days, and {} corroborating source(s).",
                related.len(),
                percentage(related.len(), total),
                percentage(recent_count, related.len()),
                source_count
            ));
        }

        if insight.evidence.is_empty() {
            let mut evidence = vec![format!(
                "This recommendation is based on {} records ({:.0}% of active set).",
                related.len(),
                percentage(related.len(), total)
            )];

            if !top_sources.is_empty() {
                evidence.push(format!(
                    "Primary evidence sources: {}.",
                    top_sources.join(", ")
                ));
            }

            if !top_accounts.is_empty() {
                evidence.push(format!(
                    "Most affected accounts: {}.",
                    top_accounts.join(", ")
                ));
            }

            if !top_industries.is_empty() {
                evidence.push(format!(
                    "Industry concentration observed in: {}.",
                    top_industries.join(", ")
                ));
            }

            match insight.category.as_str() {
                "renewal_window_risk" => {
                    let renewal_0_30 = count_tag_value(&related, "renewal_window:", "0-30");
                    let renewal_31_90 = count_tag_value(&related, "renewal_window:", "31-90");
                    if renewal_0_30 > 0 || renewal_31_90 > 0 {
                        evidence.push(format!(
                            "Renewal window split: {} in 0-30 days, {} in 31-90 days.",
                            renewal_0_30, renewal_31_90
                        ));
                    }
                    if total_arr_exposure > 0 {
                        evidence.push(format!(
                            "Estimated ARR tied to affected accounts: {}.",
                            format_compact_currency(total_arr_exposure)
                        ));
                    }
                    if risk_count > 0 {
                        evidence.push(format!(
                            "{} related records include explicit risk markers.",
                            risk_count
                        ));
                    }
                }
                "arr_exposure_risk" => {
                    if total_arr_exposure > 0 {
                        evidence.push(format!(
                            "Estimated total ARR represented in this cohort: {}.",
                            format_compact_currency(total_arr_exposure)
                        ));
                    }
                    if !top_arr_accounts.is_empty() {
                        let ranked = top_arr_accounts
                            .iter()
                            .map(|(account, arr)| {
                                format!("{account} ({})", format_compact_currency(*arr))
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        evidence.push(format!("Largest ARR concentration: {ranked}."));
                    }
                }
                "severity_burden" => {
                    let sev1_count = count_tag_exact(&related, "severity:sev1");
                    let sev2_count = count_tag_exact(&related, "severity:sev2");
                    evidence.push(format!(
                        "Severity mix in this cohort: {} sev1 and {} sev2 records.",
                        sev1_count, sev2_count
                    ));
                }
                "expansion_momentum" => {
                    let expansion_count = related
                        .iter()
                        .filter(|decision| {
                            decision.tags.iter().any(|tag| {
                                tag == "opportunity:expansion"
                                    || tag == "call_outcome:expansion"
                                    || tag == "call_outcome:won"
                                    || tag == "nps:promoter"
                            })
                        })
                        .count();
                    let promoter_count = count_tag_exact(&related, "nps:promoter");
                    let won_count = count_tag_exact(&related, "call_outcome:won");
                    if expansion_count > 0 {
                        evidence.push(format!(
                            "{} related records carry explicit expansion indicators.",
                            expansion_count
                        ));
                    }
                    if promoter_count > 0 || won_count > 0 {
                        evidence.push(format!(
                            "Quality of momentum: {} promoter signals and {} won-outcome calls.",
                            promoter_count, won_count
                        ));
                    }
                    if risk_count > 0 {
                        evidence.push(format!(
                            "{} related records also include risk markers that may slow conversion.",
                            risk_count
                        ));
                    }
                }
                _ => {
                    if risk_count > 0 {
                        evidence.push(format!(
                            "{} related records include explicit risk markers.",
                            risk_count
                        ));
                    }
                }
            }

            let sample_titles = related
                .iter()
                .take(2)
                .map(|decision| truncate_label(&decision.title, 72))
                .collect::<Vec<_>>();
            if !sample_titles.is_empty() {
                evidence.push(format!(
                    "Example signal(s): {}.",
                    sample_titles.join(" | ")
                ));
            }

            insight.evidence = evidence;
        }
    }
}

fn rule_rationale(
    category: &str,
    related_count: usize,
    risk_count: usize,
    accounts: &[String],
) -> String {
    let account_focus = accounts
        .first()
        .cloned()
        .unwrap_or_else(|| "priority accounts".to_string());

    match category {
        "renewal_window_risk" => format!(
            "Near-term renewals contain sustained risk activity; immediate mitigation on {account_focus} can reduce revenue exposure."
        ),
        "arr_exposure_risk" => format!(
            "Risk signals are concentrated on higher-value accounts, so execution speed directly affects retained ARR."
        ),
        "competitive_pressure_risk" => format!(
            "Competitive pressure is recurring across active opportunities, so value defense is needed before commercial decisions."
        ),
        "severity_burden" => format!(
            "High-severity signal volume remains elevated, creating direct customer-impact risk and higher escalation probability."
        ),
        "account_signal_hotspots" => format!(
            "Signal clustering is strongest around {account_focus}; focused intervention there should improve adoption and renewal confidence fastest."
        ),
        "nps_follow_up_queue" => format!(
            "Recent NPS/voice-of-customer signals require fast follow-up to prevent sentiment degradation from becoming churn risk."
        ),
        "discovery_quality_gap" => format!(
            "Discovery markers suggest action plans may be based on incomplete customer context, increasing failure risk."
        ),
        "expansion_momentum" => format!(
            "Positive usage and sentiment signals indicate near-term expansion potential that should be converted into named plans."
        ),
        "evidence_confidence_gap" => format!(
            "A large portion of risk insights rely on thin corroboration; strengthening evidence quality will improve prioritization accuracy."
        ),
        _ => {
            if risk_count > 0 {
                format!(
                    "This pattern links to {risk_count} risk-tagged signals and should be treated as an operational priority."
                )
            } else {
                format!(
                    "This pattern is supported by {related_count} related signals and is material enough to warrant coordinated action."
                )
            }
        }
    }
}

fn unique_sources(decisions: &[&Decision]) -> usize {
    decisions
        .iter()
        .flat_map(|decision| decision.source_systems.iter())
        .map(|source| source.to_ascii_uppercase())
        .collect::<HashSet<_>>()
        .len()
}

fn summarize_sources(decisions: &[&Decision], limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for decision in decisions {
        let mut local = HashSet::new();
        for source in &decision.source_systems {
            let key = source.to_ascii_uppercase();
            if local.insert(key.clone()) {
                *counts.entry(key).or_default() += 1;
            }
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(source, count)| format!("{source} ({count})"))
        .collect()
}

fn summarize_tag_dimension(decisions: &[&Decision], prefix: &str, limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for decision in decisions {
        let mut local = HashSet::new();
        for tag in &decision.tags {
            if let Some(value) = tag.strip_prefix(prefix) {
                let normalized = value.trim().to_string();
                if !normalized.is_empty() && local.insert(normalized.clone()) {
                    *counts.entry(normalized).or_default() += 1;
                }
            }
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(value, count)| format!("{value} ({count})"))
        .collect()
}

fn summarize_account_arr_exposure(decisions: &[&Decision], limit: usize) -> (u64, Vec<(String, u64)>) {
    let mut arr_by_account: HashMap<String, u64> = HashMap::new();
    for decision in decisions {
        let Some(account) = tag_value(decision, "account:") else {
            continue;
        };
        let Some(arr) = tag_value(decision, "arr:").and_then(parse_arr_to_dollars) else {
            continue;
        };
        arr_by_account
            .entry(account.to_string())
            .and_modify(|current| *current = (*current).max(arr))
            .or_insert(arr);
    }

    let mut ranked = arr_by_account.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total_arr = ranked.iter().map(|(_, arr)| *arr).sum::<u64>();
    (total_arr, ranked.into_iter().take(limit).collect())
}

fn count_tag_exact(decisions: &[&Decision], tag: &str) -> usize {
    decisions
        .iter()
        .filter(|decision| decision.tags.iter().any(|candidate| candidate == tag))
        .count()
}

fn count_tag_value(decisions: &[&Decision], prefix: &str, value: &str) -> usize {
    decisions
        .iter()
        .filter(|decision| decision.tags.iter().any(|candidate| candidate == &format!("{prefix}{value}")))
        .count()
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn build_story_dataset() -> Vec<NewDecisionInput> {
    let now = Utc::now();
    let mut records = Vec::new();

    let mut push = |title: &str,
                    summary: &str,
                    owner: Option<&str>,
                    status: DecisionStatus,
                    sources: &[&str],
                    tags: Vec<String>,
                    days_ago: i64| {
        let updated_at = now - Duration::days(days_ago.max(0));
        let created_at = updated_at - Duration::days(18);

        records.push(NewDecisionInput {
            title: title.to_string(),
            summary: summary.to_string(),
            owner: owner.map(|value| value.to_string()),
            source_systems: sources.iter().map(|value| value.to_string()).collect(),
            tags,
            status: Some(status),
            created_at: Some(created_at),
            updated_at: Some(updated_at),
        });
    };

    let mut push_story = |account: &str,
                          industry: &str,
                          segment: &str,
                          region: &str,
                          arr: &str,
                          renewal_window: &str,
                          lifecycle: &str,
                          title: &str,
                          summary: &str,
                          owner: Option<&str>,
                          status: DecisionStatus,
                          sources: &[&str],
                          extra_tags: &[&str],
                          days_ago: i64| {
        let mut tags = vec![
            format!("account:{account}"),
            format!("industry:{industry}"),
            format!("segment:{segment}"),
            format!("region:{region}"),
            format!("arr:{arr}"),
            format!("renewal_window:{renewal_window}"),
            format!("lifecycle:{lifecycle}"),
        ];
        tags.extend(extra_tags.iter().map(|value| value.to_string()));
        push(title, summary, owner, status, sources, tags, days_ago);
    };

    // SaaS enterprise: Northstar Tech (at-risk renewal with active plan)
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "adoption",
        "Northstar admin activation dropped after permissions rollout",
        "Activation for enterprise admins declined from 82% to 61% after role policy changes in release 24.3.",
        Some("Anna CSM"),
        DecisionStatus::Proposed,
        &["Gong", "Gainsight"],
        &["risk:adoption", "severity:sev2", "metric:activation_delta_-21"],
        2,
    );
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "renewal",
        "Northstar executive sponsor requested formal recovery timeline",
        "Sponsor asked for signed remediation milestones before renewal committee review.",
        Some("Sam Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Gong"],
        &["risk:renewal", "stakeholder:exec", "contract_phase:review"],
        3,
    );
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "sentiment",
        "Northstar NPS detractor cited reporting latency",
        "Customer submitted NPS 4/10 and referenced dashboard latency during executive board prep.",
        Some("Anna CSM"),
        DecisionStatus::Proposed,
        &["Gong"],
        &[
            "event:nps_submitted",
            "nps:detractor",
            "risk:sentiment",
            "evidence:single_source",
        ],
        1,
    );
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "security",
        "Northstar SSO cutover blocked by certificate control review",
        "Identity team paused SSO go-live pending certificate rotation and SCIM scope approval.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &[
            "risk:security",
            "severity:sev1",
            "theme:sso",
            "evidence:single_source",
        ],
        2,
    );
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "renewal",
        "Northstar recovery plan accepted with weekly checkpoints",
        "Customer accepted weekly checkpoint cadence and committed platform + success leads.",
        Some("Anna CSM"),
        DecisionStatus::Approved,
        &["Notion", "Salesforce"],
        &["plan:recovery", "trend:improving"],
        8,
    );

    // Healthcare enterprise: Meridian Health (compliance-heavy renewal)
    push_story(
        "meridian_health",
        "healthcare",
        "enterprise",
        "na",
        "840k",
        "0-30",
        "renewal",
        "Meridian legal blocked renewal pending PHI audit evidence",
        "Procurement paused final signature until audit evidence package is delivered.",
        Some("Priya Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Zendesk"],
        &[
            "risk:renewal",
            "risk:compliance",
            "severity:sev1",
            "contract_phase:signature",
        ],
        2,
    );
    push_story(
        "meridian_health",
        "healthcare",
        "enterprise",
        "na",
        "840k",
        "0-30",
        "security",
        "Meridian requested updated HIPAA controls appendix",
        "Security questionnaire reopened for encryption-at-rest and key management controls.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &[
            "risk:security",
            "risk:compliance",
            "severity:sev2",
            "evidence:single_source",
        ],
        3,
    );
    push_story(
        "meridian_health",
        "healthcare",
        "enterprise",
        "na",
        "840k",
        "0-30",
        "support",
        "Meridian escalation queue exceeded SLA for critical incidents",
        "Critical support escalations rose from 2 to 9 this week impacting clinical reporting.",
        Some("Platform Reliability"),
        DecisionStatus::Approved,
        &["Zendesk", "Jira"],
        &["risk:support", "risk:stability", "severity:sev1"],
        1,
    );
    push_story(
        "meridian_health",
        "healthcare",
        "enterprise",
        "na",
        "840k",
        "0-30",
        "adoption",
        "Meridian onboarding wave missed first-value milestone",
        "New operations cohort missed day-14 first-value milestone by 6 days.",
        None,
        DecisionStatus::Proposed,
        &["Gong", "Gainsight"],
        &["risk:onboarding", "risk:adoption"],
        4,
    );

    // Fintech enterprise: Atlas Pay (high ARR, incident and compliance risk)
    push_story(
        "atlas_pay",
        "fintech",
        "enterprise",
        "emea",
        "910k",
        "31-90",
        "stability",
        "AtlasPay payment API incident breached uptime objective",
        "Payment API error rate stayed above 3.1% for 18 hours before rollback.",
        Some("Platform Reliability"),
        DecisionStatus::Approved,
        &["Jira", "Datadog"],
        &["risk:stability", "incident", "severity:sev1"],
        2,
    );
    push_story(
        "atlas_pay",
        "fintech",
        "enterprise",
        "emea",
        "910k",
        "31-90",
        "renewal",
        "AtlasPay renewal forecast downgraded from Commit to Risk",
        "Finance stakeholder requested remediation proof before approving multi-year term.",
        Some("Sam Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Gong"],
        &["risk:renewal", "risk:value", "forecast:risk"],
        3,
    );
    push_story(
        "atlas_pay",
        "fintech",
        "enterprise",
        "emea",
        "910k",
        "31-90",
        "security",
        "AtlasPay regulator readiness review opened",
        "Customer requested control mapping for PSD2 incident response and privileged access.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &[
            "risk:compliance",
            "risk:security",
            "severity:sev2",
            "evidence:single_source",
        ],
        5,
    );
    push_story(
        "atlas_pay",
        "fintech",
        "enterprise",
        "emea",
        "910k",
        "31-90",
        "expansion",
        "AtlasPay expansion paused until reliability KPI improves",
        "Expansion committee deferred additional module purchase pending 30-day incident-free period.",
        Some("Priya Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Gainsight"],
        &["risk:expansion", "risk:stability"],
        6,
    );

    // Logistics mid-market: BlueFreight (mixed quality signals)
    push_story(
        "bluefreight",
        "logistics",
        "mid_market",
        "apac",
        "280k",
        "91-180",
        "adoption",
        "BlueFreight dispatch team adoption plateaued",
        "Dispatch cohort weekly actives stalled at 47% versus 65% target.",
        Some("Dana CSM"),
        DecisionStatus::Proposed,
        &["Gong", "Gainsight"],
        &["risk:adoption", "severity:sev2"],
        5,
    );
    push_story(
        "bluefreight",
        "logistics",
        "mid_market",
        "apac",
        "280k",
        "91-180",
        "support",
        "BlueFreight support backlog exceeded SLA",
        "Priority queue reached 13 unresolved tickets after new routing workflow rollout.",
        None,
        DecisionStatus::Proposed,
        &["Zendesk"],
        &["risk:support", "severity:sev2", "evidence:single_source"],
        2,
    );
    push_story(
        "bluefreight",
        "logistics",
        "mid_market",
        "apac",
        "280k",
        "91-180",
        "sentiment",
        "BlueFreight NPS passive requested faster analytics exports",
        "NPS 7/10 response noted friction in finance export workflow during monthly close.",
        Some("Dana CSM"),
        DecisionStatus::Proposed,
        &["Gong"],
        &[
            "event:nps_submitted",
            "nps:passive",
            "risk:sentiment",
            "evidence:single_source",
        ],
        4,
    );
    push_story(
        "bluefreight",
        "logistics",
        "mid_market",
        "apac",
        "280k",
        "91-180",
        "expansion",
        "BlueFreight requested pilot for optimization module",
        "Operations director requested pilot scope for route optimization add-on.",
        Some("Dana CSM"),
        DecisionStatus::Approved,
        &["Salesforce", "Gong"],
        &["opportunity:expansion", "trend:improving"],
        7,
    );

    // Manufacturing enterprise: IronForge (operational friction, but improving)
    push_story(
        "ironforge",
        "manufacturing",
        "enterprise",
        "na",
        "460k",
        "91-180",
        "adoption",
        "IronForge plant leaders reported workflow complexity",
        "Site leads reported delayed job completion due to too many approval hops.",
        Some("Mia CSM"),
        DecisionStatus::Proposed,
        &["Gong", "Notion"],
        &["risk:adoption", "risk:value"],
        6,
    );
    push_story(
        "ironforge",
        "manufacturing",
        "enterprise",
        "na",
        "460k",
        "91-180",
        "support",
        "IronForge incident queue normalized after hotfix",
        "Backlog cleared and incident response SLA returned to baseline.",
        Some("Platform Reliability"),
        DecisionStatus::Superseded,
        &["Jira", "Zendesk"],
        &["incident", "closed_loop", "trend:improving"],
        42,
    );
    push_story(
        "ironforge",
        "manufacturing",
        "enterprise",
        "na",
        "460k",
        "91-180",
        "expansion",
        "IronForge signed add-on for predictive maintenance",
        "Expansion approved after proving 12% reduction in unplanned downtime.",
        Some("Mia CSM"),
        DecisionStatus::Approved,
        &["Salesforce", "Gong"],
        &["opportunity:expansion", "trend:improving"],
        5,
    );

    // Public sector enterprise: CivicCloud (renewal and procurement complexity)
    push_story(
        "civiccloud",
        "public_sector",
        "enterprise",
        "na",
        "730k",
        "0-30",
        "renewal",
        "CivicCloud procurement flagged budget re-approval risk",
        "Renewal requires council budget amendment approval before contract can be executed.",
        Some("Priya Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Gong"],
        &["risk:renewal", "risk:procurement", "severity:sev2"],
        2,
    );
    push_story(
        "civiccloud",
        "public_sector",
        "enterprise",
        "na",
        "730k",
        "0-30",
        "security",
        "CivicCloud requested CJIS control attestation refresh",
        "Security office requested updated CJIS control mapping and evidence links.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &[
            "risk:compliance",
            "risk:security",
            "severity:sev1",
            "evidence:single_source",
        ],
        4,
    );
    push_story(
        "civiccloud",
        "public_sector",
        "enterprise",
        "na",
        "730k",
        "0-30",
        "sentiment",
        "CivicCloud detractor response cited response-time variance",
        "NPS 5/10 noted inconsistent response times during major incident windows.",
        Some("Nia CSM"),
        DecisionStatus::Proposed,
        &["Gong", "Zendesk"],
        &["event:nps_submitted", "nps:detractor", "risk:sentiment"],
        1,
    );
    push_story(
        "civiccloud",
        "public_sector",
        "enterprise",
        "na",
        "730k",
        "0-30",
        "delivery",
        "CivicCloud implementation milestone reopened",
        "Data migration acceptance failed and final sign-off moved by two weeks.",
        None,
        DecisionStatus::Proposed,
        &["Jira", "Notion"],
        &["risk:delivery", "severity:sev2"],
        3,
    );

    // Cross-account duplicates to exercise dedup analytics.
    push_story(
        "northstar_tech",
        "saas",
        "enterprise",
        "na",
        "520k",
        "31-90",
        "stability",
        "Shared dashboard latency issue escalated by enterprise accounts",
        "Northstar reported reporting-window timeouts during board prep.",
        Some("Platform Reliability"),
        DecisionStatus::Proposed,
        &["Jira"],
        &["risk:stability", "severity:sev2", "theme:shared_latency"],
        2,
    );
    push_story(
        "atlas_pay",
        "fintech",
        "enterprise",
        "emea",
        "910k",
        "31-90",
        "stability",
        "Shared dashboard latency issue escalated by enterprise accounts",
        "AtlasPay observed the same latency profile during end-of-month reconciliation.",
        Some("Platform Reliability"),
        DecisionStatus::Proposed,
        &["Jira"],
        &["risk:stability", "severity:sev2", "theme:shared_latency"],
        2,
    );
    push_story(
        "civiccloud",
        "public_sector",
        "enterprise",
        "na",
        "730k",
        "0-30",
        "stability",
        "Shared dashboard latency issue escalated by enterprise accounts",
        "CivicCloud noted dashboard timeout variance during council reporting cycle.",
        Some("Platform Reliability"),
        DecisionStatus::Proposed,
        &["Jira", "Zendesk"],
        &["risk:stability", "severity:sev2", "theme:shared_latency"],
        3,
    );

    // Retail enterprise: Harbor Retail (checkout reliability + QBR pressure)
    push_story(
        "harbor_retail",
        "retail",
        "enterprise",
        "na",
        "610k",
        "31-90",
        "value_realization",
        "HarborRetail checkout analytics lag impacted exec QBR",
        "Executive team could not complete weekly margin review due to delayed checkout analytics.",
        Some("Luis CSM"),
        DecisionStatus::Proposed,
        &["Looker", "Jira", "Salesforce"],
        &[
            "risk:value",
            "risk:analytics",
            "severity:sev2",
            "theme:reporting_latency",
        ],
        2,
    );
    push_story(
        "harbor_retail",
        "retail",
        "enterprise",
        "na",
        "610k",
        "31-90",
        "renewal",
        "HarborRetail renewal committee requested success KPI evidence pack",
        "Procurement requested KPI proof for shelf-availability gains before approving expansion term.",
        Some("Sam Renewals"),
        DecisionStatus::Proposed,
        &["Salesforce", "Gong"],
        &["risk:renewal", "stakeholder:procurement", "contract_phase:review"],
        4,
    );
    push_story(
        "harbor_retail",
        "retail",
        "enterprise",
        "na",
        "610k",
        "31-90",
        "support",
        "HarborRetail ticket backlog crossed red threshold",
        "Store operations raised 17 unresolved P1/P2 tickets after checkout workflow redesign.",
        None,
        DecisionStatus::Proposed,
        &["Zendesk"],
        &["risk:support", "severity:sev2", "evidence:single_source"],
        3,
    );

    // Telecom enterprise: NovaTelco (high ARR + active security blockers)
    push_story(
        "nova_telco",
        "telecom",
        "enterprise",
        "emea",
        "1.2m",
        "0-30",
        "security",
        "NovaTelco security review blocked production extension",
        "Customer security office halted renewal extension pending privileged access audit artifacts.",
        Some("Security Operations"),
        DecisionStatus::Proposed,
        &["Jira", "Notion"],
        &["risk:security", "risk:compliance", "severity:sev1", "contract_phase:signature"],
        2,
    );
    push_story(
        "nova_telco",
        "telecom",
        "enterprise",
        "emea",
        "1.2m",
        "0-30",
        "stability",
        "NovaTelco API throttling caused failed provisioning batches",
        "Provisioning retries climbed 4.4x during peak hours and delayed enterprise activations.",
        Some("Platform Reliability"),
        DecisionStatus::Approved,
        &["Datadog", "Jira"],
        &["risk:stability", "severity:sev1", "incident"],
        1,
    );
    push_story(
        "nova_telco",
        "telecom",
        "enterprise",
        "emea",
        "1.2m",
        "0-30",
        "sentiment",
        "NovaTelco detractor cited delayed provisioning and sparse status updates",
        "NPS 3/10 highlighted communication gaps during service degradation.",
        Some("Nia CSM"),
        DecisionStatus::Proposed,
        &["Gong"],
        &[
            "event:nps_submitted",
            "nps:detractor",
            "risk:sentiment",
            "evidence:single_source",
        ],
        1,
    );

    // Education mid-market: Summit University (adoption + onboarding quality)
    push_story(
        "summit_university",
        "education",
        "mid_market",
        "na",
        "190k",
        "91-180",
        "onboarding",
        "Summit University onboarding completion under target for faculty cohort",
        "Only 42% of faculty completed the first-workflow milestone within 21 days.",
        Some("Dana CSM"),
        DecisionStatus::Proposed,
        &["Gong", "Gainsight"],
        &["risk:onboarding", "risk:adoption", "severity:sev2"],
        5,
    );
    push_story(
        "summit_university",
        "education",
        "mid_market",
        "na",
        "190k",
        "91-180",
        "support",
        "Summit University gradebook sync issue reopened",
        "Resolved incident reappeared after LMS patch and created grading delays.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &[
            "risk:stability",
            "risk:support",
            "severity:sev2",
            "evidence:single_source",
        ],
        6,
    );
    push_story(
        "summit_university",
        "education",
        "mid_market",
        "na",
        "190k",
        "91-180",
        "expansion",
        "Summit University considering analytics add-on after pilot",
        "Dean approved continuation review if faculty weekly active usage sustains above 60%.",
        Some("Dana CSM"),
        DecisionStatus::Approved,
        &["Salesforce", "Gong"],
        &["opportunity:expansion", "trend:improving"],
        9,
    );

    // Energy enterprise: Aegis Energy (compliance + deployment velocity)
    push_story(
        "aegis_energy",
        "energy",
        "enterprise",
        "na",
        "680k",
        "31-90",
        "compliance",
        "Aegis Energy requested SOC2 bridge letter and audit scope clarification",
        "Risk office requested additional auditor attestation details before renewal forecast signoff.",
        Some("Security Operations"),
        DecisionStatus::Proposed,
        &["Salesforce", "Notion"],
        &["risk:compliance", "risk:renewal", "severity:sev2"],
        4,
    );
    push_story(
        "aegis_energy",
        "energy",
        "enterprise",
        "na",
        "680k",
        "31-90",
        "delivery",
        "Aegis Energy deployment sequence slipped two sprints",
        "Integration dependencies delayed production rollout by 16 days and impacted training timeline.",
        None,
        DecisionStatus::Proposed,
        &["Jira"],
        &["risk:delivery", "risk:adoption", "severity:sev2", "evidence:single_source"],
        3,
    );
    push_story(
        "aegis_energy",
        "energy",
        "enterprise",
        "na",
        "680k",
        "31-90",
        "adoption",
        "Aegis Energy field operations adoption improved after workflow simplification",
        "Field team weekly active usage increased from 49% to 66% after template redesign.",
        Some("Mia CSM"),
        DecisionStatus::Approved,
        &["Gong", "Gainsight"],
        &["trend:improving", "closed_loop"],
        7,
    );

    // Media SMB: Pulse Media (growth account with mixed signal quality)
    push_story(
        "pulse_media",
        "media",
        "smb",
        "emea",
        "120k",
        "181-365",
        "growth",
        "Pulse Media requested API usage quota increase for campaign season",
        "Marketing team expects 2.3x traffic and requested pre-approved scale path.",
        Some("Nia CSM"),
        DecisionStatus::Approved,
        &["Salesforce", "Gong"],
        &["opportunity:expansion", "growth_signal", "trend:improving"],
        6,
    );
    push_story(
        "pulse_media",
        "media",
        "smb",
        "emea",
        "120k",
        "181-365",
        "support",
        "Pulse Media reported delayed campaign attribution exports",
        "Campaign attribution exports breached SLA during quarter-end analytics load.",
        None,
        DecisionStatus::Proposed,
        &["Zendesk"],
        &["risk:support", "severity:sev2", "evidence:single_source"],
        2,
    );

    records
}

fn is_risk_signal(decision: &Decision) -> bool {
    if matches!(decision.status, DecisionStatus::Superseded) {
        return false;
    }

    decision.tags.iter().any(|tag| {
        tag.starts_with("risk:")
            || tag == "severity:sev1"
            || tag == "severity:sev2"
            || tag == "nps:detractor"
            || tag == "forecast:risk"
    })
}

fn tag_value<'a>(decision: &'a Decision, prefix: &str) -> Option<&'a str> {
    decision
        .tags
        .iter()
        .find_map(|tag| tag.strip_prefix(prefix).map(str::trim))
        .filter(|value| !value.is_empty())
}

fn parse_arr_to_dollars(value: &str) -> Option<u64> {
    let mut normalized = value.trim().to_ascii_lowercase();
    normalized.retain(|ch| !ch.is_ascii_whitespace() && ch != '$' && ch != ',');
    if normalized.is_empty() {
        return None;
    }

    let multiplier = if normalized.ends_with('k') {
        normalized.pop();
        1_000f64
    } else if normalized.ends_with('m') {
        normalized.pop();
        1_000_000f64
    } else if normalized.ends_with('b') {
        normalized.pop();
        1_000_000_000f64
    } else {
        1f64
    };

    let base = normalized.parse::<f64>().ok()?;
    if !base.is_finite() || base <= 0.0 {
        return None;
    }

    Some((base * multiplier).round() as u64)
}

fn format_compact_currency(amount: u64) -> String {
    let (scaled, suffix) = if amount >= 1_000_000_000 {
        (amount as f64 / 1_000_000_000.0, "B")
    } else if amount >= 1_000_000 {
        (amount as f64 / 1_000_000.0, "M")
    } else if amount >= 1_000 {
        (amount as f64 / 1_000.0, "K")
    } else {
        (amount as f64, "")
    };

    if suffix.is_empty() {
        format!("${amount}")
    } else if scaled >= 10.0 {
        format!("${scaled:.0}{suffix}")
    } else {
        let rounded = format!("{scaled:.1}");
        let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
        format!("${trimmed}{suffix}")
    }
}

fn normalize_title(value: &str) -> String {
    let mut normalized = String::new();
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
            normalized.push(ch);
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn percentage(part: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (part as f64 / total as f64) * 100.0
}

fn unique_uuids(ids: Vec<uuid::Uuid>) -> Vec<uuid::Uuid> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(*id))
        .collect::<Vec<_>>()
}

fn normalize_new_decision_input(input: NewDecisionInput) -> ApplicationResult<NewDecisionInput> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(ApplicationError::validation("title is required"));
    }
    if title.chars().count() > 120 {
        return Err(ApplicationError::validation(
            "title must be 120 characters or fewer",
        ));
    }

    let summary = input.summary.trim().to_string();
    if summary.is_empty() {
        return Err(ApplicationError::validation("summary is required"));
    }
    if summary.chars().count() > 4000 {
        return Err(ApplicationError::validation(
            "summary must be 4000 characters or fewer",
        ));
    }

    let owner = normalize_optional_text(input.owner);

    let source_systems = normalize_unique_string_list(input.source_systems, false);
    if source_systems.is_empty() {
        return Err(ApplicationError::validation(
            "at least one source_system is required",
        ));
    }

    let tags = normalize_unique_string_list(input.tags, true);

    Ok(NewDecisionInput {
        title,
        summary,
        owner,
        source_systems,
        tags,
        status: input.status,
        created_at: input.created_at,
        updated_at: input.updated_at,
    })
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_unique_string_list(values: Vec<String>, lowercase_output: bool) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            if lowercase_output {
                out.push(trimmed.to_ascii_lowercase());
            } else {
                out.push(trimmed.to_string());
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use super::DecisionService;
    use crate::{
        application::{
            errors::ApplicationError,
            ports::{DecisionRepository, UpsertOutcome},
        },
        domain::models::{Decision, DecisionStatus, NewDecisionInput},
    };

    struct InMemoryTestRepo {
        items: Mutex<Vec<Decision>>,
    }

    impl InMemoryTestRepo {
        fn new(items: Vec<Decision>) -> Self {
            Self {
                items: Mutex::new(items),
            }
        }
    }

    #[async_trait]
    impl DecisionRepository for InMemoryTestRepo {
        async fn list(&self) -> anyhow::Result<Vec<Decision>> {
            Ok(self.items.lock().expect("lock poisoned").clone())
        }

        async fn create(&self, input: NewDecisionInput) -> anyhow::Result<Decision> {
            let now = Utc::now();
            let decision = Decision {
                id: Uuid::new_v4(),
                title: input.title,
                summary: input.summary,
                owner: input.owner,
                status: DecisionStatus::Proposed,
                source_systems: input.source_systems,
                tags: input.tags,
                created_at: now,
                updated_at: now,
            };

            self.items
                .lock()
                .expect("lock poisoned")
                .push(decision.clone());

            Ok(decision)
        }

        async fn delete_all(&self) -> anyhow::Result<usize> {
            let mut items = self.items.lock().expect("lock poisoned");
            let count = items.len();
            items.clear();
            Ok(count)
        }

        async fn upsert_by_title(&self, input: NewDecisionInput) -> anyhow::Result<UpsertOutcome> {
            let mut items = self.items.lock().expect("lock poisoned");
            if let Some(existing) = items.iter_mut().find(|item| item.title == input.title) {
                existing.summary = input.summary;
                if let Some(owner) = input.owner {
                    existing.owner = Some(owner);
                }
                existing.status = input.status.unwrap_or(DecisionStatus::Proposed);
                existing.source_systems = input.source_systems;
                existing.tags = input.tags;
                existing.updated_at = Utc::now();
                return Ok(UpsertOutcome::Updated);
            }

            let now = Utc::now();
            let decision = Decision {
                id: Uuid::new_v4(),
                title: input.title,
                summary: input.summary,
                owner: input.owner,
                status: input.status.unwrap_or(DecisionStatus::Proposed),
                source_systems: input.source_systems,
                tags: input.tags,
                created_at: input.created_at.unwrap_or(now),
                updated_at: input.updated_at.unwrap_or(now),
            };
            items.push(decision);
            Ok(UpsertOutcome::Created)
        }

        async fn bulk_assign_owner(
            &self,
            ids: Vec<Uuid>,
            owner: String,
            only_if_owner_missing: bool,
        ) -> anyhow::Result<usize> {
            let mut count = 0usize;
            let mut items = self.items.lock().expect("lock poisoned");
            for item in items.iter_mut() {
                if ids.contains(&item.id) && (!only_if_owner_missing || item.owner.is_none()) {
                    item.owner = Some(owner.clone());
                    count += 1;
                }
            }
            Ok(count)
        }

        async fn bulk_set_status(
            &self,
            ids: Vec<Uuid>,
            status: DecisionStatus,
        ) -> anyhow::Result<usize> {
            let mut count = 0usize;
            let mut items = self.items.lock().expect("lock poisoned");
            for item in items.iter_mut() {
                if ids.contains(&item.id) {
                    item.status = status.clone();
                    count += 1;
                }
            }
            Ok(count)
        }

        async fn bulk_add_tag(&self, ids: Vec<Uuid>, tag: String) -> anyhow::Result<usize> {
            let mut count = 0usize;
            let mut items = self.items.lock().expect("lock poisoned");
            for item in items.iter_mut() {
                if ids.contains(&item.id) && !item.tags.iter().any(|existing| existing == &tag) {
                    item.tags.push(tag.clone());
                    count += 1;
                }
            }
            Ok(count)
        }
    }

    fn build_service(items: Vec<Decision>) -> DecisionService {
        DecisionService::with_analytics(Arc::new(InMemoryTestRepo::new(items)), None)
    }

    fn build_decision(
        title: String,
        owner: Option<String>,
        status: DecisionStatus,
        source_systems: Vec<String>,
        tags: Vec<String>,
        updated_at: chrono::DateTime<Utc>,
    ) -> Decision {
        Decision {
            id: Uuid::new_v4(),
            title,
            summary: "Synthetic summary".to_string(),
            owner,
            status,
            source_systems,
            tags,
            created_at: updated_at,
            updated_at,
        }
    }

    #[tokio::test]
    async fn create_decision_rejects_blank_title() {
        let service = build_service(vec![]);
        let input = NewDecisionInput {
            title: "   ".to_string(),
            summary: "Summary".to_string(),
            owner: None,
            source_systems: vec!["Jira".to_string()],
            tags: vec![],
            status: None,
            created_at: None,
            updated_at: None,
        };

        let error = service
            .create_decision(input)
            .await
            .expect_err("validation should fail");

        assert!(matches!(error, ApplicationError::Validation(_)));
        assert_eq!(error.to_string(), "title is required");
    }

    #[tokio::test]
    async fn create_decision_normalizes_lists_and_owner() {
        let service = build_service(vec![]);
        let input = NewDecisionInput {
            title: "  Consolidate customer profile model  ".to_string(),
            summary: "  Remove duplicate profile records across systems.  ".to_string(),
            owner: Some("  Data Platform  ".to_string()),
            source_systems: vec![" Jira ".to_string(), "jira".to_string(), " ".to_string()],
            tags: vec![" Governance ".to_string(), "governance".to_string()],
            status: None,
            created_at: None,
            updated_at: None,
        };

        let created = service
            .create_decision(input)
            .await
            .expect("create should succeed");

        assert_eq!(created.title, "Consolidate customer profile model");
        assert_eq!(
            created.summary,
            "Remove duplicate profile records across systems."
        );
        assert_eq!(created.owner.as_deref(), Some("Data Platform"));
        assert_eq!(created.source_systems, vec!["Jira"]);
        assert_eq!(created.tags, vec!["governance"]);
    }

    #[tokio::test]
    async fn insights_include_missing_owners() {
        let now = Utc::now();
        let decision = Decision {
            id: Uuid::new_v4(),
            title: "Use canonical customer id".to_string(),
            summary: "Standardize identity joins.".to_string(),
            owner: None,
            status: DecisionStatus::Approved,
            source_systems: vec!["Salesforce".to_string()],
            tags: vec!["identity".to_string()],
            created_at: now,
            updated_at: now,
        };

        let service = build_service(vec![decision]);
        let insights = service
            .get_insights()
            .await
            .expect("insight generation should succeed");

        assert!(insights
            .iter()
            .any(|item| item.category == "missing_owners"));
    }

    #[tokio::test]
    async fn insights_include_owner_concentration_risk() {
        let now = Utc::now();
        let mut items = Vec::new();

        for index in 0..9 {
            items.push(build_decision(
                format!("Decision {index}"),
                Some("Platform Leadership".to_string()),
                DecisionStatus::Approved,
                vec!["Jira".to_string()],
                vec!["ops".to_string()],
                now - Duration::days((index % 5) as i64),
            ));
        }

        for index in 0..3 {
            items.push(build_decision(
                format!("Other Decision {index}"),
                Some("Data Ops".to_string()),
                DecisionStatus::Approved,
                vec!["Salesforce".to_string()],
                vec!["governance".to_string()],
                now,
            ));
        }

        let service = build_service(items);
        let insights = service
            .get_insights()
            .await
            .expect("insight generation should succeed");

        assert!(insights
            .iter()
            .any(|item| item.category == "owner_concentration_risk" && item.audience == "manager"));
    }

    #[tokio::test]
    async fn insights_include_csm_account_hotspots_and_nps_queue() {
        let now = Utc::now();
        let mut items = Vec::new();

        for index in 0..6 {
            let mut tags = vec!["gong".to_string(), "event:guide_seen".to_string()];
            if index % 2 == 0 {
                tags.push("event:nps_submitted".to_string());
            }
            tags.push("account:acme_corp".to_string());

            items.push(build_decision(
                format!("Gong signal {index}"),
                None,
                DecisionStatus::Proposed,
                vec!["Gong".to_string()],
                tags,
                now - Duration::days((index % 2) as i64),
            ));
        }

        let service = build_service(items);
        let insights = service
            .get_insights()
            .await
            .expect("insight generation should succeed");

        assert!(insights
            .iter()
            .any(|item| item.category == "account_signal_hotspots" && item.audience == "csm"));
        assert!(insights
            .iter()
            .any(|item| item.category == "nps_follow_up_queue" && item.audience == "csm"));
    }

    #[tokio::test]
    async fn insights_include_competitive_discovery_and_expansion_categories() {
        let now = Utc::now();
        let mut items = Vec::new();

        for index in 0..8 {
            items.push(build_decision(
                format!("Competitive risk {index}"),
                Some("CS Operations".to_string()),
                DecisionStatus::Proposed,
                vec!["Gong".to_string()],
                vec![
                    "account:acme_corp".to_string(),
                    "risk:competitive".to_string(),
                    "risk:discovery_gap".to_string(),
                ],
                now - Duration::days((index % 3) as i64),
            ));
        }

        for index in 0..6 {
            items.push(build_decision(
                format!("Expansion momentum {index}"),
                Some("CS Operations".to_string()),
                DecisionStatus::Approved,
                vec!["Gong".to_string()],
                vec![
                    "account:apex_inc".to_string(),
                    "opportunity:expansion".to_string(),
                    "nps:promoter".to_string(),
                ],
                now - Duration::days((index % 2) as i64),
            ));
        }

        let service = build_service(items);
        let insights = service
            .get_insights()
            .await
            .expect("insight generation should succeed");

        assert!(insights
            .iter()
            .any(|item| item.category == "competitive_pressure_risk"));
        assert!(insights
            .iter()
            .any(|item| item.category == "discovery_quality_gap"));
        assert!(insights
            .iter()
            .any(|item| item.category == "expansion_momentum"));
    }
}
