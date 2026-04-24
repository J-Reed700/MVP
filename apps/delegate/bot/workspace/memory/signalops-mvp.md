# SignalOps MVP

## Location
**https://github.com/J-Reed700/MVP** — the entire repo IS SignalOps

Delegate was added later as `apps/delegate/` on the `delegate-spec` branch.

## What it is
Customer Success + Product Ops intelligence tool. Ingests account and product signals (Jira + Gong), normalizes them, surfaces action-oriented insights for CSMs, CS managers, and CS ops/product ops leaders.

## MVP Scope (Phase 1)
1. Capture customer signal records
2. View canonical signal timeline
3. Surface actionable CS insights

## Stack
- **Backend**: Rust (`axum`) + SeaORM + PostgreSQL
- **Frontend**: React + Vite
- **Integrations**: Jira sync + Gong pull sync + Gong webhook ingest
- **LLM enrichment**: Optional (OpenAI or Ollama) — enriches insights with priority calibration, rationale, evidence

## Key API endpoints
- `GET /api/signals` — list signals
- `POST /api/signals` — create signal
- `GET /api/insights` — get CS insights (rules + optional LLM enrichment)
- `POST /api/integrations/jira/sync` — sync Jira issues
- `POST /api/integrations/gong/sync` — pull Gong events
- `POST /api/integrations/gong/webhook` — ingest Gong webhook

## Relation to Delegate
- Same repo, different apps
- SignalOps = CS-facing (customer health, churn risk, expansion signals)
- Delegate = team-facing (internal decisions, blockers, shipping)
- Architectural overlap: both capture context → synthesize → surface what matters
- Could theoretically become one platform with configurable "employee" modes (PM Agent, CS Agent, etc.)