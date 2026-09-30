//! Wave 2 — progressive deepening.
//!
//! After bootstrap completes, the heartbeat continues to refine IDENTITY /
//! INTENTS / OPERATIONS by asking *targeted* follow-up questions based on
//! observed activity. Two rules:
//!
//! 1. Only ask when an observation genuinely lacks context that would change
//!    how the agent behaves. "Marcus merged 3 PRs on DEV-42 — is he tech
//!    lead?" is legitimate. "Did someone join the channel?" is not.
//! 2. Never ask the same thing twice. The `deepening_asked` table de-dups.
//!
//! The heartbeat prompt tells the model to use these primitives. The actual
//! judgment about what to ask lives with the model; this module just
//! provides the mechanical hooks.

use anyhow::Result;

use crate::db::Db;

/// Prompt fragment appended to the heartbeat system prompt once bootstrap
/// is complete. Encourages the agent to file ONE targeted clarifier per
/// batch if an observation genuinely needs context.
pub const DEEPENING_PROMPT: &str = "\
# Deepening (Wave 2)

You have an additional ongoing job: refining what you know about the team. \
When you observe something that lacks context that would change how you act, \
ask ONE targeted question via `ask_clarifier(key, question)` — but only if:

- The gap is real. If you can answer the question from existing files, don't ask.
- The answer would change future behavior. (\"Is Marcus tech lead on DEV-42?\" \
  would — you'd route DEV-42 decisions to him. \"What's Marcus's favorite color?\" \
  would not.)
- You haven't asked something similar before. Use a stable `key` so the system \
  can de-dup (e.g. `owner:dev-42`, `cadence:sprint-review`, `channel-usage:eng-leads`).

Prefer at most one clarifier per heartbeat tick. The team will lose patience fast \
if you interview them every 5 minutes.

When the team answers a clarifier, update the relevant file (IDENTITY.md, \
INTENTS.md, or OPERATIONS.md) with `save_memory` or an edit. The answer is \
high-confidence — treat it as ground truth.";

/// Returns true if this clarifier should be asked (i.e. we haven't asked it before).
#[allow(dead_code)]
pub async fn should_ask(db: &Db, clarifier_key: &str) -> bool {
    db.deepening_is_new(clarifier_key).await.unwrap_or(false)
}

/// Record that a clarifier was asked so we don't ask it again.
#[allow(dead_code)]
pub async fn record_asked(db: &Db, clarifier_key: &str) -> Result<()> {
    db.deepening_record_asked(clarifier_key).await
}
