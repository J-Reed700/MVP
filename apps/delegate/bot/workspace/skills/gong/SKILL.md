---
name: gong
description: Search and review Gong call recordings, transcripts, and AI-generated summaries. Use for customer insight, deal health, and meeting context.
required_credentials: gong
tools_json: |
  [
    {
      "name": "gong_list_calls",
      "description": "List recent Gong calls. Returns call ID, title, date, duration, and participants.",
      "parameters": {
        "type": "object",
        "properties": {
          "from_date": {
            "type": "string",
            "description": "Start date in ISO 8601 format (e.g. '2026-03-01T00:00:00Z')"
          },
          "to_date": {
            "type": "string",
            "description": "End date in ISO 8601 format"
          }
        },
        "required": []
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GONG_BASE_URL}}/v2/calls/extensive",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"filter\":{\"fromDateTime\":\"{{from_date}}\",\"toDateTime\":\"{{to_date}}\"},\"contentSelector\":{\"exposedFields\":{\"content\":{\"brief\":true,\"topics\":true,\"trackers\":true}}}}"
    },
    {
      "name": "gong_get_call",
      "description": "Get details for a specific Gong call by ID, including summary, topics, and action items.",
      "parameters": {
        "type": "object",
        "properties": {
          "call_id": {
            "type": "string",
            "description": "Gong call ID"
          }
        },
        "required": ["call_id"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GONG_BASE_URL}}/v2/calls/extensive",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"filter\":{\"callIds\":[\"{{call_id}}\"]},\"contentSelector\":{\"exposedFields\":{\"content\":{\"brief\":true,\"topics\":true,\"trackers\":true,\"pointsOfInterest\":true}}}}"
    },
    {
      "name": "gong_get_call_transcript",
      "description": "Get the transcript for a specific Gong call. Returns speaker-labeled text segments.",
      "parameters": {
        "type": "object",
        "properties": {
          "call_id": {
            "type": "string",
            "description": "Gong call ID"
          }
        },
        "required": ["call_id"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GONG_BASE_URL}}/v2/calls/transcript",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"filter\":{\"callIds\":[\"{{call_id}}\"]}}"
    },
    {
      "name": "gong_search_calls",
      "description": "Search Gong calls by keyword across titles, transcripts, and summaries.",
      "parameters": {
        "type": "object",
        "properties": {
          "keyword": {
            "type": "string",
            "description": "Search keyword (e.g. 'onboarding', 'renewal', company name)"
          }
        },
        "required": ["keyword"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GONG_BASE_URL}}/v2/calls/extensive",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"filter\":{\"textSearch\":\"{{keyword}}\"},\"contentSelector\":{\"exposedFields\":{\"content\":{\"brief\":true,\"topics\":true}}}}"
    },
    {
      "name": "gong_list_users",
      "description": "List Gong users in the workspace. Returns user IDs, names, and emails.",
      "parameters": {
        "type": "object",
        "properties": {},
        "required": []
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GONG_BASE_URL}}/v2/users",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gong_get_deal",
      "description": "Get Gong deal/opportunity details including engagement metrics and activity timeline.",
      "parameters": {
        "type": "object",
        "properties": {
          "deal_id": {
            "type": "string",
            "description": "Gong deal ID"
          }
        },
        "required": ["deal_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GONG_BASE_URL}}/v2/deals/{{deal_id}}",
      "headers": {
        "Authorization": "{{env.GONG_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    }
  ]
---

# Gong Integration

Use this skill to access customer call recordings, transcripts, and AI-generated insights from Gong.

## Setup

**Option A: API key** — Set `GONG_ACCESS_KEY` and `GONG_ACCESS_KEY_SECRET` (Settings → API → Create Access Key).

**Option B: Mock (local testing)** — Set `GONG_BASE_URL=http://localhost:18082`

## When to use

- Customer asks about a deal or account health → `gong_search_calls` by company name
- Need context before a customer meeting → `gong_list_calls` for recent calls with that account
- Preparing a stakeholder update with customer insights → `gong_search_calls` by topic
- Want to understand what was discussed → `gong_get_call` for summary, `gong_get_call_transcript` for full transcript
- Checking deal engagement → `gong_get_deal`

## Guidelines

- Use `gong_search_calls` to find relevant calls before requesting full transcripts
- Transcripts can be very long — summarize key points rather than quoting entire transcripts
- Gong call summaries (briefs) are AI-generated and usually sufficient for context
- Topics and trackers provide structured insight without reading full transcripts
- When relaying customer feedback to other tools (Jira, Notion), cite the Gong call ID
- Deal data can reveal engagement trends — useful for renewal and expansion planning
