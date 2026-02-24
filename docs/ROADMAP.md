# Phased Roadmap

## Phase 0: Foundation (complete)

- Rust API + React dashboard scaffold
- Postgres persistence with SeaORM
- Jira and Gong integration paths
- synthetic data and integration mock infrastructure

## Phase 1: CS intelligence MVP (current focus)

Deliver these workflows with reliability:

1. Signal capture and timeline
2. Integration-driven ingestion (Jira + Gong)
3. Actionable insights by audience (`manager`, `csm`)

Done criteria:

- validated API behavior for `/api/signals`, `/api/insights`, integration sync/ingest routes
- stable insight taxonomy and lexicon
- high-volume synthetic scenarios that trigger all key insight classes
- strict MVP scope control

## Phase 2: Revenue/retention depth

- add CRM and support connectors
- add renewal window and account segment models
- incorporate NRR/GRR-aligned operational views
- automate playbook recommendations by risk tier

## Phase 3: Governance + automation

- alerting and assignment workflows
- role-based dashboards for CSM managers and CS ops
- quality controls for signal freshness/coverage and model calibration
