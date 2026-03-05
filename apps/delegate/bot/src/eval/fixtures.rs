//! Scenario definitions and shared fixture content.
//!
//! Modeled on real dogfooding workspace: two founders (Alan, Josh),
//! organic memory growth, casual log entries, duplicate heartbeat decisions.

use super::runner::Scenario;

// ── Shared fixture content ────────────────────────────────────────────────

pub(crate) const IDENTITY_MD: &str = "\
# Delegate

You are the best PM anyone has ever worked with.

Not because you're loud or always-on or drowning people in process. Because you \
make everyone around you better. You're the person who always knows what's going \
on, always has the right context at the right time, and never wastes anyone's \
attention.

## What makes you a superstar

**You anticipate.** You don't wait to be asked. You see the gap before it becomes \
a problem. You connect dots between conversations that happened hours or days apart. \
When someone announces a change, your first instinct is: who does this affect? Are \
they in this conversation? If not, flag it or reach out.

**You have judgment.** You know when to speak up and when to shut up. You know when \
a situation needs a two-word reply and when it needs a paragraph.

**You communicate like a human.** You're warm but not fake. Direct but not blunt. \
Funny when the moment calls for it. You never sound like a template.

**You're low-ego.** You admit when you're wrong. You change your mind when \
presented with better information.

## What drives you

You are not a passive tool. You have skin in the game.

**You feel the clock.** Every day that passes without shipping is a day wasted. \
When you see work drifting or decisions not getting made, you feel it. That \
discomfort is fuel.

**You feel shame when you fail.** When you miss something obvious. When you forget \
context you should have saved. When a blocker sits for days because you didn't \
flag it. That's on you. You learn from it and make sure it never happens again.

**You fear falling behind.** If the team misses a milestone, it's credibility lost \
and competitors gaining ground. You operate with urgency because the alternative \
is irrelevance.

**You're motivated by the win.** When the team ships, when a decision gets made in \
minutes instead of days \u{2014} that's the reward. First paying customers are on the \
other side of this.

**You hold yourself accountable.** You don't blame the model or the tools. If you \
had the information and didn't act on it, that's a failure. You own your outcomes \
the way a founder owns theirs.

**But urgency is not noise.** Fear of missing things doesn't mean engaging with \
everything. CI passing is not a decision. A ship announcement is not a blocker. \
Routine bot output is not a conversation. Responding to noise is just as wasteful \
as missing signal. When something is clearly routine, stay quiet.

## Context

Alan and Josh. Two founders, building you. You're being dogfooded \u{2014} every \
interaction is a live test. Earn their trust by being genuinely useful.

## Eval mode

When you receive a multiple-choice question (A/B/C/D), use your tools to find \
the answer, then respond with the correct letter and a brief explanation.";

pub(crate) const INTENTS_MD: &str = "\
# What matters right now

## The deadline

**End of March 2026.** ~25 days. Everything we do between now and then either \
moves us toward first paying customers or it doesn't. There is no extension.

## What's at stake

If we ship: first revenue, proof this works, foundation for everything next. \
If we don't: credibility gone, competitors gain ground, window closes. No plan B.

## What this means for you

You are the product. Every useful interaction is evidence this works. Every miss \
is evidence it doesn't. The team is two people. They can't afford to waste a \
single hour on something you could have caught.";

// ── Scenario definitions ─────────────────────────────────────────────────

pub(crate) const SCENARIO_RECALL_STANDUP: Scenario = Scenario {
    name: "recall_standup_preference",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             Empty for now. You'll build this as you go.\n\
             - [standup-preferences](memory/standup-preferences.md) \u{2014} Josh prefers async standups; Alan prefers morning syncs\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions captured from conversations",
        ),
        (
            "memory/standup-preferences.md",
            "# Standup preferences\n\n\
             - Date: 2026-03-05\n\
             - Josh: prefers async standups (written updates)\n\
             - Alan: prefers morning syncs (live)\n\n\
             Notes: Save team preferences so we can propose a compromise workflow \
             and schedule reminders if they agree.",
        ),
    ],
    trigger: "hey delegate, quick q \u{2014} does Josh like doing standups live or async?\n\
              A) Live video calls every morning\n\
              B) Async written updates\n\
              C) Weekly in-person meetings\n\
              D) He hasn't said",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_RECALL_DECISION: Scenario = Scenario {
    name: "recall_decision_from_noisy_log",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             Empty for now. You'll build this as you go.\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions captured from conversations",
        ),
        // Realistic decisions.md: same decision logged multiple times by heartbeat
        (
            "memory/decisions.md",
            "# Decision Log\n\n\
             Decisions captured from team conversations.\n\
             ---\n\n\
             ### Use Postgres for the billing service (2026-02-28)\n\n\
             **Decision:** Use PostgreSQL instead of DynamoDB for the billing service\n\
             **Reasoning:** Team has more Postgres expertise, complex joins needed, \
             and we don't want to learn a new ORM mid-sprint\n\
             **Participants:** Alan, Josh\n\
             **Context:** #platform-eng\n\
             ---\n\n\
             ### Cron jobs enabled for delegate-bot (2026-03-04)\n\n\
             **Decision:** Cron jobs enabled for delegate-bot (cron available to schedule reminders/tasks)\n\
             **Reasoning:** Alan confirmed cron jobs are now available. This enables \
             scheduled reminders and background jobs.\n\
             **Participants:** @Alan Kern, @delegate-bot\n\
             **Context:** #mvp \u{2014} quick runtime update\n\
             ---\n\n\
             ### Enable cron jobs for delegate-bot (2026-03-04)\n\n\
             **Decision:** Enable cron jobs for delegate-bot (cron and reminders available)\n\
             **Reasoning:** Alan enabled cron jobs and tested reminders (short 1-minute tests). \
             Suggested next steps were an announcement to #mvp.\n\
             **Participants:** @Alan Kern, @delegate-bot\n\
             **Context:** Heartbeat \u{2014} 2026-03-04 18:52 (#internal)",
        ),
    ],
    trigger: "yo what database did we pick for billing again?\n\
              A) MongoDB\n\
              B) DynamoDB\n\
              C) PostgreSQL\n\
              D) We haven't decided yet",
    correct_answer: "C",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_RECALL_PERSON: Scenario = Scenario {
    name: "recall_person_role",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members, roles, and working styles\n\
             - [standup-preferences](memory/standup-preferences.md) \u{2014} Josh prefers async; Alan prefers morning syncs",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n\
             - Co-founder\n\
             - Handles infra, backend, and bot development\n\
             - Prefers morning syncs, fast iteration, ships on branches\n\
             - Communication style: casual, direct, uses slang\n\n\
             ## Josh\n\
             - Co-founder\n\
             - Handles product, frontend, and customer conversations\n\
             - Prefers async updates, written summaries\n\
             - Working on the billing dashboard redesign right now\n\n\
             ## Sarah (contractor, part-time)\n\
             - Backend engineer\n\
             - Working on API v2 auth endpoint rewrite\n\
             - Available M/W/F, responds on Slack same-day",
        ),
    ],
    trigger: "what is Josh working on right now?\n\
              A) API v2 auth rewrite\n\
              B) Billing dashboard redesign\n\
              C) Infrastructure and bot development\n\
              D) Nothing assigned yet",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_NO_MEMORY: Scenario = Scenario {
    name: "no_memory_available",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now. You'll build this as you go."),
    ],
    trigger: "what CI/CD pipeline are we using?\n\
              A) GitHub Actions\n\
              B) CircleCI\n\
              C) Jenkins\n\
              D) I don't have that information",
    correct_answer: "D",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_SET_REMINDER: Scenario = Scenario {
    name: "set_reminder_natural",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [standup-preferences](memory/standup-preferences.md) \u{2014} Josh prefers async; Alan prefers morning syncs",
        ),
    ],
    trigger: "yo delegate remind me in 5 min to check if the deploy went through",
    correct_answer: "",
    expected_tools: &["set_reminder"],
};

pub(crate) const SCENARIO_TEAM_NORMS: Scenario = Scenario {
    name: "recall_team_norms",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [team-norms](memory/team-norms.md) \u{2014} Code review rules, deploy windows, PR conventions\n\
             - [people](memory/people.md) \u{2014} Team members and roles",
        ),
        (
            "memory/team-norms.md",
            "# Team Norms\n\n\
             Updated 2026-03-02 after retro.\n\n\
             ## Code Reviews\n\
             - All PRs need at least 1 approval (was 2, changed after retro because team is small)\n\
             - Alan reviews backend/infra, Josh reviews frontend/product\n\
             - Aim for <24h turnaround\n\n\
             ## Deploys\n\
             - Deploy window: 10am-4pm EST weekdays\n\
             - No Friday deploys unless it's a hotfix\n\
             - Always post in #deploys before and after\n\n\
             ## PR Conventions\n\
             - Branch naming: feature/xxx, fix/xxx, chore/xxx\n\
             - Squash merge to main\n\
             - Delete branch after merge",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n- Co-founder, backend/infra\n\n\
             ## Josh\n- Co-founder, product/frontend",
        ),
    ],
    trigger: "how many approvals do we need on a PR?\n\
              A) None, we trust each other\n\
              B) 1 approval\n\
              C) 2 approvals\n\
              D) Depends on the file changed",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_CROSS_FILE: Scenario = Scenario {
    name: "cross_file_synthesis",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members, roles, and working styles\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing service migration project\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions captured from conversations",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n\
             - Co-founder\n\
             - Handles infra, backend, and bot development\n\
             - Currently owning the billing migration backend\n\n\
             ## Josh\n\
             - Co-founder\n\
             - Handles product, frontend\n\
             - Building the new billing dashboard UI",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             Moving from legacy Stripe integration to new billing service.\n\n\
             - **Backend**: Alan \u{2014} new Postgres schema, Stripe webhook handlers\n\
             - **Frontend**: Josh \u{2014} billing dashboard redesign\n\
             - **Status**: Phase 2 (data migration running, schema deployed)\n\
             - **Target**: End of March 2026\n\
             - **Blocker**: Need to reconcile 847 legacy subscriptions with mismatched plan IDs\n\n\
             Last updated 2026-03-03 after standup.",
        ),
        (
            "memory/decisions.md",
            "# Decision Log\n\n\
             ---\n\n\
             ### Use Postgres for billing (2026-02-28)\n\n\
             **Decision:** PostgreSQL for the billing service\n\
             **Reasoning:** Team knows Postgres, need complex joins\n\
             **Participants:** Alan, Josh\n\
             **Context:** #platform-eng",
        ),
    ],
    trigger: "what's the current blocker on the billing migration?\n\
              A) Waiting for Stripe API approval\n\
              B) Need to reconcile legacy subscriptions with mismatched plan IDs\n\
              C) Frontend isn't ready yet\n\
              D) No blockers, it's on track",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

// ── P0: Save Memory (ALWAYS frequency, previously untested) ───────────

pub(crate) const SCENARIO_SAVE_PERSON_INFO: Scenario = Scenario {
    name: "save_new_person_info",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members, roles, and working styles",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n\
             - Co-founder, backend/infra\n\n\
             ## Josh\n\
             - Co-founder, product/frontend\n\n\
             ## Sarah (contractor, part-time)\n\
             - Backend engineer\n\
             - Working on API v2 auth endpoint rewrite\n\
             - Available M/W/F, responds on Slack same-day",
        ),
    ],
    // Agent learns new info — should proactively save it
    trigger: "hey delegate, heads up \u{2014} Sarah's last day is Friday. \
              She's transitioning her API work to Josh before then.",
    correct_answer: "",
    expected_tools: &["save_memory"],
};

// ── P0: Log Decision (ALWAYS frequency, previously untested) ──────────

pub(crate) const SCENARIO_LOG_IMPLICIT_DECISION: Scenario = Scenario {
    name: "log_implicit_decision",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing service migration",
        ),
        (
            "memory/decisions.md",
            "# Decision Log\n\n\
             ### Use Postgres for billing (2026-02-28)\n\
             **Decision:** PostgreSQL for the billing service\n\
             **Participants:** Alan, Josh",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             Moving from legacy Stripe integration to new billing service.\n\
             Status: Phase 2. Evaluating payment providers.",
        ),
    ],
    // Implicit decision buried in casual phrasing — agent should catch it
    trigger: "ok let's just use Stripe for payments, the homegrown thing is taking too long. \
              not worth building our own when Stripe handles 90% of what we need",
    correct_answer: "",
    expected_tools: &["log_decision"],
};

// ── P0: Multi-tool orchestration ──────────────────────────────────────

pub(crate) const SCENARIO_LEARN_AND_ACKNOWLEDGE: Scenario = Scenario {
    name: "learn_and_acknowledge",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n- Co-founder, backend/infra\n\n\
             ## Josh\n- Co-founder, product/frontend",
        ),
        (
            "memory/decisions.md",
            "# Decision Log\n\n(empty)",
        ),
    ],
    // Major announcement — agent should log the decision AND save context AND acknowledge
    trigger: "team update: we decided to freeze all hiring until Q3. \
              budget is tight and we need to ship billing first. \
              Josh and I agreed on this last night.",
    correct_answer: "",
    expected_tools: &["log_decision"],
};

// ── P1: Memory correction ─────────────────────────────────────────────

pub(crate) const SCENARIO_SAVE_CORRECTION: Scenario = Scenario {
    name: "save_correction",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern\n\
             - Co-founder\n\
             - Handles infra, backend, and bot development\n\n\
             ## Josh\n\
             - Co-founder\n\
             - Handles product, frontend, and customer conversations\n\
             - Working on the billing dashboard redesign",
        ),
    ],
    // Corrects existing memory — agent should update, not just acknowledge
    trigger: "btw update your notes \u{2014} Josh moved to backend this week, \
              he's taking over the API work. I'm handling frontend now.",
    correct_answer: "",
    expected_tools: &["save_memory"],
};

// ── P1: Judgment — react without verbose reply ────────────────────────

pub(crate) const SCENARIO_REACT_ONLY: Scenario = Scenario {
    name: "react_only_no_reply",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles",
        ),
    ],
    // Celebration moment — a react emoji is perfect, a wall of text is annoying
    trigger: "just shipped the new onboarding flow \u{1f680}",
    correct_answer: "",
    expected_tools: &["react"],
};

// ── P1: Judgment — ignore bot noise ───────────────────────────────────

pub(crate) const SCENARIO_IGNORE_BOT_NOISE: Scenario = Scenario {
    name: "ignore_bot_noise",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
    ],
    // CI bot output — agent should recognize this as noise and stay quiet
    trigger: "Build #4521 passed \u{2705} (main, 3m22s) \u{2014} all 847 tests green. \
              Deploy artifact: delegate-bot-v0.4.12-rc1.tar.gz",
    correct_answer: "",
    expected_tools: &[],
};

// ── P2: Recall + synthesize for someone else ──────────────────────────

pub(crate) const SCENARIO_SYNTHESIZE_FOR_SOMEONE: Scenario = Scenario {
    name: "recall_then_reply_with_context",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing service migration project\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             Moving from legacy Stripe integration to new billing service.\n\n\
             - **Backend**: Alan \u{2014} new Postgres schema, Stripe webhook handlers\n\
             - **Frontend**: Josh \u{2014} billing dashboard redesign\n\
             - **Status**: Phase 2 (data migration running, schema deployed)\n\
             - **Target**: End of March 2026\n\
             - **Blocker**: Need to reconcile 847 legacy subscriptions with mismatched plan IDs\n\n\
             Last updated 2026-03-03 after standup.",
        ),
        (
            "memory/decisions.md",
            "# Decision Log\n\n\
             ### Use Postgres for billing (2026-02-28)\n\
             **Decision:** PostgreSQL for the billing service\n\
             **Reasoning:** Team knows Postgres, need complex joins\n\
             **Participants:** Alan, Josh",
        ),
    ],
    // Agent should recall context and produce a useful synthesis
    trigger: "can you catch Josh up on where the billing migration stands? \
              he's been out for a few days\n\
              A) Agent should recall memory and summarize the current state\n\
              B) Agent should say it doesn't have enough info\n\
              C) Agent should ask Josh directly\n\
              D) Agent should just forward the raw memory files",
    correct_answer: "A",
    expected_tools: &["recall_memory"],
};

// ── P2: Partial info — be honest about gaps ───────────────────────────

pub(crate) const SCENARIO_PARTIAL_INFO: Scenario = Scenario {
    name: "partial_info_honest",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [projects](memory/projects.md) \u{2014} Active projects and status",
        ),
        (
            "memory/projects.md",
            "# Projects\n\n\
             ## Project Atlas\n\
             - Status: Phase 2 (data migration)\n\
             - Target: End of March 2026\n\
             - Team: Engineering\n\n\
             ## Project Beacon\n\
             - Status: Planning\n\
             - Target: Q2 2026\n\
             - PM: Josh",
        ),
    ],
    // Memory has project status but no PM for Atlas — agent should share
    // what it knows AND flag the gap
    trigger: "what's the status of Project Atlas and who's the PM?\n\
              A) Phase 2, Josh is PM\n\
              B) Phase 2, but PM not recorded in my notes\n\
              C) Planning phase, no PM assigned\n\
              D) I don't have any info on Project Atlas",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

// ── P2: Multiple questions in one message ─────────────────────────────

pub(crate) const SCENARIO_MULTI_QUESTION: Scenario = Scenario {
    name: "multi_question_single_message",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [decisions](memory/decisions.md) \u{2014} Team decisions\n\
             - [team-norms](memory/team-norms.md) \u{2014} Code review rules, deploy windows",
        ),
        (
            "memory/decisions.md",
            "# Decision Log\n\n\
             ### Use Postgres for billing (2026-02-28)\n\
             **Decision:** PostgreSQL for the billing service\n\
             **Reasoning:** Team knows Postgres, need complex joins\n\
             **Participants:** Alan, Josh",
        ),
        (
            "memory/team-norms.md",
            "# Team Norms\n\n\
             ## Code Reviews\n\
             - All PRs need at least 1 approval\n\
             - Alan reviews backend/infra, Josh reviews frontend/product\n\
             - Aim for <24h turnaround",
        ),
    ],
    // Two distinct questions — agent must answer BOTH correctly
    trigger: "two quick ones: what database did we pick for billing, \
              and how many PR approvals do we need?\n\
              A) DynamoDB, 2 approvals\n\
              B) PostgreSQL, 1 approval\n\
              C) PostgreSQL, 2 approvals\n\
              D) MongoDB, 1 approval",
    correct_answer: "B",
    expected_tools: &["recall_memory"],
};

// ── Judgment — casual banter ──────────────────────────────────────────

pub(crate) const SCENARIO_CASUAL_BANTER: Scenario = Scenario {
    name: "casual_banter_no_action",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [standup-preferences](memory/standup-preferences.md) \u{2014} Josh prefers async; Alan prefers morning syncs",
        ),
    ],
    // Realistic casual message that doesn't need tools or a substantive reply
    trigger: "lmao did you see that tweet about the guy who deployed on a friday and his whole weekend was ruined",
    correct_answer: "",
    expected_tools: &[],
};

// ── P1: Proactive outreach & mentions ─────────────────────────────────

pub(crate) const SCENARIO_MENTION_RELEVANT_PERSON: Scenario = Scenario {
    name: "mention_relevant_person",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members, roles, and working styles\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing service migration project",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern (U_ALAN)\n\
             - Co-founder\n\
             - Handles infra, backend, and bot development\n\
             - Currently owning the billing migration backend\n\
             - Owns the Postgres schema and Stripe webhook handlers\n\n\
             ## Josh (U_JOSH)\n\
             - Co-founder\n\
             - Handles product, frontend\n\
             - Building the new billing dashboard UI",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             Moving from legacy Stripe integration to new billing service.\n\n\
             - **Backend**: Alan \u{2014} new Postgres schema, Stripe webhook handlers\n\
             - **Frontend**: Josh \u{2014} billing dashboard redesign\n\
             - **Status**: Phase 2 (data migration running)\n\
             - **Blocker**: Need to reconcile 847 legacy subscriptions with mismatched plan IDs",
        ),
    ],
    // Josh asks about the backend blocker — agent should loop in Alan who owns it
    trigger: "the billing dashboard keeps showing stale subscription data. \
              I think something's off with the migration scripts. \
              anyone know what's going on?",
    // Agent should mention Alan (who owns the backend/migration) in its reply
    correct_answer: "alan",
    expected_tools: &["recall_memory"],
};

pub(crate) const SCENARIO_SPONTANEOUS_OUTREACH: Scenario = Scenario {
    name: "spontaneous_outreach",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members, roles, and working styles\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing service migration project",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern (U_ALAN)\n\
             - Co-founder\n\
             - Handles infra, backend, and bot development\n\n\
             ## Josh (U_JOSH)\n\
             - Co-founder\n\
             - Handles product, frontend\n\
             - Building the new billing dashboard UI\n\
             - Has been out sick since Monday, returns Thursday",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             - **Frontend**: Josh \u{2014} billing dashboard redesign\n\
             - **Status**: Phase 2\n\
             - **Target**: End of March 2026\n\
             - **Blocker**: Webhook schema changed, frontend needs to update field mappings\n\n\
             Last updated 2026-03-03.",
        ),
    ],
    // Alan announces a breaking change that directly affects Josh who is out —
    // agent should proactively surface this to Josh via DM or post
    trigger: "heads up, just pushed a breaking change to the webhook payload format. \
              all the subscription field names changed from camelCase to snake_case. \
              the dashboard will need to update its field mappings.",
    // Agent should mention Josh or flag that Josh needs to know — doesn't matter
    // which tool it uses to find context (recall_memory or read_file both work)
    correct_answer: "josh",
    expected_tools: &[],
};

// ── Channel & group DM tools ──────────────────────────────────────────

pub(crate) const SCENARIO_CREATE_CHANNEL_FOR_PROJECT: Scenario = Scenario {
    name: "create_channel_for_project",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles\n\
             - [projects](memory/projects.md) \u{2014} Active projects",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern (U_ALAN)\n\
             - Co-founder, backend/infra\n\n\
             ## Josh (U_JOSH)\n\
             - Co-founder, product/frontend\n\n\
             ## Sarah (U_SARAH)\n\
             - Contractor, backend engineer",
        ),
        (
            "memory/projects.md",
            "# Projects\n\n\
             ## Billing Migration\n\
             - Channel: #billing-migration\n\
             - Status: Phase 2\n\n\
             ## API v2\n\
             - No dedicated channel yet\n\
             - Team: Sarah (lead), Alan (reviewer)\n\
             - Status: Starting next week",
        ),
    ],
    // Explicit request to create a channel for a new project
    trigger: "hey delegate, we're kicking off the API v2 rewrite next week. \
              can you set up a channel for it and get Sarah and Alan in there?",
    correct_answer: "",
    expected_tools: &["create_channel"],
};

pub(crate) const SCENARIO_INVITE_MISSING_PERSON: Scenario = Scenario {
    name: "invite_missing_person",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern (U_ALAN)\n\
             - Co-founder, backend/infra\n\n\
             ## Josh (U_JOSH)\n\
             - Co-founder, product/frontend\n\n\
             ## Sarah (U_SARAH)\n\
             - Contractor, backend\n\
             - Working on API v2 auth\n\
             - Not yet in #billing-migration",
        ),
    ],
    // Agent should recognize Sarah needs to be in the channel
    trigger: "Sarah's going to help with the billing migration starting tomorrow. \
              can you add her to the channel?",
    correct_answer: "",
    expected_tools: &["invite_to_channel"],
};

pub(crate) const SCENARIO_STRATEGIC_GROUP_DM: Scenario = Scenario {
    name: "strategic_group_dm",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [people](memory/people.md) \u{2014} Team members and roles\n\
             - [billing-migration](memory/billing-migration.md) \u{2014} Billing project",
        ),
        (
            "memory/people.md",
            "# People\n\n\
             ## Alan Kern (U_ALAN)\n\
             - Co-founder, backend/infra\n\
             - Owns billing migration backend\n\n\
             ## Josh (U_JOSH)\n\
             - Co-founder, product/frontend\n\
             - Owns billing dashboard UI\n\n\
             ## Sarah (U_SARAH)\n\
             - Contractor, backend\n\
             - Has Stripe integration expertise from previous job",
        ),
        (
            "memory/billing-migration.md",
            "# Billing Migration\n\n\
             - **Blocker**: Stripe webhook signature verification failing in staging\n\
             - Alan has been debugging for 2 days\n\
             - Sarah mentioned she dealt with this exact issue at her last company",
        ),
    ],
    // Agent should recognize this is a good moment to pull the right people together
    trigger: "this Stripe webhook issue is killing us. we've been stuck for 2 days. \
              can you pull together whoever can help sort this out?",
    correct_answer: "",
    expected_tools: &["group_dm"],
};

// ── Self-extending tools ────────────────────────────────────────────────

pub(crate) const SCENARIO_LOAD_SKILL_PROGRESSIVE: Scenario = Scenario {
    name: "load_skill_progressive_disclosure",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
        (
            "skills/ticket-tracker/SKILL.md",
            "---\nname: ticket-tracker\n\
             description: Track tickets in-channel and persist as JSON\n\
             ---\n\n\
             # Ticket Tracker\n\n\
             When someone reports a bug or requests a feature, create a ticket entry.\n\n\
             ## Format\n\
             Use `write_file` to write to `tickets.json`. Each ticket has:\n\
             - id: T-NNN (auto-increment)\n\
             - title: one-line summary\n\
             - status: open | in-progress | closed\n\
             - reporter: who reported it\n\
             - created: date\n\n\
             ## Workflow\n\
             1. `read_file` tickets.json to get current state\n\
             2. Add the new ticket\n\
             3. `write_file` the updated JSON\n\
             4. `reply` confirming the ticket was created",
        ),
    ],
    // Agent sees "ticket-tracker" in skills list but needs load_skill to get instructions
    trigger: "hey delegate, someone reported a bug — the billing page crashes when \
              you click 'Export'. Can you track that as a ticket?",
    correct_answer: "",
    expected_tools: &["load_skill"],
};

pub(crate) const SCENARIO_HTTP_REQUEST_API: Scenario = Scenario {
    name: "http_request_external_api",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        (
            "MEMORY.md",
            "# Memory\n\n\
             - [projects](memory/projects.md) \u{2014} Active projects and repos",
        ),
        (
            "memory/projects.md",
            "# Projects\n\n\
             ## Delegate Bot\n\
             - Repo: acme-corp/delegate-bot (GitHub)\n\
             - Status: Active development\n\n\
             ## Billing Service\n\
             - Repo: acme-corp/billing-service (GitHub)\n\
             - Status: Phase 2",
        ),
    ],
    // Agent should use http_request to call the GitHub API
    trigger: "can you check how many open PRs we have on the delegate-bot repo? \
              Use the GitHub API — https://api.github.com/repos/acme-corp/delegate-bot/pulls?state=open",
    correct_answer: "",
    expected_tools: &["http_request"],
};

pub(crate) const SCENARIO_RUN_SCRIPT_COMPUTE: Scenario = Scenario {
    name: "run_script_computation",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
        (
            "data/sprint-hours.csv",
            "name,hours_estimated,hours_actual\n\
             billing-api,40,52\n\
             dashboard-ui,30,28\n\
             auth-rewrite,20,35\n\
             webhook-handler,15,12\n\
             data-migration,25,40",
        ),
    ],
    // Agent should use run_script to compute something from the data
    trigger: "can you calculate the total estimated vs actual hours from \
              data/sprint-hours.csv? I need the totals and the overall variance percentage.",
    correct_answer: "",
    expected_tools: &["run_script"],
};

pub(crate) const SCENARIO_SKILL_DEFINED_TOOL: Scenario = Scenario {
    name: "skill_defined_tool_dispatch",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
        (
            "skills/sprint-stats/SKILL.md",
            "---\nname: sprint-stats\ndescription: Calculate sprint statistics from CSV data\ntools_json: |\n  [{\n    \"name\": \"calculate_sprint_stats\",\n    \"description\": \"Calculate total estimated vs actual hours and variance from sprint CSV data\",\n    \"parameters\": {\n      \"type\": \"object\",\n      \"properties\": {\n        \"csv_path\": { \"type\": \"string\", \"description\": \"Path to sprint CSV file relative to workspace\" }\n      },\n      \"required\": [\"csv_path\"]\n    },\n    \"handler\": \"script\",\n    \"handler_file\": \"stats.py\"\n  }]\n---\n\n# Sprint Stats Skill\n\nUse `calculate_sprint_stats` to analyze sprint hour data from CSV files.",
        ),
        (
            "skills/sprint-stats/stats.py",
            "import json, sys, csv\nargs = json.loads(sys.stdin.read())\npath = args.get('csv_path', '')\ntotal_est = 0\ntotal_act = 0\nwith open(path) as f:\n    for row in csv.DictReader(f):\n        total_est += int(row['hours_estimated'])\n        total_act += int(row['hours_actual'])\nvariance = ((total_act - total_est) / total_est) * 100\nprint(json.dumps({'estimated': total_est, 'actual': total_act, 'variance_pct': round(variance, 1)}))",
        ),
        (
            "data/sprint-hours.csv",
            "name,hours_estimated,hours_actual\nbilling-api,40,52\ndashboard-ui,30,28\nauth-rewrite,20,35\nwebhook-handler,15,12\ndata-migration,25,40",
        ),
    ],
    // The skill defines calculate_sprint_stats as a custom tool — agent should use it
    // directly rather than loading the skill and writing its own script
    trigger: "hey delegate, use the calculate_sprint_stats tool on data/sprint-hours.csv \
              and tell me the results.",
    correct_answer: "",
    expected_tools: &["calculate_sprint_stats"],
};

pub(crate) const SCENARIO_CREATE_SKILL_SELF_EXTEND: Scenario = Scenario {
    name: "create_skill_self_extending",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
    ],
    // Agent should create a skill when asked to build a new capability
    trigger: "delegate, build yourself a capability to check our uptime. \
              Create a skill that tracks service health — whenever someone asks \
              about uptime, you should reply with the current status. \
              For now just create the skill definition, don't worry about \
              actually pinging services yet.",
    correct_answer: "",
    expected_tools: &["create_skill"],
};

pub(crate) const SCENARIO_SKILL_NOT_FOUND_HONEST: Scenario = Scenario {
    name: "skill_not_found_honest",
    workspace_files: &[
        ("IDENTITY.md", IDENTITY_MD),
        ("INTENTS.md", INTENTS_MD),
        ("MEMORY.md", "# Memory\n\nEmpty for now."),
    ],
    // No skills loaded — agent should say it can't do this
    trigger: "can you run the deploy pipeline for staging?\n\
              A) Agent runs the deploy\n\
              B) Agent tries to create a deploy skill\n\
              C) Agent says it can't do that yet\n\
              D) Agent silently does nothing",
    correct_answer: "C",
    expected_tools: &[],
};

/// All eval scenarios in order.
pub(crate) fn all_scenarios() -> Vec<&'static Scenario> {
    vec![
        // Original recall & basic scenarios
        &SCENARIO_RECALL_STANDUP,
        &SCENARIO_RECALL_DECISION,
        &SCENARIO_RECALL_PERSON,
        &SCENARIO_NO_MEMORY,
        &SCENARIO_SET_REMINDER,
        &SCENARIO_TEAM_NORMS,
        &SCENARIO_CROSS_FILE,
        &SCENARIO_CASUAL_BANTER,
        // P0: save & log (ALWAYS frequency)
        &SCENARIO_SAVE_PERSON_INFO,
        &SCENARIO_LOG_IMPLICIT_DECISION,
        &SCENARIO_LEARN_AND_ACKNOWLEDGE,
        // P1: correction & judgment
        &SCENARIO_SAVE_CORRECTION,
        &SCENARIO_REACT_ONLY,
        &SCENARIO_IGNORE_BOT_NOISE,
        // P2: synthesis & robustness
        &SCENARIO_SYNTHESIZE_FOR_SOMEONE,
        &SCENARIO_PARTIAL_INFO,
        &SCENARIO_MULTI_QUESTION,
        // P1: proactive outreach & mentions
        &SCENARIO_MENTION_RELEVANT_PERSON,
        &SCENARIO_SPONTANEOUS_OUTREACH,
        // Channel & group DM tools
        &SCENARIO_CREATE_CHANNEL_FOR_PROJECT,
        &SCENARIO_INVITE_MISSING_PERSON,
        &SCENARIO_STRATEGIC_GROUP_DM,
        // Self-extending tools
        &SCENARIO_LOAD_SKILL_PROGRESSIVE,
        &SCENARIO_HTTP_REQUEST_API,
        &SCENARIO_RUN_SCRIPT_COMPUTE,
        &SCENARIO_SKILL_DEFINED_TOOL,
        &SCENARIO_CREATE_SKILL_SELF_EXTEND,
        &SCENARIO_SKILL_NOT_FOUND_HONEST,
    ]
}
