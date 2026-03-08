---
name: confluence
description: Search, read, and write Confluence pages. Use for documentation, knowledge base, and team wikis.
required_credentials: atlassian
tools_json: |
  [
    {
      "name": "confluence_search",
      "description": "Search Confluence content using CQL (Confluence Query Language).",
      "parameters": {
        "type": "object",
        "properties": {
          "cql": {
            "type": "string",
            "description": "CQL query (e.g. 'text ~ \"deployment process\" AND space = DEV')"
          },
          "limit": {
            "type": "integer",
            "description": "Max results (default 10)"
          }
        },
        "required": ["cql"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content/search?cql={{cql}}&limit={{limit}}&expand=version,space",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "confluence_get_page",
      "description": "Get a Confluence page by ID, including its body content.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Confluence page ID (numeric)"
          }
        },
        "required": ["page_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content/{{page_id}}?expand=body.storage,version,space",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "confluence_create_page",
      "description": "Create a new Confluence page in a space.",
      "parameters": {
        "type": "object",
        "properties": {
          "space_key": {
            "type": "string",
            "description": "Space key (e.g. DEV, TEAM)"
          },
          "title": {
            "type": "string",
            "description": "Page title"
          },
          "body_html": {
            "type": "string",
            "description": "Page body in HTML (Confluence storage format)"
          },
          "parent_id": {
            "type": "string",
            "description": "Parent page ID to nest under (optional)"
          }
        },
        "required": ["space_key", "title", "body_html"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"type\":\"page\",\"title\":\"{{title}}\",\"space\":{\"key\":\"{{space_key}}\"},\"body\":{\"storage\":{\"value\":\"{{body_html}}\",\"representation\":\"storage\"}}}"
    },
    {
      "name": "confluence_update_page",
      "description": "Update an existing Confluence page. Requires current version number.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Page ID to update"
          },
          "title": {
            "type": "string",
            "description": "Page title (required even if unchanged)"
          },
          "body_html": {
            "type": "string",
            "description": "New page body in HTML"
          },
          "version_number": {
            "type": "integer",
            "description": "Current version number + 1 (get from confluence_get_page)"
          }
        },
        "required": ["page_id", "title", "body_html", "version_number"]
      },
      "handler": "http",
      "method": "PUT",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content/{{page_id}}",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"type\":\"page\",\"title\":\"{{title}}\",\"body\":{\"storage\":{\"value\":\"{{body_html}}\",\"representation\":\"storage\"}},\"version\":{\"number\":{{version_number}}}}"
    },
    {
      "name": "confluence_list_spaces",
      "description": "List available Confluence spaces.",
      "parameters": {
        "type": "object",
        "properties": {
          "limit": {
            "type": "integer",
            "description": "Max results (default 25)"
          }
        }
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/space?limit={{limit}}&expand=description.plain",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "confluence_add_comment",
      "description": "Add a comment to a Confluence page.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Page ID to comment on"
          },
          "comment_html": {
            "type": "string",
            "description": "Comment body in HTML (e.g. '<p>Looks good, shipping next week.</p>')"
          }
        },
        "required": ["page_id", "comment_html"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"type\":\"comment\",\"container\":{\"id\":\"{{page_id}}\",\"type\":\"page\"},\"body\":{\"storage\":{\"value\":\"{{comment_html}}\",\"representation\":\"storage\"}}}"
    },
    {
      "name": "confluence_get_children",
      "description": "Get child pages of a Confluence page (for navigating page trees).",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Parent page ID"
          },
          "limit": {
            "type": "integer",
            "description": "Max results (default 25)"
          }
        },
        "required": ["page_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.CONFLUENCE_BASE_URL}}/wiki/rest/api/content/{{page_id}}/child/page?limit={{limit}}&expand=version",
      "headers": {
        "Authorization": "{{env.CONFLUENCE_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    }
  ]
---

# Confluence Integration

Use this skill to read and write documentation in the team's Confluence wiki.

## Setup

Requires:
- `CONFLUENCE_BASE_URL` — e.g. `https://yourteam.atlassian.net`
- `CONFLUENCE_AUTH` — Base64-encoded `email:api_token`

## When to use

- Writing meeting notes, PRDs, decision records → `confluence_create_page`
- Updating existing documentation → `confluence_get_page` then `confluence_update_page`
- Finding existing docs for context → `confluence_search`
- Checking what spaces exist → `confluence_list_spaces`

## CQL Examples

- Text search: `text ~ "deployment" AND space = DEV`
- Recent pages: `type = page AND lastModified >= now("-7d")`
- By title: `title = "Architecture Decision Records"`
- In a space: `space = TEAM AND type = page`

## Guidelines

- Always get the current version number before updating (version conflict otherwise)
- Body uses Confluence storage format (HTML) — use `<p>`, `<h2>`, `<ul><li>` etc.
- Nest pages under existing parents to maintain hierarchy
- Search before creating to avoid duplicates
- Use `confluence_get_children` to navigate page hierarchies before creating new subpages
- Comments use the same HTML storage format as page bodies
