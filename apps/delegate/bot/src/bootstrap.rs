//! Bootstrap pipeline — three-stage onboarding for new teams.
//!
//! Stage 1 (onboarding): Guided ~8-question DM with the team lead.
//! Stage 2 (ingesting):  Background pull of Slack history + open tickets + linked docs.
//! Stage 3 (validating): Post a "here's what I think is happening" summary, capture corrections.
//!
//! After stage 3 completes, the state machine transitions to `Complete` and
//! the bot switches to normal operation. Heartbeat and cron are gated on
//! `Complete` — an unconfigured bot does not proactively reason or post.

use anyhow::Result;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::db::Db;
use crate::messenger::Messenger;
use crate::models::{CompleteOptions, ModelClient};
use crate::workspace::Workspace;

// ── State types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStage {
    Onboarding,
    Ingesting,
    Validating,
    Complete,
}

impl BootstrapStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Onboarding => "onboarding",
            Self::Ingesting => "ingesting",
            Self::Validating => "validating",
            Self::Complete => "complete",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "ingesting" => Self::Ingesting,
            "validating" => Self::Validating,
            "complete" => Self::Complete,
            _ => Self::Onboarding,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BootstrapState {
    pub stage: BootstrapStage,
    pub approver_user: Option<String>,
    pub dm_channel: Option<String>,
    pub current_step: usize,
    /// Keyed by question id (stable slugs like "roster", "top_initiatives").
    pub answers: Value,
    /// Sub-task progress for stage 2 (slack/jira/notion/etc.).
    pub ingestion_progress: Value,
    pub completed_at: Option<String>,
}

impl Default for BootstrapState {
    fn default() -> Self {
        Self {
            stage: BootstrapStage::Onboarding,
            approver_user: None,
            dm_channel: None,
            current_step: 0,
            answers: json!({}),
            ingestion_progress: json!({}),
            completed_at: None,
        }
    }
}

// ── Wave 1 question bank ────────────────────────────────────────────────
//
// Eight questions, chosen to capture the minimum context needed for the bot
// to operate without being dangerous. Each question has a stable id (used as
// the key under `answers`) and a target file it feeds into.

#[derive(Debug, Clone, Copy)]
pub enum AnswerTarget {
    Identity,
    Intents,
    Operations,
}

#[derive(Debug, Clone, Copy)]
pub struct BootstrapQuestion {
    pub id: &'static str,
    pub prompt: &'static str,
    pub target: AnswerTarget,
}

pub const WAVE_1_QUESTIONS: &[BootstrapQuestion] = &[
    BootstrapQuestion {
        id: "roster",
        prompt: "Who's on the team? Give me names and roles — I just need enough to know who's who. \
                 Example: \"Sarah (eng lead), Marcus (backend), Priya (design), Jordan (PM).\"",
        target: AnswerTarget::Identity,
    },
    BootstrapQuestion {
        id: "top_initiatives",
        prompt: "What are the 1-3 most important things the team is working on right now? \
                 For each, a sentence on *why it matters* is more useful to me than a project name.",
        target: AnswerTarget::Intents,
    },
    BootstrapQuestion {
        id: "stakeholders",
        prompt: "Who outside the team is going to ask about this work, and what do they care about? \
                 (Think: VPs, exec sponsors, customer-facing folks, adjacent teams.)",
        target: AnswerTarget::Identity,
    },
    BootstrapQuestion {
        id: "channels",
        prompt: "Which Slack channels should I pay attention to, and what is each one for? \
                 If there's a channel I should *default to* when I post something the whole team should see, tell me which.",
        target: AnswerTarget::Operations,
    },
    BootstrapQuestion {
        id: "standup",
        prompt: "How does the team do standup? Time, channel, format (async-written or live), and when you'd want \
                 me to post a summary. If you don't do standups, just say so.",
        target: AnswerTarget::Operations,
    },
    BootstrapQuestion {
        id: "current_worry",
        prompt: "What's your hardest blocker or biggest worry *right now*? Doesn't need to be profound — \
                 I just want to know what you'd flag first if I had to pick one thing to watch.",
        target: AnswerTarget::Intents,
    },
    BootstrapQuestion {
        id: "comm_norms",
        prompt: "How should I communicate? A few things I'm looking for: \
                 Who prefers DMs vs. public channels? Anyone I should *never* ping directly? \
                 What's the team's tolerance for me posting proactively vs. only when asked?",
        target: AnswerTarget::Identity,
    },
    BootstrapQuestion {
        id: "do_not_touch",
        prompt: "Last one: is there anything I should *not* touch or talk about? Confidential topics, \
                 channels I shouldn't read, people who'd rather not be mentioned, live incidents or negotiations. \
                 If there's nothing, just say \"nothing off-limits.\"",
        target: AnswerTarget::Identity,
    },
];

// ── BootstrapCoordinator ────────────────────────────────────────────────

/// Coordinates the bootstrap flow. Holds a mutex so only one answer is
/// processed at a time (the DM channel is single-threaded anyway).
pub struct BootstrapCoordinator {
    state: Mutex<BootstrapState>,
    db: Db,
    ws: Workspace,
}

impl BootstrapCoordinator {
    /// Load persisted state from Postgres, creating an empty one if none exists.
    pub async fn load(db: Db, ws: Workspace) -> Result<Arc<Self>> {
        let state = match db.load_bootstrap_state().await? {
            Some(s) => s,
            None => {
                let s = BootstrapState::default();
                db.save_bootstrap_state(&s).await?;
                s
            }
        };
        Ok(Arc::new(Self {
            state: Mutex::new(state),
            db,
            ws,
        }))
    }

    pub async fn stage(&self) -> BootstrapStage {
        self.state.lock().await.stage
    }

    pub async fn is_complete(&self) -> bool {
        matches!(self.stage().await, BootstrapStage::Complete)
    }

    pub async fn approver(&self) -> Option<String> {
        self.state.lock().await.approver_user.clone()
    }

    #[allow(dead_code)]
    pub async fn dm_channel(&self) -> Option<String> {
        self.state.lock().await.dm_channel.clone()
    }

    /// True if this DM channel is the live bootstrap conversation.
    pub async fn owns_dm(&self, channel_id: &str) -> bool {
        matches!(
            self.state.lock().await.dm_channel.as_deref(),
            Some(dm) if dm == channel_id
        )
    }

    /// Initiate onboarding with a chosen approver. Opens a DM and asks Q1.
    pub async fn kick_off(
        &self,
        approver_user: &str,
        messenger: &dyn Messenger,
    ) -> Result<()> {
        let intro = "Hi — I'm Delegate. I'm going to be your team's PM assistant.\n\n\
            Before I can do anything useful I need ~10 minutes to get to know the team. \
            I'll ask 8 questions. Write as much or as little as you want — prose is fine, \
            a single line is fine. If you don't know an answer, say so and we'll come back to it.\n\n\
            After this, I'll spend a bit of time reading recent Slack history and open tickets \
            to build up a picture on my own, and then I'll check in with what I think is going on.\n\n\
            Sound good? Let's start.";

        let sent = messenger.send_dm(approver_user, intro).await?;

        {
            let mut st = self.state.lock().await;
            st.approver_user = Some(approver_user.to_string());
            st.dm_channel = Some(sent.channel.clone());
            st.current_step = 0;
            st.stage = BootstrapStage::Onboarding;
            self.db.save_bootstrap_state(&st).await?;
        }

        // Post Q1
        let first = WAVE_1_QUESTIONS[0];
        let q = format!("**1/{}** — {}", WAVE_1_QUESTIONS.len(), first.prompt);
        messenger.send_dm(approver_user, &q).await?;

        info!(approver = %approver_user, "Bootstrap kick-off DM sent");
        Ok(())
    }

    /// Handle an incoming DM from the approver during the onboarding stage.
    /// Stores the answer, advances the step, asks the next question, or moves to stage 2.
    ///
    /// Returns true if the message was consumed by the bootstrap flow.
    pub async fn handle_onboarding_message(
        &self,
        from_user: &str,
        content: &str,
        messenger: &dyn Messenger,
    ) -> Result<bool> {
        let mut st = self.state.lock().await;

        if st.stage != BootstrapStage::Onboarding {
            return Ok(false);
        }
        // Only the designated approver drives the flow
        if st.approver_user.as_deref() != Some(from_user) {
            return Ok(false);
        }

        let step = st.current_step;
        if step >= WAVE_1_QUESTIONS.len() {
            // Shouldn't happen but be defensive
            st.stage = BootstrapStage::Ingesting;
            self.db.save_bootstrap_state(&st).await?;
            return Ok(true);
        }

        let q = WAVE_1_QUESTIONS[step];

        // Store the answer under its stable id
        let answers = st
            .answers
            .as_object_mut()
            .expect("answers is always an object");
        answers.insert(q.id.to_string(), Value::String(content.to_string()));

        st.current_step = step + 1;

        // If there are more questions, ask the next one.
        if st.current_step < WAVE_1_QUESTIONS.len() {
            let next = WAVE_1_QUESTIONS[st.current_step];
            let msg = format!(
                "**{}/{}** — {}",
                st.current_step + 1,
                WAVE_1_QUESTIONS.len(),
                next.prompt
            );
            self.db.save_bootstrap_state(&st).await?;
            drop(st);
            messenger.send_dm(from_user, &msg).await?;
            return Ok(true);
        }

        // All questions answered — move to stage 2.
        st.stage = BootstrapStage::Ingesting;
        self.db.save_bootstrap_state(&st).await?;
        let approver = st.approver_user.clone();
        let answers_snapshot = st.answers.clone();
        drop(st);

        // Tell the user what happens next.
        let transition = "Perfect — that's enough for me to start. I'm going to:\n\n\
            1. Read the last couple of weeks of Slack history in the channels you mentioned\n\
            2. Pull open tickets and recent activity from the tools you've connected\n\
            3. Come back with a summary of what I think is going on so you can correct me\n\n\
            You can keep working normally — I'll DM you when I'm ready to check in. \
            If you think of something you forgot to tell me, just send it and I'll remember it.";
        if let Some(u) = &approver {
            messenger.send_dm(u, transition).await.ok();
        }

        // Write initial IDENTITY/INTENTS/OPERATIONS from the answers.
        if let Err(e) = self.materialize_answers(&answers_snapshot).await {
            warn!(error = %e, "Failed to materialize bootstrap answers to workspace files");
        }

        Ok(true)
    }

    /// Translate answers collected in stage 1 into IDENTITY.md / INTENTS.md /
    /// OPERATIONS.md. We preserve the raw Q&A under a "Captured during onboarding"
    /// section so the agent can refer back to the exact words.
    async fn materialize_answers(&self, answers: &Value) -> Result<()> {
        let obj = match answers.as_object() {
            Some(o) => o,
            None => return Ok(()),
        };

        let mut identity_section = String::new();
        let mut intents_section = String::new();
        let mut operations_section = String::new();

        for q in WAVE_1_QUESTIONS {
            let answer = obj.get(q.id).and_then(|v| v.as_str()).unwrap_or("").trim();
            if answer.is_empty() {
                continue;
            }
            let block = format!("### {}\n\n> {}\n\n{}\n\n", q.id.replace('_', " "), q.prompt, answer);
            match q.target {
                AnswerTarget::Identity => identity_section.push_str(&block),
                AnswerTarget::Intents => intents_section.push_str(&block),
                AnswerTarget::Operations => operations_section.push_str(&block),
            }
        }

        let now = Local::now().format("%Y-%m-%d").to_string();

        if !identity_section.is_empty() {
            let existing = self.ws.identity().await;
            let merged = merge_section(
                &existing,
                "Captured during onboarding",
                &format!("_Recorded {}._\n\n{}", now, identity_section),
            );
            self.ws.save("IDENTITY.md", &merged).await?;
        }

        if !intents_section.is_empty() {
            let existing = self.ws.intents().await;
            let merged = merge_section(
                &existing,
                "Captured during onboarding",
                &format!("_Recorded {}._\n\n{}", now, intents_section),
            );
            self.ws.save("INTENTS.md", &merged).await?;
        }

        if !operations_section.is_empty() {
            let existing = self.ws.operations().await;
            let merged = merge_section(
                &existing,
                "Captured during onboarding",
                &format!("_Recorded {}._\n\n{}", now, operations_section),
            );
            self.ws.save("OPERATIONS.md", &merged).await?;
        }

        Ok(())
    }

    /// Advance from ingesting to validating. Called by the ingestion worker
    /// once background pulls are done.
    pub async fn mark_ingestion_complete(&self) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.stage != BootstrapStage::Ingesting {
            return Ok(());
        }
        st.stage = BootstrapStage::Validating;
        self.db.save_bootstrap_state(&st).await?;
        info!("Bootstrap: ingestion complete, moving to validation");
        Ok(())
    }

    /// Final transition: mark bootstrap complete.
    pub async fn mark_complete(&self) -> Result<()> {
        let mut st = self.state.lock().await;
        if matches!(st.stage, BootstrapStage::Complete) {
            return Ok(());
        }
        st.stage = BootstrapStage::Complete;
        st.completed_at = Some(Local::now().to_rfc3339());
        self.db.save_bootstrap_state(&st).await?;
        info!("Bootstrap complete — normal operation begins");
        Ok(())
    }

    /// Update ingestion progress for a given source (slack, jira, …).
    pub async fn record_ingestion_progress(
        &self,
        source: &str,
        pct: u32,
        note: &str,
    ) -> Result<()> {
        let mut st = self.state.lock().await;
        let progress = st
            .ingestion_progress
            .as_object_mut()
            .expect("ingestion_progress is always an object");
        progress.insert(
            source.to_string(),
            json!({ "pct": pct, "note": note, "at": Local::now().to_rfc3339() }),
        );
        self.db.save_bootstrap_state(&st).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn snapshot(&self) -> BootstrapState {
        self.state.lock().await.clone()
    }
}

/// Merge `body` into `existing` under a heading, replacing that section if
/// already present. Uses explicit HTML-comment delimiters so user-supplied
/// content (which may itself contain markdown headings) can't corrupt the
/// section-boundary detection.
///
/// Layout produced:
/// ```md
/// ## {heading}
/// <!-- delegate:managed-section:{slug}:begin -->
/// {body}
/// <!-- delegate:managed-section:{slug}:end -->
/// ```
fn merge_section(existing: &str, heading: &str, body: &str) -> String {
    let slug = heading
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let begin = format!("<!-- delegate:managed-section:{}:begin -->", slug);
    let end = format!("<!-- delegate:managed-section:{}:end -->", slug);
    let marker = format!("## {}", heading);

    let new_section = format!(
        "{marker}\n{begin}\n{body}\n{end}",
        body = body.trim_end()
    );

    // Fast path: delimiters already present — replace their contents (and the
    // heading line just above, if present) verbatim.
    if let (Some(begin_idx), Some(end_idx)) = (existing.find(&begin), existing.find(&end)) {
        if end_idx > begin_idx {
            // Include trailing newline of end marker in the slice if present
            let end_of_end = end_idx + end.len();
            // Look backward from begin_idx to swallow an immediately-preceding heading line.
            let prefix_end = existing[..begin_idx]
                .rfind(&marker)
                .filter(|&i| existing[i + marker.len()..begin_idx].trim().is_empty())
                .unwrap_or(begin_idx);
            let before = &existing[..prefix_end];
            let after = &existing[end_of_end..];
            let sep_before = if before.trim().is_empty() { "" } else { "\n\n" };
            let sep_after = if after.trim_start().is_empty() { "\n" } else { "\n\n" };
            return format!(
                "{}{}{}{}{}",
                before.trim_end(),
                sep_before,
                new_section,
                sep_after,
                after.trim_start()
            );
        }
    }

    // No delimiters — append. (First-run or legacy files.)
    let sep = if existing.trim().is_empty() { "" } else { "\n\n" };
    format!("{}{}{}\n", existing.trim_end(), sep, new_section)
}

// ── Validation summary (stage 3) ────────────────────────────────────────

/// Produce and post a "here's what I think is happening" summary to the
/// approver DM. Uses the model's own understanding of IDENTITY/INTENTS/
/// OPERATIONS + recently-ingested logs.
pub async fn post_validation_summary(
    coord: &BootstrapCoordinator,
    client: &ModelClient,
    messenger: &dyn Messenger,
    model_override: Option<&str>,
    recent_logs: &str,
) -> Result<()> {
    let approver = match coord.approver().await {
        Some(a) => a,
        None => {
            warn!("Validation summary requested but no approver is set");
            return Ok(());
        }
    };

    let identity = coord.ws.identity().await;
    let intents = coord.ws.intents().await;
    let operations = coord.ws.operations().await;

    let system = "You are Delegate, a PM agent checking in at the end of onboarding. \
        You've just finished reading the team's background (identity, intents, operations) \
        plus the last couple of weeks of logs. Write a concise (~250 words) check-in DM to the \
        team lead covering: active projects and apparent status, the key people you've identified \
        and their roles, pending decisions or blockers you noticed, and — critically — anything \
        you're confused about or unsure of. Be specific, reference real names/tickets where you \
        can, and explicitly ask the lead to correct anything you got wrong. End with: \
        'Once you've corrected me, I'll switch to normal operation.' Do not hedge with generic \
        language — if you don't know, say so specifically.";

    let prompt = format!(
        "# IDENTITY\n{}\n\n# INTENTS\n{}\n\n# OPERATIONS\n{}\n\n# Recent activity\n{}\n\n\
         Now write the check-in DM for the team lead.",
        identity, intents, operations, recent_logs
    );

    let response = client
        .complete(CompleteOptions {
            system: system.to_string(),
            prompt,
            model: model_override.map(|s| s.to_string()),
            max_tokens: Some(800),
            temperature: Some(0.5),
            tools: None,
        })
        .await?;

    messenger.send_dm(&approver, &response.content).await?;
    info!("Validation summary DM sent to {approver}");
    Ok(())
}

// ── Serde helpers ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct AnswerMap(pub serde_json::Map<String, Value>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_section_appends_when_absent() {
        let existing = "# Team\n\nSome existing content.";
        let merged = merge_section(existing, "Captured during onboarding", "Q1: foo");
        assert!(merged.contains("Some existing content."));
        assert!(merged.contains("## Captured during onboarding"));
        assert!(merged.contains("Q1: foo"));
        assert!(merged.contains("delegate:managed-section:captured-during-onboarding:begin"));
        assert!(merged.contains("delegate:managed-section:captured-during-onboarding:end"));
    }

    #[test]
    fn merge_section_replaces_existing() {
        // Start from a file that already has the managed delimiters.
        let first_pass = merge_section(
            "# Team\n\n",
            "Captured during onboarding",
            "Old answer.",
        );
        let with_next = format!("{}\n## Next Section\n\nkeep me.", first_pass.trim_end());
        let merged = merge_section(&with_next, "Captured during onboarding", "New answer.");
        assert!(merged.contains("New answer."));
        assert!(!merged.contains("Old answer."));
        assert!(merged.contains("## Next Section"));
        assert!(merged.contains("keep me."));
    }

    #[test]
    fn merge_section_survives_headings_in_answer() {
        // User answer contains a markdown heading — the old heuristic would
        // treat it as a section boundary and corrupt the file.
        let existing = merge_section("", "Captured during onboarding", "first answer");
        let hostile_answer = "## My team structure\n\n- Sarah (eng)\n- Marcus (backend)";
        let merged = merge_section(&existing, "Captured during onboarding", hostile_answer);
        // The hostile answer must appear verbatim inside the delimited block.
        assert!(merged.contains("## My team structure"));
        assert!(merged.contains("Marcus (backend)"));
        // And a re-merge with different content must still replace cleanly.
        let again = merge_section(&merged, "Captured during onboarding", "third answer");
        assert!(again.contains("third answer"));
        assert!(!again.contains("Marcus (backend)"));
        assert!(!again.contains("## My team structure"));
    }

    #[test]
    fn merge_section_preserves_surrounding_content() {
        let existing = merge_section("# Team\n\nPreamble paragraph.", "Captured during onboarding", "seed");
        let with_tail = format!("{}\n## Tail Section\n\nImportant tail.\n", existing.trim_end());
        let merged = merge_section(&with_tail, "Captured during onboarding", "updated");
        assert!(merged.contains("Preamble paragraph."));
        assert!(merged.contains("## Tail Section"));
        assert!(merged.contains("Important tail."));
        assert!(merged.contains("updated"));
        assert!(!merged.contains("seed"));
    }

    #[test]
    fn stage_roundtrip() {
        for s in ["onboarding", "ingesting", "validating", "complete"] {
            assert_eq!(BootstrapStage::from_str(s).as_str(), s);
        }
    }

    #[test]
    fn wave_1_has_eight_questions() {
        assert_eq!(WAVE_1_QUESTIONS.len(), 8);
        // All ids must be unique and stable
        let mut seen = std::collections::HashSet::new();
        for q in WAVE_1_QUESTIONS {
            assert!(seen.insert(q.id), "duplicate question id: {}", q.id);
        }
    }
}
