# Delegate Bot

An AI-powered Product Manager assistant that lives in Slack. Delegate connects to the tools your team already uses — Jira, Linear, GitHub, Figma, Notion, Confluence, Gong, Google Calendar, and Gmail — and helps you manage work, synthesize context, and take action through natural conversation.

## What it does

- **Triage and route conversations** — understands intent from Slack messages and takes action across integrations
- **Multi-tool orchestration** — chains tool calls across services (e.g., finds a Jira ticket, checks the linked GitHub PR status, and summarizes the Figma design feedback)
- **Streaming responses** — LLM responses stream progressively into Slack for fast feedback
- **Approval workflows** — high-impact actions (creating tickets, posting comments) can require human approval before executing
- **Persistent state** — token budgets, event deduplication, pending approvals, reminders, and activity logs stored in PostgreSQL
- **Dynamic skills** — integrations are defined as JSON-based skill files, making it easy to add new tools without changing Rust code

## Architecture

```
apps/delegate/bot/          Rust binary (Axum + Tokio)
├── src/                    Core bot logic
│   ├── main.rs             Entry point, Slack socket mode event loop
│   ├── tool_loop.rs        Multi-turn LLM ↔ tool execution loop
│   ├── models.rs           LLM client (Anthropic, OpenAI, compatible APIs)
│   ├── streaming.rs        SSE parsing + progressive Slack message updates
│   ├── db.rs               PostgreSQL persistence (sqlx)
│   ├── oauth.rs            OAuth2 flows + credential storage for 8 providers
│   ├── dynamic_registry.rs Loads skill files into tool definitions at runtime
│   ├── triage.rs           Intent classification and routing
│   └── ...
├── workspace/
│   ├── skills/             Integration skill definitions (one folder per integration)
│   ├── credentials/        OAuth tokens (gitignored)
│   └── config.toml         Bot personality and behavior settings
├── Cargo.toml
docker-compose.yml          Postgres + 8 WireMock integration mocks
testdata/                   WireMock stubs and synthetic data per integration
scripts/                    Data generation and loading scripts
.env.template               All environment variables documented
```

## Integrations

| Integration | Tools | Auth | Mock Port |
|---|---|---|---|
| Jira | 11 (search, CRUD, transitions, sprints, linking) | Atlassian OAuth2 | :18081 |
| Gong | 6 (calls, transcripts, search, users, deals) | OAuth2 / API key | :18082 |
| Linear | 9 (search, CRUD, projects, cycles, members) | OAuth2 | :18083 |
| Notion | 8 (search, pages, databases, blocks) | OAuth2 | :18084 |
| Confluence | 7 (search, pages, spaces, comments) | Atlassian OAuth2 | :18085 |
| GitHub | 10 (issues, PRs, reviews, actions, search) | OAuth2 / PAT | :18086 |
| Figma | 6 (files, comments, versions, projects) | OAuth2 / access token | :18087 |
| Google Calendar | 4 (events, calendars, free/busy) | Google OAuth2 | :18088 (shared) |
| Gmail | 7 (messages, threads, drafts, labels) | Google OAuth2 | :18088 (shared) |

Plus built-in tools: set reminders, read/write files, manage intents, post messages, and more.

## Prerequisites

- **Rust** (stable, 2021 edition)
- **Docker** and **Docker Compose**
- **Slack app** with Socket Mode enabled
- **LLM API key** (Anthropic or OpenAI)

## Quick start

### 1. Set up environment

```bash
cp .env.template .env
```

Edit `.env` and fill in the required values:

```bash
# Required
DELEGATE_DATABASE_URL=postgres://postgres:postgres@localhost:5432/delegate
SLACK_APP_TOKEN=xapp-...        # Slack app-level token (Socket Mode)
SLACK_BOT_TOKEN=xoxb-...        # Slack bot token
SLACK_BOT_USER_ID=U...          # Bot's Slack user ID
ANTHROPIC_API_KEY=sk-ant-...    # Or OPENAI_API_KEY for OpenAI

# Optional — defaults shown
DELEGATE_TRANSPORT=slack        # "slack" or "cli" for local testing
DELEGATE_PROVIDER=anthropic     # "anthropic" or "openai"
# DELEGATE_MODEL=claude-sonnet-4-6
```

### 2. Start Postgres

```bash
docker compose up -d postgres
```

The bot auto-runs migrations on startup — no manual migration step needed.

### 3. Build and run the bot

```bash
cd apps/delegate/bot
cargo run
```

The bot will connect to Slack via Socket Mode and start listening for messages. Mention the bot in any channel it's been added to, or DM it directly.

### 4. Connect integrations (optional)

Add OAuth credentials to `.env` for any integrations you want to use:

```bash
# Atlassian (Jira + Confluence)
ATLASSIAN_CLIENT_ID=...
ATLASSIAN_CLIENT_SECRET=...

# Linear
LINEAR_CLIENT_ID=...
LINEAR_CLIENT_SECRET=...

# GitHub
GITHUB_CLIENT_ID=...
GITHUB_CLIENT_SECRET=...
# Or use a personal access token:
GITHUB_TOKEN=ghp_...

# Figma
FIGMA_CLIENT_ID=...
FIGMA_CLIENT_SECRET=...
# Or use an access token:
FIGMA_ACCESS_TOKEN=...

# Gong
GONG_CLIENT_ID=...
GONG_CLIENT_SECRET=...
# Or use API keys:
GONG_ACCESS_KEY=...
GONG_ACCESS_KEY_SECRET=...

# Notion
NOTION_CLIENT_ID=...
NOTION_CLIENT_SECRET=...

# Google (Calendar + Gmail)
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
```

For OAuth providers, the bot serves a callback endpoint on `OAUTH_PORT` (default 3456). Users can connect integrations by messaging the bot: "connect jira", "connect github", etc.

## Local testing with mocks

For development without real API credentials, use the WireMock mock services:

### 1. Generate synthetic data

```bash
./scripts/generate_synthetic_data.sh
```

This generates interconnected test data across all 8 integrations, telling a cohesive story about an Acme product team in Sprint 12 with four initiatives:
- Onboarding Redesign (DEV-42)
- API v2 Launch (DEV-50)
- Mobile Deep Link Bug (DEV-34)
- Dark Mode (DEV-55)

All data cross-references the same people, tickets, PRs, designs, and meetings.

### 2. Start Postgres + all mocks

```bash
docker compose --profile integration-mocks up -d
```

This starts Postgres and 8 WireMock containers (one per integration).

### 3. Point the bot at mocks

No OAuth credentials or API keys are needed for mock testing — WireMock ignores auth headers and responds to any matching request.

All base URLs default to real production APIs (e.g., `https://api.github.com`, `https://api.linear.app`). To override them for local mock testing, add these to your `.env`:

```bash
JIRA_BASE_URL=http://localhost:18081
GONG_BASE_URL=http://localhost:18082
LINEAR_BASE_URL=http://localhost:18083
NOTION_BASE_URL=http://localhost:18084
CONFLUENCE_BASE_URL=http://localhost:18085
GITHUB_BASE_URL=http://localhost:18086
FIGMA_BASE_URL=http://localhost:18087
GOOGLE_BASE_URL=http://localhost:18088
```

Remove or comment out these overrides when you want to use real integrations — the bot will automatically use production URLs and authenticate via OAuth or API tokens.

### 4. Run the bot

```bash
cd apps/delegate/bot
cargo run
```

With mock overrides set, API calls route to local WireMock containers. Without them, calls go to the real services.

## Slack app setup

Create a Slack app at [api.slack.com/apps](https://api.slack.com/apps) with:

1. **Socket Mode** enabled (generates the `SLACK_APP_TOKEN` starting with `xapp-`)
2. **Bot Token Scopes**: `app_mentions:read`, `chat:write`, `im:history`, `im:read`, `im:write`, `reactions:write`, `users:read`
3. **Event Subscriptions** (Socket Mode): `app_mention`, `message.im`
4. Install the app to your workspace and copy the `SLACK_BOT_TOKEN` (`xoxb-...`)
5. Find the bot's user ID (`SLACK_BOT_USER_ID`) from the app's "About" page or by calling `auth.test`

## Adding a new integration

1. Create a skill file at `workspace/skills/<name>/SKILL.md` with YAML frontmatter containing `tools_json` (see existing skills for format)
2. If the integration needs OAuth, add the provider config in `src/oauth.rs` → `load_provider_configs()`
3. Add `resolve_env_var` mappings in `src/oauth.rs` for auth headers and base URLs
4. Add env vars to `.env.template`
5. Optionally, add a WireMock mock service in `docker-compose.yml` and test stubs in `testdata/<name>/wiremock/`

## Project structure

```
MVP/
├── apps/delegate/bot/      The delegate bot (this is the entire product)
├── docker-compose.yml      Postgres + integration mocks
├── testdata/               WireMock mappings and response stubs
│   ├── jira/
│   ├── gong/
│   ├── linear/
│   ├── notion/
│   ├── confluence/
│   ├── github/
│   ├── figma/
│   └── google/             Serves both Calendar and Gmail
├── scripts/
│   └── generate_synthetic_data.sh
└── .env.template
```

## Data generated by seed script

| Integration | Volume | Notes |
|---|---|---|
| Jira | 1,000 issues | Story-canonical + variations |
| Gong | 5,000 call events | With transcripts and sentiment |
| Linear | 200 issues | GraphQL `issueSearch` format |
| Notion | 50 pages | PRDs, meeting notes, specs |
| Confluence | 50 pages | Runbooks, architecture docs |
| GitHub | 100 issues, 50 PRs, 30 runs | Cross-referenced to Jira/Linear |
| Figma | 100 comments | Design review threads |
| Google Calendar | 30 events | Sprint ceremonies, 1:1s, reviews |
| Gmail | 50 messages | Digest emails linking all tools |
