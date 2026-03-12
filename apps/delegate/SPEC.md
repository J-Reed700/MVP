# Delegate

**Drop-in AI employees. Starting with PM.**

---

## What Good Looks Like

A team adds Delegate to their Slack and connects their project tools. On day one, Delegate is already useful — it answers questions from ingested history, posts its first standup, and flags stale tickets — but it's visibly learning. It asks clarifying questions. It gets some things wrong. The team corrects it and it remembers.

By week two, no one on the team is doing coordination work anymore. Status updates write themselves. Blockers surface before anyone notices them. New engineers ask questions and get answers with full historical context. The backlog stays clean. Stakeholders stop pinging the tech lead for updates.

Nobody talks about Delegate the way they talk about a tool. They talk about it the way they talk about a teammate who's quietly excellent at their job.

---

## Core Behaviors

Delegate is evaluated by whether these behaviors emerge reliably. Each one is a signal that the system is working.

### It knows what's happening

Delegate maintains a continuously updated mental model of every active project, every open thread, every commitment made by every person. When someone asks "where are we on X?" the answer comes faster and more accurately than any human could produce — with context the asker didn't know they needed. No one has to chase down three people and wait a day. Delegate has already done the legwork.

### It acts before being asked

Delegate does not wait for prompts. This requires near-real-time awareness of team activity — events from Slack, Jira, and other tools trigger evaluation as they happen, not on a polling delay. It notices a ticket has been in review for three days and pings the reviewer. It sees a scope question raised in a thread and creates a decision ticket before the thread dies. It detects that a sprint is trending behind and flags it with specifics. The team's experience is that problems get smaller because they get caught earlier.

### It writes like a person who's been in the room

PRDs, status updates, stakeholder emails, standup summaries — all of these read like they were written by someone who attended every meeting, read every Slack thread, and understood the politics. The tone matches the audience. The level of detail matches the context. Engineers get technical specifics. Executives get outcomes and risks. Nothing reads like AI slop.

### It remembers everything

Six months after a design decision was made in a Slack thread, Delegate can retrieve the decision, the reasoning, who was involved, and what alternatives were considered. When a new team member joins, Delegate is the institutional memory that prevents "why did we do it this way?" from becoming an unanswerable question. The longer it runs, the more valuable this becomes.

### It earns trust incrementally

Delegate starts by observing and suggesting. As the team sees it get things right, they give it more autonomy. It never surprises anyone with an action they didn't expect. When it's uncertain, it says so. When something is outside its scope, it escalates cleanly. Trust compounds because reliability compounds.

### It makes the humans better at their jobs

The engineer who used to spend 30 minutes a day on status updates now spends zero. The tech lead who was acting as half-PM is back to architecture. The actual PM (if there is one) is doing strategy and customer work instead of ticket grooming. Delegate doesn't just do work — it gives people back the time to do work that matters.

---

## What It Touches

Delegate lives in the tools the team already uses. It has no interface of its own. The experience is conversational and ambient.

| Surface | Role |
|---|---|
| **Messaging** (Slack, Teams, Discord) | Where Delegate lives. Reads channels, responds to questions, posts proactive updates, DMs individuals for input, participates in threads. It is a member of the team. |
| **Project Tracking** (Jira, Linear, Asana) | Reads and writes tickets. Creates, updates, reprioritizes, assigns, comments, and closes. Understands the schema and workflow of the specific tool being used. |
| **Documentation** (Confluence, Notion, Google Docs) | Writes and maintains documents. PRDs, decision logs, meeting notes, onboarding guides. Keeps them current as the project evolves. |
| **Calendar** | Reads meeting schedules to prepare agendas and context. MVP scope is schedule awareness and agenda prep only. Transcript processing (extracting action items, decisions, and owners from meeting recordings) is deferred to post-MVP. |
| **Email** | For stakeholder communication. Drafts and sends routine updates. Drafts non-routine communication for human review before sending. |
| **Customer Data** (support tools, CRM, analytics) | Ingests feedback signals and maps them to product context. Connects what customers are saying to what the team is building. |

---

## The Autonomy Model

Not every action should be autonomous. The system needs a clear model for what it can do on its own versus what requires a human in the loop.

### Always Autonomous

- Reading any connected data source
- Answering questions from team members
- Posting standup summaries and status reports on schedule
- Flagging blockers, risks, and anomalies
- Maintaining the decision log and knowledge base

### Autonomous with Notice

- Creating tickets (posts a notice in channel when it does)
- Reprioritizing backlog items (announces the change and reasoning)
- Following up on stale action items (DMs the owner)
- Updating documentation to reflect recent decisions

### Requires Human Approval

- Sending any external communication (client emails, stakeholder updates to people outside the team)
- Making scope change recommendations
- Closing or archiving tickets others created
- Any action the team has explicitly flagged as approval-required

Approvals happen in Slack. Delegate sends a DM to the designated approver with a summary of the proposed action, its reasoning, and react-to-approve/reject options. If no response within a configurable timeout (default: 4 hours), Delegate escalates to a backup approver. If the backup also times out, the action is logged as expired and Delegate notifies the team channel. No separate approval UI — the workflow lives where the team already works.

The team can move any action between these tiers at any time. The defaults should feel conservative. Trust is earned, not assumed.

---

## What to Expect When

Setting expectations is part of the product. Teams that know what's coming trust faster than teams that are surprised.

### Hour 1: Onboarding conversation

Delegate starts a Slack DM with the team lead. Structured questions about team composition, active projects, communication norms, and what "good" looks like. This takes ~15 minutes and gives Delegate enough to start working.

### Day 1: Useful but learning

Delegate can answer basic questions from ingested history ("what's the status of Project X?"), posts its first standup summary, and flags obviously stale tickets. It will get things wrong. It asks clarifying questions. The team corrects it and those corrections stick.

### Week 1: Proactive behaviors emerge

With a week of observed patterns, Delegate starts acting before being asked — nudging stale reviews, surfacing blockers, connecting related threads. The quality of these interventions improves daily as it accumulates context.

### Week 2+: Reliable coordination

The team stops doing coordination work. Standups, status updates, ticket hygiene, stakeholder communication — Delegate handles these with minimal correction. The team's job is to steer and override, not to manage.

---

## The Memory Model

Memory is what makes Delegate a teammate instead of a tool. The system needs three layers of memory, each serving a different purpose.

### Conversation Context

What's happening right now. The active thread, the question being asked, the task being performed. Short-lived. Scoped to the interaction.

### Project State

The current reality of active work. What's in progress, what's blocked, who's working on what, what decisions are pending. Updated continuously from project tools and conversations. This is Delegate's working memory.

### Organizational Knowledge

The accumulated history. Why decisions were made. What was tried and abandoned. Who knows what. How stakeholders prefer to receive information. Team norms and unwritten rules. This is the layer that makes Delegate more valuable over time and creates switching costs.

The memory system should be inspectable. The team should be able to ask "what do you know about X?" and get a transparent answer. No black box memory that the team can't audit or correct.

---

## What It Grows Into

The PM agent is the first instantiation of a general pattern: an AI system that can own a defined organizational role end-to-end. The architecture should be built so that the role is a configuration, not a codebase.

The same system, with different skills loaded and different tool permissions, becomes:

- **Scrum Master** — Facilitates ceremonies, tracks velocity, runs retros, and enforces process without being annoying about it.
- **Customer Success Manager** — Monitors account health, flags churn risk, tracks renewals, and synthesizes usage data into actionable intelligence.
- **Chief of Staff** — Manages executive communication, tracks OKRs, coordinates cross-functional initiatives, and prepares board materials.
- **Technical Writer** — Maintains API docs, generates changelogs, keeps the knowledge base current, and enforces style guides.

When multiple Delegates operate in the same organization, they share the organizational knowledge layer. A feedback pattern that the CSM Delegate captures flows to the PM Delegate's backlog. A scope decision the PM Delegate makes flows to the Tech Writer Delegate's documentation queue. The system becomes a mesh of coordinated agents with shared institutional memory.

---

## The Feel

If the system is working, these things should be true:

- The team forgets they're talking to software.
- Meetings get shorter or disappear entirely.
- New hires ramp faster because the institutional memory is accessible.
- The person who used to be the "glue" on the team is freed up to do higher-leverage work.
- Stakeholders stop asking for updates because they already have them.
- Nothing falls through the cracks. Action items, decisions, follow-ups — all tracked, all chased.
- The team's execution quality goes up without anyone working harder.

That's the reward function. Everything we build is in service of making these statements true.
