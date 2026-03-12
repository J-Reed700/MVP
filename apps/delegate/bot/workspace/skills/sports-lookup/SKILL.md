---
name: sports-lookup
description: Look up real-time sports scores and schedules using free sports APIs
---

# Sports Lookup Skill

Use `http_request` to fetch real-time sports scores and schedules from free sports APIs.

## When to use

- When someone asks for sports scores ("did the Lakers win?", "what's the Warriors score?", "golf games today")
- When someone asks about game schedules or upcoming matches
- When someone wants standings or team stats

## How to use

### NBA Scores (balldontlie API - free, no auth)

```python
# Get today's games
http_request(method="GET", url="https://www.balldontlie.io/api/v1/games?start_date=YYYY-MM-DD&end_date=YYYY-MM-DD")

# Example: games for March 8, 2026
http_request(method="GET", url="https://www.balldontlie.io/api/v1/games?start_date=2026-03-08&end_date=2026-03-08")
```

Response includes: home/team scores, status (final/in progress), game time.

### Golf (PGA Tour - unofficial)

```python
# PGA Tour leaderboard (current tournament)
http_request(method="GET", url="https://statdata.pgatour.com/r/current/message.json")
```

Or search for current tournament ID and fetch leaderboard.

### General approach

1. Parse the sport from the query (basketball, golf, baseball, football, etc.)
2. Use the appropriate API
3. Format response as: team names + scores + status

## Example queries and responses

- "did the Lakers win?" → fetch NBA games for today, find Lakers, report score
- "golf games today" → fetch PGA leaderboard, show top 5 + tournament name
- "what's the Warriors score?" → fetch NBA games, find Warriors game, report current/final score

## Limitations

- These are free APIs — may have rate limits or occasional downtime
- Not all sports have good free APIs; if stuck, say "I can't find a reliable source for that sport right now"
- For obscure sports or minor leagues, may need to fall back to "I don't have good data for that"

## What NOT to do

- Don't fabricate scores or make up game results
- Don't promise real-time data for sports without an available API
- If the API fails or returns nothing, say so honestly

