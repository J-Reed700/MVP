# Heartbeat Configuration

## Schedule
- **Standup post:** Daily 9:15 AM ET in #platform-standup
- **Log rotation:** Daily at midnight ET
- **Intent review:** Every 4 hours during business hours (9 AM–6 PM ET)
- **Digest for Nina:** Weekly, Friday 4 PM ET

## Watched Channels
- #platform-standup
- #platform-eng
- #platform-design
- #billing-migration (project channel)

## Watched Jira Filters
- Project = PLAT AND status changed after -1d
- Project = PLAT AND labels = billing-migration

## Triage Thresholds
- **act-now:** Messages mentioning blockers, outages, or from Jordan/Nina about active intents
- **queue:** Jira updates on watched tickets, questions in #platform-eng, design reviews
- **ignore:** Bot notifications, CI green builds, emoji reactions, channel join/leave

## Token Budgets
- Standup generation: 4000 tokens
- Question answering: 8000 tokens
- Triage classification: 500 tokens
- Intent update: 6000 tokens
