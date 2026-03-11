---
name: google-calendar
description: Read Google Calendar events for schedule awareness, meeting prep, and agenda context. Read-only.
required_credentials: google
tools_json: |
  [
    {
      "name": "gcal_list_events",
      "description": "List upcoming Google Calendar events. Returns event title, time, attendees, and description.",
      "parameters": {
        "type": "object",
        "properties": {
          "calendar_id": {
            "type": "string",
            "description": "Calendar ID (default: 'primary')"
          },
          "time_min": {
            "type": "string",
            "description": "Start of time range in RFC3339 (e.g. '2026-03-06T00:00:00Z'). Defaults to now."
          },
          "time_max": {
            "type": "string",
            "description": "End of time range in RFC3339 (e.g. '2026-03-07T23:59:59Z'). Defaults to 7 days from now."
          },
          "max_results": {
            "type": "integer",
            "description": "Max events to return (default 10)"
          }
        },
        "required": ["calendar_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GOOGLE_BASE_URL}}/calendar/v3/calendars/{{calendar_id}}/events?timeMin={{time_min}}&timeMax={{time_max}}&maxResults={{max_results}}&singleEvents=true&orderBy=startTime",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gcal_get_event",
      "description": "Get full details of a specific calendar event.",
      "parameters": {
        "type": "object",
        "properties": {
          "calendar_id": {
            "type": "string",
            "description": "Calendar ID (default: 'primary')"
          },
          "event_id": {
            "type": "string",
            "description": "Event ID (from gcal_list_events)"
          }
        },
        "required": ["event_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GOOGLE_BASE_URL}}/calendar/v3/calendars/{{calendar_id}}/events/{{event_id}}",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gcal_list_calendars",
      "description": "List all calendars the user has access to.",
      "parameters": {
        "type": "object",
        "properties": {}
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GOOGLE_BASE_URL}}/calendar/v3/users/me/calendarList?maxResults=50",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gcal_freebusy",
      "description": "Check free/busy status across one or more calendars for a time range. Use to find availability for scheduling.",
      "parameters": {
        "type": "object",
        "properties": {
          "time_min": {
            "type": "string",
            "description": "Start of time range in RFC3339 (e.g. '2026-03-06T09:00:00Z')"
          },
          "time_max": {
            "type": "string",
            "description": "End of time range in RFC3339 (e.g. '2026-03-06T17:00:00Z')"
          },
          "calendar_ids_json": {
            "type": "string",
            "description": "JSON array of calendar IDs to check (e.g. '[{\"id\":\"primary\"},{\"id\":\"team@group.calendar.google.com\"}]')"
          }
        },
        "required": ["time_min", "time_max", "calendar_ids_json"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GOOGLE_BASE_URL}}/calendar/v3/freeBusy",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"timeMin\":\"{{time_min}}\",\"timeMax\":\"{{time_max}}\",\"items\":{{calendar_ids_json}}}"
    }
  ]
---

# Google Calendar Integration

Read-only access to Google Calendar for schedule awareness and meeting preparation.

## Setup

Requires: `GOOGLE_ACCESS_TOKEN` — OAuth2 access token with `calendar.readonly` scope.

Note: Access tokens expire. For production, use a refresh token flow via a script skill or external service.

## When to use

- Before standups: check what meetings are coming up today
- When someone asks "what's on the calendar" or "when's the next sync"
- Preparing meeting agendas: get event details, attendees, and description
- Detecting scheduling conflicts when planning work
- Daily/weekly digests: summarize the team's meeting load

## Guidelines

- Default calendar_id to "primary" if not specified
- Use RFC3339 timestamps (e.g. 2026-03-06T09:00:00Z)
- This is read-only — do not promise to create or modify events
- When summarizing meetings, include time, title, and attendee count
- Flag meetings without agendas/descriptions as needing prep
- Use `gcal_freebusy` to check availability before suggesting meeting times
- FreeBusy requires calendar IDs as JSON array: `[{"id":"primary"},{"id":"other@group.calendar.google.com"}]`
