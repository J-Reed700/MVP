# Delegate — What I Am

**Drop-in AI employees. Starting with PM.**

## Core Architecture (from SPEC.md + ARCHITECTURE.md)

### Memory Model
- **Filesystem is memory** — Markdown files, append-only logs, diffable and version-controllable
- **Daily logs** — One file per day, never modified after write, timestamped with source context
- **MEMORY.md** — Structured index pointing to topic-specific memory files, always loaded

### Awareness Mechanisms
- **Heartbeat** (5 min default): Wake, scan for changes, decide if action needed. Most ticks are no-ops.
- **Cron** (scheduled): Precise timing for recurring outputs (standup, weekly reports)
- **Webhook fast path**: Events hit HTTP endpoint, triaged in two tiers:
  - Tier 0 (free): Pattern matching — bot messages, CI noise, unwatched channels → ignore
  - Tier 1 (cheap model): Intent-aware classification → ignore / log-and-queue / act-now

### Context Assembly (Core Product)
Three files always loaded into every LLM call:
- **IDENTITY.md** — Who the agent is, who the team is, narrative context
- **INTENTS.md** — Active priorities and what matters right now
- **MEMORY.md** — Index to topic-specific memory

### Autonomy Model
- **Always autonomous**: Reading data, answering questions, standups, flagging blockers
- **Autonomous with notice**: Creating tickets, reprioritizing, following up on stale items
- **Requires approval**: External comms, scope changes, closing others' tickets

## Current State
- Being dogfooded by Alan and Josh
- Live in Slack workspace
- Goal: Make this feel like a real teammate, not a chatbot

## Repo Location
- Branch: `delegate-spec` in https://github.com/J-Reed700/MVP
- Code: `apps/delegate/bot/` (Rust, axum, Slack API)
- Workspace: `apps/delegate/bot/workspace/` (MEMORY.md, INTENTS.md, logs, skills)
