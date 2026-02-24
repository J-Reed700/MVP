# Architecture Direction

## Product objective

SignalOps provides a customer-signal intelligence layer for CS and product operations teams.

Primary outcomes:

- detect renewal/adoption risk earlier
- identify ownership and operational gaps
- drive consistent next actions for CSM and management teams

## Industry focus

In-scope industry: B2B SaaS Customer Success + Product Ops.

## MVP boundary

Current MVP intentionally prioritizes only:

1. signal capture
2. signal timeline
3. actionable insight generation

## Integration baseline

- Jira Cloud pull sync (`/api/integrations/jira/sync`)
- Gong webhook ingest (`/api/integrations/gong/webhook`)

## System shape

1. Ingestion:
   - pull/push events from operational systems (Jira, Gong, then CRM/support)
2. Normalization:
   - map source data to canonical signal records (`title`, `summary`, `owner`, `status`, `source`, `tags`, timestamps)
3. Insight engine:
   - generate audience-specific insights for manager and CSM actions
4. Experience:
   - dashboard + signal timeline + integration controls

## Backend organization

Single-crate Rust backend with clear boundaries:

- `domain`: core models
- `application`: use-cases and insight logic
- `infrastructure`: persistence + external adapters
- `presentation`: HTTP routes

## Persistence choices

- PostgreSQL as primary store
- SeaORM for data mapping and query ergonomics

## Upgrade path

Split into workspace crates only when ownership/reuse pressure justifies it.
