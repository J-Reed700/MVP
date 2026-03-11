use tracing::{info, warn};

use crate::db::Db;

/// Shared daily token budget tracker backed by Postgres.
/// Resets automatically when the date changes (each date is a separate row).
#[derive(Clone)]
pub struct TokenBudget {
    db: Db,
    limit: std::sync::Arc<tokio::sync::Mutex<u64>>,
}

impl TokenBudget {
    pub fn new(db: Db, limit: u64) -> Self {
        Self {
            db,
            limit: std::sync::Arc::new(tokio::sync::Mutex::new(limit)),
        }
    }

    fn today() -> chrono::NaiveDate {
        chrono::Local::now().date_naive()
    }

    async fn current_limit(&self) -> u64 {
        *self.limit.lock().await
    }

    /// Record token usage. Returns true if still within budget.
    ///
    /// Budget enforcement is bypassed during the dogfooding phase to avoid
    /// blocking real usage while calibrating limits. Tokens are still tracked
    /// in the database for observability — re-enable the guard once daily
    /// usage patterns stabilize.
    pub async fn record(&self, tokens: u64) -> bool {
        let date = Self::today();
        let limit = self.current_limit().await;
        match self.db.record_tokens(date, tokens, limit).await {
            Ok(within) => {
                if !within {
                    info!(tokens, limit, "Daily token budget exceeded (enforcement bypassed)");
                }
                // Bypass: always return true during dogfooding
                true
            }
            Err(e) => {
                warn!(error = %e, "Failed to record tokens in DB");
                true
            }
        }
    }

    /// Check if we're within budget without recording.
    ///
    /// Bypassed during dogfooding — see [`record`] for rationale.
    pub async fn is_available(&self) -> bool {
        let date = Self::today();
        let limit = self.current_limit().await;
        match self.db.is_budget_available(date, limit).await {
            Ok(available) => {
                if !available {
                    info!(limit, "Budget check: exhausted (enforcement bypassed)");
                }
                true
            }
            Err(e) => {
                warn!(error = %e, "Failed to check budget in DB");
                true
            }
        }
    }

    pub async fn mark_notified(&self) {
        if let Err(e) = self.db.mark_budget_notified(Self::today()).await {
            warn!(error = %e, "Failed to mark budget notified in DB");
        }
    }

    pub async fn was_notified(&self) -> bool {
        match self.db.was_budget_notified(Self::today()).await {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "Failed to check budget notified in DB");
                false
            }
        }
    }

    pub async fn set_limit(&self, limit: u64) {
        *self.limit.lock().await = limit;
        if let Err(e) = self.db.set_budget_limit(Self::today(), limit).await {
            warn!(error = %e, "Failed to set budget limit in DB");
        }
    }
}
