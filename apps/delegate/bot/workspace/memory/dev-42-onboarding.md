## DEV-42: User Onboarding Redesign

**Status:** In Progress (High Priority Epic)
**Assigned to:** Alice Chen
**Created:** Feb 10, 2026 | Last updated: Mar 9, 2026

### The Problem
40% drop-off at profile setup (per product analytics). Multiple customers citing onboarding as activation blocker.

### Customer Signal (from Gong)
DEV-42 mentioned in **6 customer calls** in March:
- **Acme Corp** (Mar 3, Mar 4): Expansion deal — onboarding + mobile deep links
- **StartupXYZ** (Mar 5, Mar 6): Renewal call — onboarding ROI concern (neutral sentiment)
- **MidMarket Inc** (Mar 7, Mar 8): Expansion — team management + admin dashboard
- **Acme Corp follow-up** (Mar 4): "Activation" is the sticking point

### Design & Implementation
**Figma:** abc123XYZ (Diana Wu — updated Mar 7)
- Simplified welcome screen (per Bob's feedback)
- Ready for eng review

**Design Review:** Scheduled March 11, 2026

**Related Linear Issues:**
- ENG-103: WebSocket reconnection (In Review)
- ENG-106: Analytics event tracking (Backlog)
- ENG-110: Profile step skip option (Done)
- ENG-114: Team invite email template (In Progress)
- ENG-115: OAuth 2.0 PKCE migration (In Review, PR #189)

### Blockers
- **ENG-101** (auth migration) — dependency for implementation start; PR #189 depends on this
- **DEV-34** (deep link bug) — blocks mobile testing after implementation lands

### GitHub
PR #189 (auth migration) — implementation started, depends on ENG-101

### Next
- Design review outcomes (Mar 11)
- Unblock DEV-34
- Start implementation once auth migration lands