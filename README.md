# SignalOps MVP

SignalOps is a Customer Success + Product Ops intelligence MVP.

It ingests account and product signals (Jira + Gong in this phase), normalizes them, and surfaces action-oriented insights for:

- CSMs
- CS managers
- CS ops/product ops leaders

## MVP scope

Current MVP is limited to 3 core workflows:

1. Capture customer signal records
2. View canonical signal timeline
3. Surface actionable CS insights

## Stack

- Backend: Rust (`axum`) + SeaORM + PostgreSQL
- Frontend: React + Vite
- Integrations in scope: Jira sync + Gong pull sync + Gong webhook ingest

## Run backend

Start Postgres:

```bash
cd /Users/joshreed/Code/MVP
docker compose up -d postgres
```

Set environment variables:

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/signalops
export JIRA_BASE_URL=https://your-domain.atlassian.net
export JIRA_USER_EMAIL=you@company.com
export JIRA_API_TOKEN=your_api_token
export SIGNALOPS_WEBHOOK_INGEST_KEY=replace_with_strong_secret
export GONG_EVENTS_URL=https://your-gong-export-endpoint/events   # enables Gong pull sync
export GONG_API_KEY=your_gong_api_key                             # optional bearer token
export SIGNALOPS_LLM_PROVIDER=openai       # optional: openai or ollama
export SIGNALOPS_LLM_MODEL=gpt-4.1-mini    # optional
export OPENAI_API_KEY=your_openai_api_key  # required when provider=openai

# Optional if using provider=ollama
export OLLAMA_BASE_URL=http://localhost:11434
# Optional endpoint protection for Ollama/reverse proxy
export SIGNALOPS_LLM_BASIC_AUTH_USER=svc_user
export SIGNALOPS_LLM_BASIC_AUTH_PASS=svc_pass
export SIGNALOPS_LLM_USER_HEADER_NAME=X-Auth-User
export SIGNALOPS_LLM_USER_HEADER_VALUE=svc_user
export SIGNALOPS_LLM_PASS_HEADER_NAME=X-Auth-Pass
export SIGNALOPS_LLM_PASS_HEADER_VALUE=svc_pass
```

Run API:

```bash
cd /Users/joshreed/Code/MVP/backend
cargo run
```

## Run frontend

```bash
cd /Users/joshreed/Code/MVP/frontend
npm install
npm run dev
```

## API endpoints

Preferred routes:

- `GET /health`
- `GET /api/signals`
- `POST /api/signals`
- `GET /api/insights`
- `GET /api/settings/llm`
- `POST /api/settings/llm`
- `POST /api/settings/llm/models`
- `POST /api/settings/llm/warmup`
- `POST /api/dev/story/reset` (dev helper to load curated narrative data)
- `POST /api/integrations/jira/sync`
- `POST /api/integrations/gong/sync`
- `POST /api/integrations/gong/webhook`

## Integration examples

Jira sync:

```bash
curl -X POST http://localhost:8080/api/integrations/jira/sync \
  -H "Content-Type: application/json" \
  -d '{"jql":"project is not EMPTY ORDER BY updated DESC","limit":250}'
```

Gong ingest:

```bash
curl -X POST http://localhost:8080/api/integrations/gong/webhook \
  -H "Content-Type: application/json" \
  -H "x-signalops-ingest-key: replace_with_strong_secret" \
  -d '{"events":[{"event":"call_analyzed","callId":"call-1","accountId":"northstar_tech","accountName":"Northstar Tech","title":"Executive renewal risk checkpoint","riskFlags":["renewal_risk"],"transcriptExcerpt":"Customer said renewal is at risk unless activation improves this month."}]}'
```

Gong pull sync:

```bash
curl -X POST http://localhost:8080/api/integrations/gong/sync \
  -H "Content-Type: application/json" \
  -d '{"limit":500,"default_owner":"CS Operations"}'
```

Load curated story dataset:

```bash
curl -X POST http://localhost:8080/api/dev/story/reset
```

## Dockerized mocks + synthetic data

Generate synthetic fixtures:

```bash
cd /Users/joshreed/Code/MVP
./scripts/generate_synthetic_data.sh
```

Start Postgres + mock Jira/Gong:

```bash
cd /Users/joshreed/Code/MVP
docker compose --profile integration-mocks up -d
```

Local mock env:

```bash
export JIRA_BASE_URL=http://localhost:18081
export JIRA_USER_EMAIL=test@example.com
export JIRA_API_TOKEN=test-token
export SIGNALOPS_WEBHOOK_INGEST_KEY=test-ingest-key
```

Load synthetic data into backend:

```bash
cd /Users/joshreed/Code/MVP
BACKEND_URL=http://localhost:8080 SIGNALOPS_WEBHOOK_INGEST_KEY=test-ingest-key ./scripts/load_synthetic_data.sh
```

Replace noisy synthetic records with curated story-mode demo data:

```bash
cd /Users/joshreed/Code/MVP
BACKEND_URL=http://localhost:8080 ./scripts/load_story_data.sh
```

Windows PowerShell equivalents:

```powershell
Set-Location C:\Users\joshreed\Code\MVP
pwsh -File .\scripts\generate_synthetic_data.ps1
pwsh -File .\scripts\load_synthetic_data.ps1
pwsh -File .\scripts\load_story_data.ps1
```

## First installation checklist

1. Start dependencies:
   - Mac/Linux: `./dev up`
   - Windows: `docker compose --profile integration-mocks up -d`
2. Configure backend env (`DATABASE_URL`, Jira vars, webhook key, optional Gong pull vars).
3. Start backend:
   - `cd /Users/joshreed/Code/MVP/backend && cargo run`
4. Start frontend:
   - `cd /Users/joshreed/Code/MVP/frontend && npm install && npm run dev`
5. Load data:
   - Synthetic integration load: `./scripts/load_synthetic_data.sh` or PowerShell equivalent
   - Curated story mode: `./scripts/load_story_data.sh` or PowerShell equivalent

Default synthetic scale:

- Jira: 1000 issues
- Gong: 5000 events

The dataset includes transcript-backed Gong call events with account metadata (`industry`, `segment`, `region`, `arr`, `renewalWindow`), mixed recency, and story arcs (recovery, stalled, expansion) so CS insights (risk, ownership, concentration, account hotspots, ARR exposure, and NPS follow-up) are exercised.

## LLM analytics enrichment (optional)

If LLM provider env is configured, `/api/insights` runs a second-stage enrichment pass on top of deterministic rules:

- calibrates `priority`, `confidence`, and `due_in_days`
- adds `rationale` and evidence bullets grounded in related signals
- annotates `generated_by` as `llm+rules`
- supports `SIGNALOPS_LLM_PROVIDER=openai` or `SIGNALOPS_LLM_PROVIDER=ollama`
- supports optional Basic Auth + custom user/pass headers for protected endpoints

If the LLM call fails or is unavailable, the API automatically returns deterministic insights (`generated_by: "rules"`).
