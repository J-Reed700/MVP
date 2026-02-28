# JIRA

> Simple file-based project tracker. Epics → Tasks. Git is the history.

## Statuses

`backlog` · `todo` · `in-progress` · `review` · `done`

---

## E-001: Core Bot Runtime

**Owner:** Alan
**Status:** done

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-001 | Socket Mode connection + event loop | Alan | done | |
| T-002 | Two-tier triage (pattern + LLM) | Alan | done | |
| T-003 | Context assembly pipeline | Alan | done | |
| T-004 | Tool-calling response loop | Alan | done | |
| T-005 | Thread context fetching | Alan | done | |
| T-006 | Skill registry (SKILL.md pattern) | Alan | done | |
| T-007 | Self-authoring skills (create_skill) | Alan | done | |
| T-008 | File I/O tools (read_file, write_file) | Alan | done | |
| T-009 | Multi-turn tool use (read → decide → write in one event) | Alan | done | Tool loop with accumulated conversation history, up to 5 turns |
| T-010 | Deduplication of events (message + app_mention for same msg) | Alan | done | LRU cache (200 entries) keyed on channel:timestamp |

---

## E-002: Heartbeat & Scheduling

**Owner:** Alan
**Status:** done

Spec: 5-min heartbeat scans for changes, cron for scheduled outputs. Heartbeat diffs daily log since last tick; no-op if nothing new.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-011 | Heartbeat loop (configurable interval, default 5 min) | Alan | done | Background loop scans daily log diffs, batched reasoning on new entries, no-op when nothing changed |
| T-012 | Cron scheduler for recurring outputs | Alan | done | Parses schedule from HEARTBEAT.md, fires at configured times, posts to target channels with appropriate TaskType |
| T-013 | HEARTBEAT.md as control surface | Alan | done | Parses interval, cron jobs (time/day/channel/type), token budgets. Hot-reloads every tick. Unit tested |
| T-014 | Batched reasoning on heartbeat tick | Alan | done | Queued events tagged [queued], heartbeat skips reasoning when no queued entries, batch prompt looks for cross-signal patterns |
| T-015 | Daily token budget + log-only mode | Alan | done | Shared TokenBudget tracker, 500K/day default from HEARTBEAT.md, log-only mode with team notification, midnight reset |

---

## E-003: Memory System

**Owner:** Alan
**Status:** done

Spec: three layers — conversation context, project state, organizational knowledge. Inspectable, correctable, grows over time.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-016 | MEMORY.md as structured index to topic files | Alan | done | save_memory tool auto-maintains MEMORY.md index pointing to memory/*.md |
| T-017 | Agent-driven memory writes (learn → persist → index) | Alan | done | save_memory tool: writes memory/{topic}.md + updates MEMORY.md index atomically |
| T-018 | Memory inspection ("what do you know about X?") | Alan | done | recall_memory tool: searches across all memory files, returns matching excerpts |
| T-019 | Memory correction (team says "that's wrong" → update) | Alan | done | save_memory overwrites topic; recall_memory → read → correct → save_memory flow |
| T-020 | INTENTS.md auto-update from observations | Alan | done | update_intents tool: full replacement with audit log entry |

---

## E-004: Context Assembly (The Product)

**Owner:** Alan
**Status:** done

Spec: this is 70% of the product. Intent-biased retrieval, audience-aware framing, token budget allocation.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-021 | Intent-biased retrieval | Alan | done | Co-occurrence bonus when files match both query + intent terms; backtick-quoted phrase extraction; bias terms from INTENTS.md |
| T-022 | Audience-aware framing | Alan | done | Audience enum inferred from channel name patterns (eng/exec/team/direct), shapes framing instructions |
| T-023 | Token budget allocation with priority ordering | Alan | done | 6-tier priority: Identity/Intents/Framing/Trigger never cut, logs/memory gracefully truncated, retrieval fills remainder |
| T-024 | Compressed intent summary for Tier 1 triage | Alan | done | compress_intents() extracts structural lines (headings, bullets) first, fills with prose, caps at ~500 tokens |

---

## E-005: Autonomy Model

**Owner:** Alan
**Status:** in-progress

Spec: three tiers — always autonomous, autonomous with notice, requires human approval.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-025 | Action tier classification on tool calls | Alan | done | classify_action() maps each tool to Autonomous/AutonomousWithNotice/RequiresApproval |
| T-026 | Approval workflow via Slack DM | | todo | Propose → DM approver → react to approve/reject → timeout → escalate |
| T-027 | pending/ audit trail for approval actions | Alan | done | write_pending_action() writes to pending/{timestamp}-{slug}.md, never deleted |
| T-028 | Configurable tier overrides per team | | backlog | Team can move any action between tiers |

---

## E-006: Bootstrap & Onboarding

**Owner:**
**Status:** backlog

Spec: day-one value. Guided onboarding → background ingestion → validation summary.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-029 | Guided onboarding DM flow (~15 min) | | backlog | Structured questions about team, projects, norms → populates IDENTITY.md + INTENTS.md |
| T-030 | Background Slack history ingestion (last 2 weeks) | | backlog | Async, append to daily log with [ingestion] tag, show progress in channel |
| T-031 | Validation summary ("here's what I think is happening") | | backlog | Post summary to team channel, team corrects, corrections stick |

---

## E-007: Integrations

**Owner:**
**Status:** backlog

Spec: Slack is home base. Jira/Linear, Notion/Confluence, Calendar, Email, CRM follow.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-032 | Slack: channel history read (beyond current thread) | Alan | done | channel_history tool using conversations.history API, user name resolution, multi-turn enabled |
| T-033 | Slack: DM support (send/receive) | Alan | done | dm_user tool using conversations.open + chat.postMessage, enables approvals and private nudges |
| T-034 | Slack: user ID → display name resolution | Alan | done | Cached users.info API calls, thread history shows real names |
| T-035 | Jira/Linear integration (read tickets, create, update, comment) | | backlog | ExternalService trait |
| T-036 | Notion/Confluence integration (read/write docs) | | backlog | |
| T-037 | Calendar integration (schedule awareness, agenda prep) | | backlog | Read-only MVP |
| T-038 | Email integration (draft stakeholder updates) | | backlog | Requires approval tier for sends |

---

## E-008: Proactive Behaviors

**Owner:**
**Status:** backlog

Spec: "It acts before being asked." These are the behaviors that make Delegate feel like a teammate.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-039 | Standup summary generation (cron-triggered) | | backlog | Compile overnight activity into morning summary, post to channel |
| T-040 | Stale ticket/PR nudging | | backlog | Detect items idle >N days, DM owner |
| T-041 | Blocker detection and escalation | | backlog | Cross-reference signals: someone says "blocked" + intent says it's critical path |
| T-042 | Cross-thread connection ("this relates to...") | | backlog | Notice when a conversation in one channel is relevant to another |
| T-043 | Stakeholder update drafts (weekly, requires approval) | | backlog | Audience-aware: execs get outcomes/risks, engineers get specifics |
| T-044 | Decision log maintenance | Alan | done | log_decision tool captures decisions to memory/decisions.md with reasoning, participants, date, auto-updates MEMORY.md index |

---

## E-009: Production Readiness

**Owner:**
**Status:** backlog

Move from dogfooding to deployable.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-045 | Move from Socket Mode to webhook-driven (axum HTTP) | | backlog | Socket Mode is for dogfooding; prod needs webhooks per ARCHITECTURE.md |
| T-046 | Credential isolation (never expose to agent logic) | | backlog | IronClaw pattern: injected at execution boundary |
| T-047 | Cloud workspace (not local filesystem) | | backlog | Team agent, not personal agent — workspace is cloud-hosted |
| T-048 | Concurrent access safety (single-writer mpsc queue) | | backlog | Per ARCHITECTURE.md: all mutations serialized through tokio::mpsc |
| T-049 | Error recovery + reconnection logic | Alan | done | LLM retry with exponential backoff (3 attempts), Slack API retry on rate limits/transient errors, 5-min event handler timeout, Socket Mode reconnect already in place |
| T-050 | Structured logging + observability | Alan | done | #[instrument] spans on handle_event/execute_tool, per-event token tracking, duration metrics, span fields for total_tokens/tool_count |

---

## E-010: Dogfooding & Polish

**Owner:** Alan, Josh
**Status:** in-progress

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-051 | Test ticket tracking end-to-end | | todo | |
| T-052 | Tune triage sensitivity | | todo | |
| T-053 | Test cross-channel posting | | todo | |
| T-054 | Stress test with real conversations | | todo | |
| T-055 | IDENTITY.md refinement from dogfooding feedback | | todo | |
| T-056 | Kill hallucinated capabilities (ongoing) | Alan | done | SKILLS.md registry loaded into context, explicit "I can't do that yet" instruction in system prompt |

---

## How to use this file

- **Add a task:** Append a row to the relevant epic table
- **New epic:** Add a new `## E-XXX` section
- **Update status:** Change the status cell
- **Delegate bot** can read and write this file via `read_file` and `write_file`
