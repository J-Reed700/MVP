use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        let db = Self { pool };
        db.run_migrations().await?;
        info!("Database connected and migrations applied");
        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
        sqlx::raw_sql(
            r#"
            CREATE TABLE IF NOT EXISTS token_budget (
                date         DATE PRIMARY KEY,
                used         BIGINT  NOT NULL DEFAULT 0,
                budget_limit BIGINT  NOT NULL,
                notified     BOOLEAN NOT NULL DEFAULT FALSE
            );

            CREATE TABLE IF NOT EXISTS event_dedup (
                dedup_key  TEXT        PRIMARY KEY,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_event_dedup_created
                ON event_dedup (created_at);

            CREATE TABLE IF NOT EXISTS pending_approvals (
                id               UUID PRIMARY KEY,
                state            TEXT    NOT NULL DEFAULT 'pending',
                tool_name        TEXT    NOT NULL,
                tool_arguments   JSONB   NOT NULL DEFAULT '{}',
                requester        TEXT    NOT NULL,
                trigger_channel  TEXT    NOT NULL,
                trigger_ts       TEXT    NOT NULL,
                thread_ts        TEXT,
                approver         TEXT    NOT NULL,
                dm_channel       TEXT,
                dm_ts            TEXT,
                created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                timeout_secs     BIGINT  NOT NULL DEFAULT 14400,
                backup_approver  TEXT,
                escalated        BOOLEAN NOT NULL DEFAULT FALSE,
                resolved_at      TIMESTAMPTZ,
                resolution_note  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_pending_state
                ON pending_approvals (state);
            CREATE INDEX IF NOT EXISTS idx_pending_dm
                ON pending_approvals (dm_channel, dm_ts);

            CREATE TABLE IF NOT EXISTS activity_logs (
                id         BIGSERIAL   PRIMARY KEY,
                log_date   DATE        NOT NULL,
                log_time   TEXT        NOT NULL,
                channel    TEXT        NOT NULL,
                username   TEXT        NOT NULL,
                content    TEXT        NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_logs_date
                ON activity_logs (log_date);

            CREATE TABLE IF NOT EXISTS cron_state (
                job_name   TEXT        PRIMARY KEY,
                last_fired TIMESTAMPTZ NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reminders (
                id      UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
                channel TEXT        NOT NULL,
                username TEXT       NOT NULL,
                message TEXT        NOT NULL,
                fire_at TIMESTAMPTZ NOT NULL,
                fired   BOOLEAN     NOT NULL DEFAULT FALSE
            );
            CREATE INDEX IF NOT EXISTS idx_reminders_fire
                ON reminders (fire_at) WHERE NOT fired;
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Token Budget ───────────────────────────────────────────────────

    pub async fn record_tokens(&self, date: NaiveDate, tokens: u64, limit: u64) -> Result<bool> {
        let row = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO token_budget (date, used, budget_limit)
            VALUES ($1, $2, $3)
            ON CONFLICT (date) DO UPDATE
                SET used = token_budget.used + $2
            RETURNING used
            "#,
        )
        .bind(date)
        .bind(tokens as i64)
        .bind(limit as i64)
        .fetch_one(&self.pool)
        .await?;
        Ok(row <= limit as i64)
    }

    pub async fn is_budget_available(&self, date: NaiveDate, limit: u64) -> Result<bool> {
        let used: Option<i64> = sqlx::query_scalar(
            "SELECT used FROM token_budget WHERE date = $1",
        )
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(used.unwrap_or(0) < limit as i64)
    }

    pub async fn mark_budget_notified(&self, date: NaiveDate) -> Result<()> {
        sqlx::query("UPDATE token_budget SET notified = TRUE WHERE date = $1")
            .bind(date)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn was_budget_notified(&self, date: NaiveDate) -> Result<bool> {
        let notified: Option<bool> = sqlx::query_scalar(
            "SELECT notified FROM token_budget WHERE date = $1",
        )
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(notified.unwrap_or(false))
    }

    pub async fn set_budget_limit(&self, date: NaiveDate, limit: u64) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO token_budget (date, used, budget_limit)
            VALUES ($1, 0, $2)
            ON CONFLICT (date) DO UPDATE SET budget_limit = $2
            "#,
        )
        .bind(date)
        .bind(limit as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Event Dedup ────────────────────────────────────────────────────

    /// Returns true if this is a NEW event (not a duplicate).
    pub async fn check_and_insert_dedup(&self, key: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO event_dedup (dedup_key)
            VALUES ($1)
            ON CONFLICT (dedup_key) DO NOTHING
            "#,
        )
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_old_dedup(&self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM event_dedup WHERE created_at < NOW() - INTERVAL '24 hours'",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ── Pending Approvals ──────────────────────────────────────────────

    pub async fn save_approval(&self, action: &super::approval::PendingAction) -> Result<()> {
        let id: Uuid = action.id.parse()?;
        let created_at = DateTime::parse_from_rfc3339(&action.created_at)?
            .with_timezone(&Utc);
        let resolved_at = action
            .resolved_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        sqlx::query(
            r#"
            INSERT INTO pending_approvals (
                id, state, tool_name, tool_arguments, requester,
                trigger_channel, trigger_ts, thread_ts, approver,
                dm_channel, dm_ts, created_at, timeout_secs,
                backup_approver, escalated, resolved_at, resolution_note
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12, $13,
                $14, $15, $16, $17
            )
            ON CONFLICT (id) DO UPDATE SET
                state = $2, dm_channel = $10, dm_ts = $11,
                created_at = $12, escalated = $15,
                resolved_at = $16, resolution_note = $17
            "#,
        )
        .bind(id)
        .bind(approval_state_str(&action.state))
        .bind(&action.tool_name)
        .bind(&action.tool_arguments)
        .bind(&action.requester)
        .bind(&action.trigger_channel)
        .bind(&action.trigger_ts)
        .bind(&action.thread_ts)
        .bind(&action.approver)
        .bind(&action.dm_channel)
        .bind(&action.dm_ts)
        .bind(created_at)
        .bind(action.timeout_secs as i64)
        .bind(&action.backup_approver)
        .bind(action.escalated)
        .bind(resolved_at)
        .bind(&action.resolution_note)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_approval_by_dm(
        &self,
        dm_channel: &str,
        dm_ts: &str,
    ) -> Result<Option<super::approval::PendingAction>> {
        let row = sqlx::query_as::<_, ApprovalRow>(
            r#"
            SELECT id, state, tool_name, tool_arguments, requester,
                   trigger_channel, trigger_ts, thread_ts, approver,
                   dm_channel, dm_ts, created_at, timeout_secs,
                   backup_approver, escalated, resolved_at, resolution_note
            FROM pending_approvals
            WHERE dm_channel = $1 AND dm_ts = $2
              AND state IN ('pending', 'escalated')
            LIMIT 1
            "#,
        )
        .bind(dm_channel)
        .bind(dm_ts)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_pending_action()))
    }

    pub async fn load_pending_approvals(&self) -> Result<Vec<super::approval::PendingAction>> {
        let rows = sqlx::query_as::<_, ApprovalRow>(
            r#"
            SELECT id, state, tool_name, tool_arguments, requester,
                   trigger_channel, trigger_ts, thread_ts, approver,
                   dm_channel, dm_ts, created_at, timeout_secs,
                   backup_approver, escalated, resolved_at, resolution_note
            FROM pending_approvals
            WHERE state IN ('pending', 'escalated')
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_pending_action()).collect())
    }

    // ── Activity Logs ──────────────────────────────────────────────────

    pub async fn append_log(
        &self,
        date: NaiveDate,
        time: &str,
        channel: &str,
        user: &str,
        content: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO activity_logs (log_date, log_time, channel, username, content)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(date)
        .bind(time)
        .bind(channel)
        .bind(user)
        .bind(content)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn read_recent_logs(&self, today: NaiveDate) -> Result<String> {
        let yesterday = today - chrono::Duration::days(1);
        let rows = sqlx::query_as::<_, LogRow>(
            r#"
            SELECT log_date, log_time, channel, username, content
            FROM activity_logs
            WHERE log_date >= $1
            ORDER BY log_date, id
            "#,
        )
        .bind(yesterday)
        .fetch_all(&self.pool)
        .await?;

        let mut output = String::new();
        let mut current_date: Option<NaiveDate> = None;
        for row in &rows {
            if current_date != Some(row.log_date) {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!("# Daily Log — {}\n\n", row.log_date));
                current_date = Some(row.log_date);
            }
            output.push_str(&format!(
                "- {} | #{} | @{} | {}\n",
                row.log_time, row.channel, row.username, row.content
            ));
        }
        Ok(output)
    }

    /// Read log entries since a given row ID. Returns (formatted entries, last_id).
    pub async fn read_logs_since(&self, today: NaiveDate, after_id: i64) -> Result<(String, i64)> {
        let rows = sqlx::query_as::<_, LogRowWithId>(
            r#"
            SELECT id, log_time, channel, username, content
            FROM activity_logs
            WHERE log_date = $1 AND id > $2
            ORDER BY id
            "#,
        )
        .bind(today)
        .bind(after_id)
        .fetch_all(&self.pool)
        .await?;

        let mut last_id = after_id;
        let mut entries = String::new();
        for row in &rows {
            entries.push_str(&format!(
                "- {} | #{} | @{} | {}\n",
                row.log_time, row.channel, row.username, row.content
            ));
            last_id = row.id;
        }
        Ok((entries, last_id))
    }

    // ── Cron State ─────────────────────────────────────────────────────

    pub async fn get_all_last_fired(
        &self,
    ) -> Result<std::collections::HashMap<String, DateTime<Local>>> {
        let rows = sqlx::query_as::<_, CronRow>(
            "SELECT job_name, last_fired FROM cron_state",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.job_name, r.last_fired.with_timezone(&Local)))
            .collect())
    }

    pub async fn set_last_fired(&self, job_name: &str, fired_at: DateTime<Local>) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cron_state (job_name, last_fired)
            VALUES ($1, $2)
            ON CONFLICT (job_name) DO UPDATE SET last_fired = $2
            "#,
        )
        .bind(job_name)
        .bind(fired_at.with_timezone(&Utc))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Reminders ──────────────────────────────────────────────────────

    pub async fn add_reminder(
        &self,
        channel: &str,
        user: &str,
        message: &str,
        fire_at: DateTime<Utc>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO reminders (id, channel, username, message, fire_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(channel)
        .bind(user)
        .bind(message)
        .bind(fire_at)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_due_reminders(&self) -> Result<Vec<ReminderRow>> {
        let rows = sqlx::query_as::<_, ReminderRow>(
            r#"
            SELECT id, channel, username, message, fire_at
            FROM reminders
            WHERE NOT fired AND fire_at <= NOW()
            ORDER BY fire_at
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_reminder_fired(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE reminders SET fired = TRUE WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ── Row types for sqlx ─────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ApprovalRow {
    id: Uuid,
    state: String,
    tool_name: String,
    tool_arguments: Value,
    requester: String,
    trigger_channel: String,
    trigger_ts: String,
    thread_ts: Option<String>,
    approver: String,
    dm_channel: Option<String>,
    dm_ts: Option<String>,
    created_at: DateTime<Utc>,
    timeout_secs: i64,
    backup_approver: Option<String>,
    escalated: bool,
    resolved_at: Option<DateTime<Utc>>,
    resolution_note: Option<String>,
}

impl ApprovalRow {
    fn into_pending_action(self) -> super::approval::PendingAction {
        super::approval::PendingAction {
            id: self.id.to_string(),
            state: parse_approval_state(&self.state),
            tool_name: self.tool_name,
            tool_arguments: self.tool_arguments,
            requester: self.requester,
            trigger_channel: self.trigger_channel,
            trigger_ts: self.trigger_ts,
            thread_ts: self.thread_ts,
            approver: self.approver,
            dm_channel: self.dm_channel,
            dm_ts: self.dm_ts,
            created_at: self.created_at.with_timezone(&Local).to_rfc3339(),
            timeout_secs: self.timeout_secs as u64,
            backup_approver: self.backup_approver,
            escalated: self.escalated,
            resolved_at: self.resolved_at.map(|dt| dt.with_timezone(&Local).to_rfc3339()),
            resolution_note: self.resolution_note,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LogRow {
    log_date: NaiveDate,
    log_time: String,
    channel: String,
    username: String,
    content: String,
}

#[derive(sqlx::FromRow)]
struct LogRowWithId {
    id: i64,
    log_time: String,
    channel: String,
    username: String,
    content: String,
}

#[derive(sqlx::FromRow)]
struct CronRow {
    job_name: String,
    last_fired: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct ReminderRow {
    pub id: Uuid,
    pub channel: String,
    pub username: String,
    pub message: String,
    pub fire_at: DateTime<Utc>,
}

fn approval_state_str(state: &super::approval::ApprovalState) -> &'static str {
    use super::approval::ApprovalState;
    match state {
        ApprovalState::Pending => "pending",
        ApprovalState::Approved => "approved",
        ApprovalState::Rejected => "rejected",
        ApprovalState::Escalated => "escalated",
        ApprovalState::Expired => "expired",
    }
}

fn parse_approval_state(s: &str) -> super::approval::ApprovalState {
    use super::approval::ApprovalState;
    match s {
        "approved" => ApprovalState::Approved,
        "rejected" => ApprovalState::Rejected,
        "escalated" => ApprovalState::Escalated,
        "expired" => ApprovalState::Expired,
        _ => ApprovalState::Pending,
    }
}
