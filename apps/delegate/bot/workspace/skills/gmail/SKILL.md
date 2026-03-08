---
name: gmail
description: Read and send emails via Gmail. Use for stakeholder communication, status updates, and external outreach.
required_credentials: google
tools_json: |
  [
    {
      "name": "gmail_list_messages",
      "description": "Search Gmail messages. Returns message IDs and thread IDs matching the query.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "Gmail search query (same syntax as Gmail search bar, e.g. 'from:client@example.com is:unread')"
          },
          "max_results": {
            "type": "integer",
            "description": "Max messages to return (default 10)"
          }
        },
        "required": ["query"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={{query}}&maxResults={{max_results}}",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gmail_read_message",
      "description": "Read a specific Gmail message by ID. Returns headers (from, to, subject, date) and body.",
      "parameters": {
        "type": "object",
        "properties": {
          "message_id": {
            "type": "string",
            "description": "Gmail message ID (from gmail_list_messages)"
          }
        },
        "required": ["message_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/messages/{{message_id}}?format=full",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gmail_send_message",
      "description": "Send an email via Gmail. The raw_message must be a base64url-encoded RFC 2822 email.",
      "parameters": {
        "type": "object",
        "properties": {
          "raw_message": {
            "type": "string",
            "description": "Base64url-encoded RFC 2822 email string (To, From, Subject headers + body)"
          }
        },
        "required": ["raw_message"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/messages/send",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"raw\":\"{{raw_message}}\"}"
    },
    {
      "name": "gmail_create_draft",
      "description": "Create a Gmail draft for human review before sending. The raw_message must be a base64url-encoded RFC 2822 email.",
      "parameters": {
        "type": "object",
        "properties": {
          "raw_message": {
            "type": "string",
            "description": "Base64url-encoded RFC 2822 email string"
          }
        },
        "required": ["raw_message"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/drafts",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"message\":{\"raw\":\"{{raw_message}}\"}}"
    },
    {
      "name": "gmail_list_labels",
      "description": "List Gmail labels (folders) for the account.",
      "parameters": {
        "type": "object",
        "properties": {}
      },
      "handler": "http",
      "method": "GET",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/labels",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gmail_get_thread",
      "description": "Get all messages in a Gmail thread (full email conversation).",
      "parameters": {
        "type": "object",
        "properties": {
          "thread_id": {
            "type": "string",
            "description": "Gmail thread ID (from gmail_list_messages or gmail_read_message)"
          }
        },
        "required": ["thread_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/threads/{{thread_id}}?format=full",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "gmail_reply",
      "description": "Reply to an existing email thread. The raw_message must include In-Reply-To and References headers, and the same Subject (prefixed with Re:).",
      "parameters": {
        "type": "object",
        "properties": {
          "thread_id": {
            "type": "string",
            "description": "Gmail thread ID to reply to"
          },
          "raw_message": {
            "type": "string",
            "description": "Base64url-encoded RFC 2822 reply (must include In-Reply-To, References, and matching Subject headers)"
          }
        },
        "required": ["thread_id", "raw_message"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://gmail.googleapis.com/gmail/v1/users/me/messages/send",
      "headers": {
        "Authorization": "Bearer {{env.GOOGLE_ACCESS_TOKEN}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"raw\":\"{{raw_message}}\",\"threadId\":\"{{thread_id}}\"}"
    }
  ]
---

# Gmail Integration

Read and send emails via Gmail for stakeholder communication.

## Setup

Requires: `GOOGLE_ACCESS_TOKEN` — OAuth2 access token with `gmail.modify` scope (or `gmail.readonly` + `gmail.send`).

Note: Access tokens expire. For production, use a refresh token flow.

## When to use

- Checking for stakeholder emails → `gmail_list_messages` + `gmail_read_message`
- Sending routine status updates → `gmail_send_message` (if approved for autonomous sending)
- Drafting non-routine communication for review → `gmail_create_draft`
- Preparing email digests from recent messages → `gmail_list_messages` with date query

## Gmail Search Query Examples

- Recent from a person: `from:client@example.com newer_than:7d`
- Unread: `is:unread`
- Subject search: `subject:"weekly update"`
- Has attachment: `has:attachment from:team@example.com`

## CRITICAL Guidelines

- **External emails ALWAYS require human approval** per the autonomy model
- Prefer `gmail_create_draft` over `gmail_send_message` for non-routine communication
- Only send directly when the content is a routine, pre-approved format (e.g. weekly status template)
- The `raw_message` format requires base64url encoding of a full RFC 2822 message — use `run_script` with Python to construct this if needed
- Always include proper To, From, Subject headers
- Never send to addresses not previously approved by the team
- To reply to a thread: first `gmail_get_thread` to get the Message-ID header, then construct a reply with `In-Reply-To` and `References` headers matching that Message-ID
- Use `gmail_get_thread` to read full email conversations before responding
