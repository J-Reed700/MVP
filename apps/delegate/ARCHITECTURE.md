# Delegate — Architecture

**Not a fork. An adaptation for teams.**

The *Claw ecosystem (OpenClaw, ZeroClaw, IronClaw) proved a set of principles for autonomous AI agents. We adopt the ones that hold up, diverge where our context demands it, and are honest about what we haven't solved yet.

---

## What We're Taking from the *Claws

### Filesystem is memory

Memory is plain files. Markdown, structured text, append-only logs. Files are diffable, greppable, version-controllable, and human-readable. OpenClaw proved this at scale — Markdown files are canonical, with any indexes treated as derived, rebuildable caches.

We adopt this directly. The filesystem is the source of truth. Any future indexes (SQLite, FTS) are acceleration layers, not primary storage.

### Daily logs as append-only event ledger

Every interaction, observation, and action appends to a daily log. One file per day. Never modified after write. Timestamped with source context (channel, user, tool).

This is universal across the *Claw ecosystem and is the closest thing to an uncontroversial design choice in this document.

### MEMORY.md as index

`MEMORY.md` is a structured index pointing to topic-specific memory files. Always loaded into the agent's context window. The agent maintains this itself — when it learns something worth retaining, it updates the relevant file and ensures `MEMORY.md` references it.

OpenClaw uses this pattern along with `SOUL.md` (identity/boundaries), `USER.md`, `AGENTS.md`, and `TOOLS.md`. We adopt the index concept. The specific file taxonomy will emerge from use rather than being prescribed upfront.

### Heartbeat + cron scheduling

Two primitives, distinct purposes:

**Heartbeat** (configurable interval, starting at 5 min): The agent wakes, scans connected tools for changes, and decides if anything needs attention. Most ticks are no-ops. This is the mechanical driver for proactive behavior.

**Cron** (scheduled): Precise timing for recurring outputs. Standup at 9:15am. Stakeholder update Friday at 4pm. Configured per-team.

OpenClaw defaults to 30-minute heartbeats. ZeroClaw matches this. We start at 5 minutes because a PM agent needs tighter awareness than a personal assistant, but this is tunable per-team.

### Webhook fast path

The heartbeat is not the primary awareness mechanism — webhooks are. Events from Slack, Jira, Notion, and other connected tools hit an HTTP endpoint in the web layer. A two-tier triage pipeline classifies each event:

**Tier 0 — Structural filtering (free, no model).** Pattern matching on event metadata. Bot messages → ignore. Automated CI notifications → ignore. Events from channels not in the configured watch list → ignore. This eliminates 60-70% of noise before any reasoning happens.

**Tier 1 — Intent-aware classification (cheap model, fractions of a cent).** Events that pass Tier 0 are classified by a fast, cheap model (Haiku-class) that reads the event plus a compressed intent summary (~500 tokens distilled from INTENTS.md). The model classifies into three buckets:

- **Ignore**: Noise that requires context to recognize. A message mentioning "blocked" that's actually "I blocked that PR" not "I'm blocked." Logged for audit.
- **Log and queue**: Interesting but not urgent. New comments on watched tickets, channel messages touching active intents. Appended to the daily log. Picked up on the next heartbeat tick for batched reasoning.
- **Act now**: Requires immediate response. Direct @mention, approval request, a signal that matches a watched concern in an active intent. Triggers immediate full context assembly and reasoning.

The cheap model makes Tier 1 intent-aware without paying full reasoning cost. "Sarah is out tomorrow" is noise without intent context but a critical signal when INTENTS.md says she's a single point of failure on the billing migration. Pure pattern matching can't make this distinction. A cheap model reading a 500-token intent summary can.

This redefines the heartbeat's role. On each tick, the heartbeat diffs the daily log against its last tick. If nothing has accumulated in "log and queue" since the last tick, the heartbeat is a no-op — no LLM call, no cost. When entries have accumulated, the heartbeat reasons about them as a batch through the lens of active intents: are there patterns? Do multiple signals add up to something worth acting on?

Triage rules (Tier 0 patterns, Tier 1 intent summary refresh cadence) are configured in `HEARTBEAT.md` alongside heartbeat interval and cron schedules.

### HEARTBEAT.md as control surface

The team-editable config for what the agent pays attention to. What sources to check, what patterns to watch for, what thresholds trigger action. Triage classification rules for the webhook fast path also live here. This is how proactive behavior stays steerable rather than magical.

---

## Where We Deliberately Diverge

### No vector search (the *Claws use it — we don't)

This is our bet, not theirs. OpenClaw uses 70/30 vector/BM25 hybrid search. ZeroClaw does the same in SQLite with FTS5 + vector BLOBs. Both maintain embedding indexes.

We're rejecting this for retrieval. Our reasoning:

- PM data is structured and keyword-rich. Project names, ticket IDs, people's names, dates — these are exact-match problems, not semantic similarity problems.
- Grep-based retrieval with LLM-driven multi-turn refinement is more debuggable. When retrieval fails, you can see exactly what was searched.
- No embedding model dependency. No drift between what was embedded and what the current model understands. No indexing infrastructure.
- The LLM is a better semantic layer than cosine similarity. It picks search terms, reads results, and refines.

**The risk:** This may not hold for large workspaces with months of history. If grep becomes a bottleneck, we add SQLite FTS as an acceleration layer behind the same retrieval interface. The filesystem remains canonical.

### Team agent, not personal agent

The *Claws are built for one person, one agent, local-first. This is the fundamental divergence and the source of most of our open engineering problems (see below).

What changes:
- Daily logs contain interactions from multiple people with attribution
- Memory files contain knowledge contributed by everyone
- Permission boundaries determine who can ask about what
- The workspace is cloud-hosted, not on someone's laptop
- Multiple processes may read/write concurrently

### Security model closer to IronClaw than OpenClaw

OpenClaw stores credentials in plaintext JSON. Its January 2026 audit found 512 vulnerabilities, 8 critical. 41% of ClawHub skills contain vulnerabilities. This is not acceptable for a team agent handling Slack tokens, Jira API keys, and potentially customer data.

We take IronClaw's principles: credentials injected at execution boundary, never exposed to agent logic. Capability-based permissions for tool access. Audit trail for every external action. We may not need WASM sandboxing at MVP, but the trust boundary between agent reasoning and credential access must exist from day one.

---

## Context Assembly — The Core Product

Everything else in this document is infrastructure. This section is the product. The quality of Delegate is entirely determined by what goes into the context window on every LLM call.

Every call to a frontier model starts from zero. The model has no memory, no instincts, no persistent understanding of the team. The only thing that makes it a PM is what we load into the window. Context assembly is the process of turning raw workspace state into a prompt that makes a general-purpose model produce PM-quality output.

### Always-loaded context (the lens)

Three files are loaded into every LLM call, unconditionally. Together they form the lens through which the model interprets everything else:

**IDENTITY.md** — Who the agent is and who the team is. Not structured config — narrative prose. Written the way a great PM would brief their replacement:

> Sarah will tell you everything is fine when it isn't. Watch her commit frequency — when she goes quiet for more than 2 days, something is wrong. Don't ask her directly, check with Marcus first.
>
> Jordan (VP Sales) treats every timeline as a promise. Never give him a date without a qualifier.
>
> This team had a bad billing incident in November. Any change touching payments will get extra scrutiny and that's appropriate.

This carries subtext, relationships, and judgment that structured data cannot. The model reads narrative better than it reads YAML. IDENTITY.md is populated during bootstrap onboarding and refined continuously as the agent learns about the team.

**INTENTS.md** — What the team is trying to accomplish right now and how it's going. This is the file that turns a generic assistant into a PM with priorities:

> ## Billing Migration (critical, trending behind)
> Ship by March 15. Blocks enterprise tier, which Sales is already selling for Q2. Sarah is the only one who knows the legacy schema — single point of failure. The data model refactor just got approved (good news) but Marcus's API team is now the bottleneck. Last migration went badly; team trust is fragile.
>
> Watch for: anything touching Sarah's availability, API team velocity, enterprise deal pipeline movement. The VP of Sales will start asking about this within two weeks — get ahead of it.
>
> ## Q2 Planning (background, stable)
> Stakeholders aligned on priorities. Resource plan still missing — nudge but don't push.

Each intent carries: what it is, why it matters, who's involved, what signals to watch, and the current emotional trajectory (stable/improving/declining/critical). Written as narrative, not structured data, because the model uses it as a reasoning lens, not as data to parse.

INTENTS.md is seeded during onboarding ("what's the most important thing happening right now?"), updated by the agent as it observes events, and correctable by the team at any time.

**MEMORY.md** — The knowledge index. Points to topic-specific memory files. Unchanged from current design.

### Intent-biased retrieval

When assembling context for a specific call, the agent retrieves relevant log entries and memory files. Retrieval is biased by active intents — not just "what's textually relevant to the query" but "what's relevant given what we're trying to accomplish."

Example: a user asks "how's the API team doing?" Without intent bias, retrieval pulls recent API team activity. With intent bias, it also pulls billing migration context — because the API team matters right now *because* they're on the critical path for the migration. The model gets the connection without having to discover it.

Mechanically: search queries are augmented with keywords and context from active intents. If INTENTS.md mentions "API team velocity" as a watch signal for the billing migration, a query about the API team also searches for billing migration context. This is cheap — it's additional grep queries, not additional LLM calls.

### Audience-aware framing

The final section of every assembled context is task framing — what the model should produce, for whom, in what style. This is where "writes like a person who's been in the room" gets architectural support.

The framing changes based on:
- **Output type**: standup summary, stakeholder update, ticket comment, DM response, PRD section — each has different conventions
- **Audience**: engineers get technical specifics and brevity; executives get outcomes, risks, and recommendations; the team channel gets informal updates
- **Intent trajectory**: if the active intent is declining, the framing biases toward surfacing risk; if stable, toward concise status; if improving, toward acknowledging progress

Framing templates live in IDENTITY.md as part of the team briefing ("this team values brevity," "Jordan wants outcomes not process," "engineers hate being micromanaged"). The context compiler selects the appropriate framing based on the output type and audience for each specific call.

### Token budget allocation

The context window is finite. When it's tight, the compiler must decide what to cut. The priority order:

1. **IDENTITY.md** — Never cut. The model must always know who it is.
2. **INTENTS.md** — Never cut. The model must always know what matters.
3. **Task framing** — Never cut. The model must know what it's being asked to do.
4. **Current trigger** — Never cut. The event or message being responded to.
5. **Relevant history** — Compressed or truncated when tight. Summaries replace full entries. Older entries dropped before newer ones.
6. **MEMORY.md** — Can be truncated to top-level pointers if budget is critically tight.

This ordering is a product decision. It says: we'd rather the model have less history than lose its sense of identity or priorities. A PM who knows who they are and what matters but has spotty history will produce better output than one with perfect history but no point of view.

### What this means for quality

Every spec behavior traces through context assembly:

- **"It knows what's happening"** — Relevant history, selected by intent-biased retrieval, gives the model current project state.
- **"It acts before being asked"** — INTENTS.md tells the model what to watch for. When a trigger matches a watched signal, the model acts with appropriate urgency.
- **"It writes like a person who's been in the room"** — IDENTITY.md carries team dynamics and communication norms. Audience-aware framing shapes the output. The model reads the subtext because the subtext is in the prompt.
- **"It remembers everything"** — Memory files and logs are the raw material. Retrieval surfaces them. Context assembly puts them in front of the model at the right moment.
- **"It earns trust incrementally"** — Autonomy tiers constrain what the model can do. Corrections update IDENTITY.md and INTENTS.md, improving future context assembly.

---

## System Architecture

### Single language: Rust

The earlier draft proposed a Rust core + Go integration layer. The critique is correct: our Rust core as described (read files, run grep, call HTTP APIs) is I/O-bound work that doesn't benefit from Rust's compute advantages. And the Go layer (webhooks, API calls, auth) is the genuinely complex part. A two-language split adds a serialization boundary, two build pipelines, and a hiring constraint for zero performance benefit.

**Decision: all Rust.** The justification:

- ZeroClaw proved the full agent stack works in Rust as a single 3.4 MB binary with <10ms cold start and <5 MB runtime memory.
- Rust's async ecosystem (tokio) handles I/O-bound webhook/API work fine.
- Single binary deployment. One build. One language for contributors to learn.
- If we ever add local computation (on-device inference, complex data processing), we're already in the right language.
- Type safety across the entire stack, including the boundary between agent logic and external integrations.

The web-facing layer (Slack webhooks, API endpoints) uses `axum` or similar. Not a separate service — a module within the same binary.

### Module boundaries (not DDD theater)

The codebase is organized into modules with clear interfaces. Each module exposes a trait that defines its contract. Implementations are swappable. But we're not going to pretend we've done domain modeling we haven't done.

```
delegate/
├── src/
│   ├── context/         # Context assembly — THE PRODUCT
│   │   ├── mod.rs       # ContextCompiler: assembles prompts from workspace state
│   │   ├── selector.rs  # Intent-biased retrieval of relevant logs and memory
│   │   ├── framing.rs   # Audience-aware prompt construction
│   │   └── budget.rs    # Token budget allocation across context sections
│   │
│   ├── triage/          # Event classification
│   │   ├── mod.rs       # Two-tier triage pipeline
│   │   ├── rules.rs     # Tier 0: structural pattern matching (free)
│   │   └── classifier.rs # Tier 1: cheap model + intent summary (fractions of a cent)
│   │
│   ├── agent/           # Core reasoning loop
│   │   ├── mod.rs
│   │   ├── reasoner.rs  # LLM interaction + output parsing
│   │   ├── autonomy.rs  # Action tier enforcement
│   │   └── executor.rs  # Execute approved actions against external tools
│   │
│   ├── memory/          # Storage and retrieval (plumbing)
│   │   ├── mod.rs       # MemoryStore, Retriever traits
│   │   ├── filesystem.rs
│   │   └── grep.rs
│   │
│   ├── integrations/    # External service adapters (plumbing)
│   │   ├── mod.rs       # ExternalService trait
│   │   ├── slack.rs
│   │   ├── jira.rs
│   │   └── notion.rs
│   │
│   ├── scheduler/       # Heartbeat, cron (plumbing)
│   │   ├── mod.rs
│   │   ├── heartbeat.rs
│   │   └── cron.rs
│   │
│   ├── web/             # HTTP layer (plumbing)
│   │   └── mod.rs
│   │
│   └── main.rs
```

The module tree is organized by value, not by infrastructure concern. `context/` and `triage/` are where the product lives — they determine the quality of every LLM call. Everything labeled "plumbing" is necessary but commodity; use well-tested libraries and don't over-engineer.

### Trait contracts (what we can actually promise)

| Trait | Contract | What it hides |
|---|---|---|
| `ContextCompiler` | Given a trigger and token budget, assemble the optimal prompt. Loads identity + intents + retrieved history + framing. Returns structured context ready for model invocation. | Selection heuristics, budget allocation strategy, framing templates. **This is the product.** |
| `MemoryStore` | Read/write/append to named files in a workspace. List files matching a pattern. | Whether it's a local filesystem, cloud storage, or a database pretending to be files. |
| `Retriever` | Given a query string and optional intent bias, return ranked results with source attribution. | Whether it's grep, FTS, or something else. **Caveat:** ranking behavior will differ between implementations. The agent core must not depend on result ordering stability. |
| `Scheduler` | Register recurring tasks (heartbeat, cron). Receive callbacks when they fire. | Timer implementation, persistence of schedule state. |
| `ExternalService` | Receive events from an external tool. Execute actions against it. | Auth, rate limiting, retry logic, API specifics. |
| `Reasoner` | Given compiled context, return a structured response with classified actions. | Which LLM, what provider, token management. |

The `ContextCompiler` is the only trait that is genuinely the product. Every other trait is plumbing. Engineering effort should be allocated accordingly — 70% of iteration on context assembly, 30% on everything else.

**Honest caveat on swappability:** Swapping `Retriever` from grep to FTS changes ranking behavior and optimal query strategies. Swapping `MemoryStore` from filesystem to S3 changes consistency guarantees. Swapping `Scheduler` from polling to event-driven changes the agent's reasoning pattern (batch vs. incremental). These traits give us clean seams for refactoring. They do not make swaps free. Any swap requires integration testing against the agent's actual behavior, not just the trait contract.

---

## Workspace Layout

```
delegate-workspace/
├── IDENTITY.md            # Who you are, who the team is, how they work (narrative, always in context)
├── INTENTS.md             # Active priorities, why they matter, how they're going (narrative, always in context)
├── MEMORY.md              # Index of all knowledge (always in context)
├── HEARTBEAT.md           # Awareness loop config + triage rules (team-editable)
│
├── logs/                  # Append-only daily logs (never modified)
│   ├── 2026-02-25.md
│   └── ...
│
├── memory/                # Agent-organized knowledge files
│   └── (structure emerges from use)
│
├── cron/                  # Scheduled task definitions (TOML)
│   └── *.toml
│
└── pending/               # Audit trail for actions awaiting/completed approval
    └── *.md               # Approval happens in Slack; these files are the record
```

For multi-instance deployments, shared organizational memory is a separate workspace that multiple Delegate instances can read from. The sync/mount mechanism is unspecified — see open problems.

---

## Bootstrap Pipeline

The spec promises day-one value. An empty workspace knows nothing. Bootstrap is three stages:

### 1. Guided onboarding (~15 min, interactive)

Delegate initiates a Slack DM with the team lead. Two kinds of questions:

**Team context** (populates IDENTITY.md):
- Team composition — who does what, who approves what
- Communication norms — standup time, update cadence, preferred channels
- How people like to be communicated with — who wants details, who wants summaries, who hates being pinged

**Active intents** (populates INTENTS.md):
- "What's the most important thing happening on your team right now?"
- "What keeps you up at night about it?"
- "Who's going to ask you about this, and what do they care about?"
- "What would make you say this is going badly vs. on track?"
- "What happened last time something like this was tried?"

These aren't generic project questions. They're intent questions — they capture *why* things matter, *who* cares, and *what to watch for*. The answers become the lens that shapes every future LLM call.

Responses populate `IDENTITY.md` (team dynamics, communication norms), `INTENTS.md` (active priorities with emotional valence), and seed `MEMORY.md` with initial knowledge pointers. The agent is minimally functional after this stage.

### 2. Background ingestion (async)

While the agent is already responding to questions in degraded mode:
- Last 2 weeks of Slack messages from configured channels
- All open Jira/Linear tickets and their recent activity
- Linked Notion/Confluence pages

Each ingested item appends to the daily log with an `[ingestion]` source tag. The agent builds its knowledge files progressively. Users see a status indicator in the team channel ("Still catching up — 60% through Slack history").

### 3. Validation (~5 min, interactive)

After ingestion completes, Delegate posts a "here's what I think is happening" summary to the team channel:
- Active projects and their apparent status
- Key people and their roles
- Pending decisions and blockers it has identified
- Anything it's confused about

The team corrects errors. Corrections are treated as high-confidence memory updates. This is the transition from "learning" to "useful."

---

## Approval Workflow

Actions in the "Requires Human Approval" tier follow this pipeline:

1. **Propose**: Delegate writes the proposed action to `pending/{timestamp}-{slug}.md` with full context: what it wants to do, why, what it expects to happen, and what could go wrong. This file is the audit trail.

2. **Request**: Delegate DMs the designated approver in Slack with a summary and a link to the full proposal. Two reactions available: approve or reject.

3. **Timeout**: If no response within the configured timeout (default: 4 hours), Delegate escalates to the backup approver with the same message. If the backup also times out, the action expires.

4. **Resolution**: On approve, Delegate executes the action and updates the pending file with the outcome. On reject, it logs the rejection with any feedback. On expiry, it logs the timeout and notifies the team channel.

The pending file is never deleted — it's the permanent record of what was proposed, who approved/rejected it, and what happened. This is the audit trail for every non-autonomous action.

---

## Open Problems and Decisions

These are engineering problems that affect whether the system works. Items marked **decided** have a chosen approach. Items marked **open** still need resolution.

### 1. Concurrency — decided

**Decision: Single-writer event queue.**

All mutations are serialized through a `tokio::mpsc` channel. One writer task owns all filesystem writes — daily log appends, memory file updates, pending action files. Reads are lock-free against the filesystem.

This is acceptable because:
- Daily logs are append-only. A stale read just means you haven't seen the latest entry yet.
- Memory files change infrequently relative to read frequency.
- The writer task is the only bottleneck, and filesystem writes are fast.

The tradeoff is write latency (mutations queue behind each other), but for our write volume (dozens per hour, not thousands per second) this is a non-issue.

### 2. Context compaction — decided

**Decision: Stateless ticks. No compaction.**

Each heartbeat tick and each user interaction is a fresh LLM call. There is no persistent session to compact. Context is assembled per-call by the `ContextCompiler` from:
- `IDENTITY.md` + `INTENTS.md` + `MEMORY.md` (always loaded — the lens)
- `HEARTBEAT.md` (always loaded — scheduling and triage config)
- Relevant log entries and memory files (loaded on demand via intent-biased retrieval)
- The immediate interaction context (user message, webhook event, accumulated queue entries)
- Audience-aware task framing (selected based on output type and recipient)

This eliminates the Summer Yue risk entirely — safety instructions cannot be compacted away because there is no persistent session to compact. Every call starts with the full identity and safety context.

The tradeoff is that the agent has no "working memory" between ticks. Anything it needs to remember must be written to a file. This is a feature, not a bug — it forces the agent to externalize its state, making the system debuggable and auditable.

### 3. Bootstrap — decided

See [Bootstrap Pipeline](#bootstrap-pipeline) above. Three-stage approach: guided onboarding, background ingestion, validation.

### 4. Cost model — decided

**Decision: Four-tier processing with daily token budget.**

Event processing has four cost tiers:
- **Tier 0 triage** (structural filtering): Zero tokens. Pattern matching on event metadata.
- **Tier 1 triage** (intent-aware classification): Cheap model, ~100-200 tokens per event. Reads event + compressed intent summary. Fractions of a cent per classification.
- **Heartbeat batch**: Zero tokens if nothing changed since last tick. When entries have accumulated, one full reasoning call covers all queued events with intent-biased context.
- **Full reasoning**: On "act now" events and direct user questions. Full context assembly (IDENTITY.md + INTENTS.md + retrieved history + framing) and multi-turn retrieval if needed.

Daily token budget defaults to 500K tokens/day (~$2-5/day depending on model). When the budget is exhausted, Delegate enters log-only mode: it continues to receive and triage events, but does not make LLM calls. A Slack notification alerts the team. Budget resets at midnight team-local time.

Projected call volume: ~40-70 LLM calls/day for a typical team, not 288. Most heartbeat ticks are no-ops. Most webhook events are triaged without LLM involvement.

### 5. Permission model — open

The *Claws have no RBAC. OpenClaw's multi-user story is broken — memory bleeds between users, agents can see each other's sessions. We need:
- What the agent can share with whom
- What memory is visible to which team members
- How audit trails map to permission boundaries

Not specifying a model yet. MVP can start with a simple model (all team members see everything, approvers are configured per-team) but this cannot be bolted on later — it affects the memory layer, the retrieval layer, and the action pipeline.

### 6. Shared memory across instances — open (deferred)

The spec describes a mesh of Delegate roles sharing organizational knowledge. This requires:
- A shared workspace readable by multiple instances
- Conflict resolution when two instances write to the same knowledge
- Consistency guarantees (how stale can shared reads be?)

This is a v2 problem. MVP is a single PM instance per team.

---

## Deferred Decisions

### Meeting transcript processing

The spec originally promised that Delegate would process meeting transcripts into structured action items. This is deferred to post-MVP.

What it would require:
- **Transcript source**: Integration with a recording/transcription service (Otter, Fireflies, Google Meet transcription, etc.) or raw audio processing
- **Extraction pipeline**: Speaker identification, action item extraction, decision extraction, topic segmentation — each a non-trivial NLP/LLM task
- **Speaker ID mapping**: Matching transcript speakers to known team members, handling nicknames, guests, and poor transcription quality
- **Confidence handling**: Transcripts are noisy. The agent needs to distinguish high-confidence extractions from guesses and flag uncertainty

This is a meaningful feature but not required for the PM agent to deliver day-one value. Calendar integration at MVP scope (schedule awareness + agenda prep) is sufficient.
