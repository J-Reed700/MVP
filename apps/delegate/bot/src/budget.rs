use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Shared daily token budget tracker.
/// Tracks total tokens used today. Resets at midnight.
#[derive(Clone)]
pub struct TokenBudget {
    inner: Arc<Mutex<TokenBudgetInner>>,
}

struct TokenBudgetInner {
    used: u64,
    limit: u64,
    date: String,
    notified: bool,
}

impl TokenBudget {
    pub fn new(limit: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenBudgetInner {
                used: 0,
                limit,
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
                notified: false,
            })),
        }
    }

    /// Record token usage. Returns true if still within budget.
    pub async fn record(&self, tokens: u64) -> bool {
        let mut inner = self.inner.lock().await;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        if inner.date != today {
            info!(
                prev_date = %inner.date,
                used = inner.used,
                "Token budget reset for new day"
            );
            inner.used = 0;
            inner.date = today;
            inner.notified = false;
        }

        inner.used += tokens;
        inner.used <= inner.limit
    }

    /// Check if we're within budget without recording.
    pub async fn is_available(&self) -> bool {
        let mut inner = self.inner.lock().await;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        if inner.date != today {
            inner.used = 0;
            inner.date = today;
            inner.notified = false;
        }

        inner.used < inner.limit
    }

    pub async fn mark_notified(&self) {
        self.inner.lock().await.notified = true;
    }

    pub async fn was_notified(&self) -> bool {
        self.inner.lock().await.notified
    }

    pub async fn set_limit(&self, limit: u64) {
        self.inner.lock().await.limit = limit;
    }
}
