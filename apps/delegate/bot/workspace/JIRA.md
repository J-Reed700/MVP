# JIRA

> Simple file-based project tracker. Epics → Tasks. Git is the history.

## Statuses

`backlog` · `todo` · `in-progress` · `review` · `done`

---

## E-001: Core Bot Runtime

**Owner:** Alan
**Status:** in-progress

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
| T-009 | Multi-turn tool use (read → decide → write in one event) | | todo | Model currently gets one shot; needs tool result loop so it can read a file then act on it |
| T-010 | Deduplication of events (message + app_mention for same msg) | | todo | Edited messages trigger both message_changed and app_mention |

---

## E-002: Heartbeat & Scheduling

**Owner:**
**Status:** todo

Spec: 5-min heartbeat scans for changes, cron for scheduled outputs. Heartbeat diffs daily log since last tick; no-op if nothing new.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-011 | Heartbeat loop (configurable interval, default 5 min) | | todo | Wake, scan daily log since last tick, decide if anything needs attention |
| T-012 | Cron scheduler for recurring outputs | | todo | Standup at 9:15am, weekly summary Friday 4pm, etc. Definitions in HEARTBEAT.md |
| T-013 | HEARTBEAT.md as control surface | | todo | Team-editable config: interval, cron schedules, triage rules, watch patterns |
| T-014 | Batched reasoning on heartbeat tick | | todo | Accumulate queued events, reason about them as a batch through intent lens |
| T-015 | Daily token budget + log-only mode | | backlog | 500K tokens/day default, enter log-only mode when exhausted, notify team |

---

## E-003: Memory System

**Owner:**
**Status:** todo

Spec: three layers — conversation context, project state, organizational knowledge. Inspectable, correctable, grows over time.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-016 | MEMORY.md as structured index to topic files | | todo | Agent maintains this itself; points to memory/*.md |
| T-017 | Agent-driven memory writes (learn → persist → index) | | todo | When bot learns something worth retaining, write to memory/ and update MEMORY.md |
| T-018 | Memory inspection ("what do you know about X?") | | todo | Transparent answers from memory files, not black box |
| T-019 | Memory correction (team says "that's wrong" → update) | | todo | Corrections treated as high-confidence updates |
| T-020 | INTENTS.md auto-update from observations | | todo | Bot updates intents as it observes events, not just human edits |

---

## E-004: Context Assembly (The Product)

**Owner:**
**Status:** in-progress

Spec: this is 70% of the product. Intent-biased retrieval, audience-aware framing, token budget allocation.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-021 | Intent-biased retrieval | | todo | Augment search queries with keywords from INTENTS.md; query about API team also pulls billing migration if intent says they're linked |
| T-022 | Audience-aware framing | | todo | Different framing for engineers vs execs vs team channel. Output type shapes the prompt |
| T-023 | Token budget allocation with priority ordering | | todo | Identity > Intents > Framing > Trigger > History > Memory. Never cut identity/intents |
| T-024 | Compressed intent summary for Tier 1 triage | | todo | ~500 token distilled version of INTENTS.md for cheap triage model |

---

## E-005: Autonomy Model

**Owner:**
**Status:** backlog

Spec: three tiers — always autonomous, autonomous with notice, requires human approval.

| ID | Task | Assignee | Status | Notes |
|----|------|----------|--------|-------|
| T-025 | Action tier classification on tool calls | | backlog | Tag each action as autonomous / notice / approval-required |
| T-026 | Approval workflow via Slack DM | | backlog | Propose → DM approver → react to approve/reject → timeout → escalate |
| T-027 | pending/ audit trail for approval actions | | backlog | Write proposed action to pending/{timestamp}-{slug}.md, never delete |
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
| T-032 | Slack: channel history read (beyond current thread) | | backlog | conversations.history for broader context |
| T-033 | Slack: DM support (send/receive) | | backlog | For approvals, onboarding, private nudges |
| T-034 | Slack: user ID → display name resolution | | todo | Currently shows raw user IDs like U0AGMGKL19B |
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
| T-044 | Decision log maintenance | | backlog | When a decision is made in a thread, capture it to memory with reasoning and participants |

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
| T-049 | Error recovery + reconnection logic | | todo | Socket drops, API failures, LLM timeouts — graceful handling |
| T-050 | Structured logging + observability | | backlog | Tracing spans, token usage tracking, cost dashboards |

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
| T-056 | Kill hallucinated capabilities (ongoing) | | in-progress | SKILLS.md registry approach |

---

## How to use this file

- **Add a task:** Append a row to the relevant epic table
- **New epic:** Add a new `## E-XXX` section
- **Update status:** Change the status cell
- **Delegate bot** can read and write this file via `read_file` and `write_file`
