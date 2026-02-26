# Billing Migration — Project History

## Overview
Migration from legacy per-product billing tables (`billing_accounts`, `customer_billing_profiles`, `product_charges`) to a unified billing schema supporting metered usage, tiered pricing, and multi-currency. Unlocks the enterprise tier that Sales has been promising since Q4 2025.

## Timeline
- **October 2025:** RFC drafted by Sarah. Approved after 2 weeks of discussion.
- **November 2025:** Work paused after the billing incident (see below). Team focused on stability.
- **December 2025:** RFC revised to include additional validation steps post-incident. Scope expanded.
- **January 2026:** Data modeling work began. Sarah estimated 6-8 weeks for the full migration.
- **February 2026:** Data model PR (PLAT-347) opened Feb 19, merged Feb 21. API work started Feb 22. Staging ready Feb 24.
- **March 15, 2026:** Deadline. Billing API endpoints deployed to production + migration script validated in staging. Production data migration follows in a maintenance window.

## The November Incident
On November 12, 2025, a billing calculation bug charged ~200 enterprise customers incorrect amounts for 3 days. Root cause: a race condition between the metering service and the billing calculation job. Priya identified the issue on November 13 but her early warning was dismissed because "the deploy looked clean." The bug wasn't confirmed until November 15 when customer reports came in.

**Impact:** $340K in credits issued. Trust damage with enterprise customers. All-hands postmortem. New requirement: staging validation for ANY billing change, no exceptions.

**Cultural impact:** The team is skittish about billing changes. Extra caution is warranted but shouldn't become paralysis. Sarah has internalized this most — she insists on staging validation even for minor billing changes.

## Technical Approach
- **Unified schema:** Single `subscriptions` table replaces `billing_accounts` + `customer_billing_profiles`. Related tables: `usage_records`, `billing_periods`, `discount_overrides`, `grandfathered_plans`.
- **Migration strategy:** Blue-green. New schema runs in parallel with old for 2 weeks. Dual-write during transition. Rollback script available.
- **Grandfathered plans:** 23 enterprise customers on legacy pricing with compounding annual discounts. Must be preserved exactly. Each variant has dedicated test fixtures.
- **API design:** REST endpoints for CRUD on subscriptions, usage reporting, and bill preview. Idempotency keys required. Proration handled server-side.

## Risk Log
| Risk | Severity | Status | Mitigation |
|------|----------|--------|------------|
| Sarah is single point of failure on legacy schema | High | Open | Knowledge transfer planned (not yet scheduled) |
| March 15 deadline is tight | High | Open | Defined "done" criteria to manage scope |
| Grandfathered plans add complexity | Medium | Active | Test fixtures for all 23 variants |
| Enterprise customer pressure from Sales | Medium | Active | Sarah hedging on commitment to Jordan |
| Alex is new and still ramping | Low | Active | Marcus mentoring, pair programming |
| Staging slow query on usage_records | Low | Resolved | Index added by Rachel (PLAT-354) |

## Key Tickets
- PLAT-347: Unified billing data model (Done)
- PLAT-348: Staging migration script (Done)
- PLAT-350: Billing API — Create subscription (In Progress, Marcus)
- PLAT-351: Billing API — Update subscription (In Progress, Alex)
- PLAT-352: Billing API — Cancel/refund (To Do, Marcus)
- PLAT-353: Billing API — Preview endpoint (To Do)
- PLAT-354: Slow query fix (Done, Rachel)
