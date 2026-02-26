# Active Intents

## 1. Billing Migration to Unified Schema

**Priority:** Critical
**Trajectory:** Trending behind — recoverable but tight
**Deadline:** March 15, 2026 (hard — blocks enterprise tier launch)

### What's happening
Migrating from the legacy per-product billing tables to a unified billing schema that supports metered usage, tiered pricing, and multi-currency. This unlocks the enterprise tier that Sales has been promising since Q4.

### Current state
- Data model PR (PLAT-347) approved Thursday, merged to main
- Marcus's API team is now the bottleneck — they need to build the new billing endpoints against the unified schema
- Staging environment needs the migration script run before API work can be integration-tested
- Sarah is the only person who fully understands the legacy schema mapping. This is a single-point-of-failure risk.
- Lisa's billing dashboard redesign is blocked until API contracts are finalized

### Watch signals
- **Marcus's API team velocity** — are they making daily progress? Alex is on this team but still ramping.
- **Sarah's availability** — if she's out, nobody can answer legacy schema questions. Knowledge transfer hasn't happened.
- **Enterprise customer pressure via Sales** — Jordan will ask about timeline. Each ask increases organizational pressure.
- **Staging deploy status** — until staging has the new schema, integration testing can't start.

### Key people
- Sarah Chen: legacy schema expertise, overall technical direction
- Marcus Washington: API implementation lead
- Alex Kim: assigned to API team, learning the codebase
- Lisa Park: billing dashboard design (downstream dependency)
- Jordan Rivera: stakeholder, will ask about timeline

---

## 2. Q2 Planning

**Priority:** Background
**Trajectory:** Stable — on track

### What's happening
Quarterly planning for Q2 2026. Stakeholders have submitted priorities. The team needs to produce a resource plan and commit to deliverables by end of March.

### Current state
- Stakeholder input collected — Nina, Jordan, and product leads have submitted requests
- Engineering capacity unknown until billing migration lands and we see how much it took out of the team
- Sarah has a rough prioritization draft but hasn't shared it
- No urgency — this is a "nudge don't push" item until billing migration pressure eases

### Watch signals
- **Billing migration completion** — Q2 plan depends on knowing how much capacity it consumed
- **Sarah's bandwidth** — she needs to drive the planning doc but is overloaded
- **New requests appearing** — track if stakeholders try to add scope mid-quarter

### Key people
- Sarah Chen: drives prioritization
- Nina Kowalski: approves final plan
- Jordan Rivera: will push for Sales-aligned priorities
