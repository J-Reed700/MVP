# Decision Log

## Decision: Use REST for Billing API (not GraphQL)
- **Date:** February 21, 2026
- **Decided by:** Marcus, Sarah, with team input
- **Context:** New billing API endpoints needed for the unified schema. Team debated REST vs GraphQL.
- **Decision:** REST, consistent with existing API gateway.
- **Reasoning:** (1) All existing Acme APIs are REST — consistency reduces cognitive load. (2) GraphQL adds operational complexity (schema federation, query cost analysis) that we don't need for simple CRUD operations. (3) The API team has deep REST expertise and no GraphQL experience. (4) We can always add a GraphQL layer later if needed.
- **Alternatives considered:** GraphQL (rejected for reasons above), gRPC (rejected — overkill for internal API, no external consumer benefit).
- **Revisit if:** We build a public developer API with complex querying needs.

## Decision: Server-Side Proration (not Client-Side)
- **Date:** February 21, 2026
- **Decided by:** Alex (proposed), Marcus and Sarah (approved)
- **Context:** When a customer upgrades mid-billing-cycle, we need to calculate a prorated charge. Could be done client-side or server-side.
- **Decision:** Server-side proration.
- **Reasoning:** (1) Billing logic must be authoritative on the server — client calculations can't be trusted for financial operations. (2) The November incident taught us that billing logic spread across multiple systems is dangerous. (3) Server-side lets us handle edge cases (grandfathered plans, compounding discounts) in one place.
- **Alternatives considered:** Client-side (rejected — security and correctness concerns), hybrid (rejected — complexity without benefit).

## Decision: Blue-Green Migration Strategy (not Big-Bang)
- **Date:** December 2025
- **Decided by:** Sarah, approved by Nina
- **Context:** How to migrate production data from legacy schema to unified schema. Big-bang migration is simpler but riskier. Blue-green is more complex but allows rollback.
- **Decision:** Blue-green with 2-week parallel-run period.
- **Reasoning:** (1) November incident made everyone risk-averse about billing changes. (2) Blue-green lets us validate the new schema with real data before cutting over. (3) If something goes wrong, we can roll back without data loss. (4) The 2-week parallel period lets us catch edge cases that don't appear in synthetic test data.
- **Alternatives considered:** Big-bang migration with maintenance window (rejected — too risky given November). Incremental table-by-table migration (rejected — creates a long period where both schemas must be maintained).
- **Trade-off accepted:** Dual-write complexity during the parallel period. Sarah will need to write and maintain the sync logic.

## Decision: Append-Only Usage Records (not Time-Series DB)
- **Date:** January 2026
- **Decided by:** Sarah
- **Context:** Marcus suggested using TimescaleDB for usage records instead of append-only PostgreSQL tables.
- **Decision:** Append-only in PostgreSQL.
- **Reasoning:** (1) We're not doing real-time analytics on usage data — batch billing runs are fine. (2) Adding TimescaleDB is operational overhead (new database engine, new backup strategy, new monitoring). (3) Append-only enables exact replay for billing disputes — we can reconstruct any bill from the raw records. (4) If we need time-series analytics later, we can replicate to a TSDB without changing the source of truth.
- **Alternatives considered:** TimescaleDB (rejected for reasons above), ClickHouse (rejected — even more operational overhead).
- **Revisit if:** Usage volume exceeds 100M records/month or we need sub-second usage analytics.
