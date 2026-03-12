---
name: notion
description: Search, read, and write Notion pages and databases. Use for documentation, PRDs, decision logs, and knowledge base.
required_credentials: notion
tools_json: |
  [
    {
      "name": "notion_search",
      "description": "Search across Notion pages and databases by title or content.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "Search query text"
          },
          "filter_type": {
            "type": "string",
            "description": "Filter by object type: 'page' or 'database' (optional)"
          }
        },
        "required": ["query"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/search",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"query\":\"{{query}}\",\"page_size\":10}"
    },
    {
      "name": "notion_get_page",
      "description": "Get a Notion page's properties by ID.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Notion page UUID (32 hex chars, with or without dashes)"
          }
        },
        "required": ["page_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/pages/{{page_id}}",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Notion-Version": "2022-06-28"
      }
    },
    {
      "name": "notion_get_page_content",
      "description": "Get the block content (body) of a Notion page.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Notion page UUID"
          }
        },
        "required": ["page_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/blocks/{{page_id}}/children?page_size=100",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Notion-Version": "2022-06-28"
      }
    },
    {
      "name": "notion_create_page",
      "description": "Create a new Notion page in a parent page or database.",
      "parameters": {
        "type": "object",
        "properties": {
          "parent_id": {
            "type": "string",
            "description": "Parent page or database UUID"
          },
          "parent_type": {
            "type": "string",
            "description": "'page_id' or 'database_id'"
          },
          "title": {
            "type": "string",
            "description": "Page title"
          },
          "content_markdown": {
            "type": "string",
            "description": "Page content as a single text block (plain text, will be added as a paragraph)"
          }
        },
        "required": ["parent_id", "parent_type", "title"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/pages",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"parent\":{\"{{parent_type}}\":\"{{parent_id}}\"},\"properties\":{\"title\":{\"title\":[{\"text\":{\"content\":\"{{title}}\"}}]}},\"children\":[{\"object\":\"block\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"{{content_markdown}}\"}}]}}]}"
    },
    {
      "name": "notion_query_database",
      "description": "Query a Notion database with optional filter and sort.",
      "parameters": {
        "type": "object",
        "properties": {
          "database_id": {
            "type": "string",
            "description": "Database UUID"
          },
          "filter_json": {
            "type": "string",
            "description": "Notion filter object as JSON string (optional)"
          },
          "page_size": {
            "type": "integer",
            "description": "Number of results to return (e.g. 10)"
          }
        },
        "required": ["database_id", "page_size"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/databases/{{database_id}}/query",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"page_size\":{{page_size}}}"
    },
    {
      "name": "notion_append_block",
      "description": "Append content to an existing Notion page.",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Page UUID to append to"
          },
          "text": {
            "type": "string",
            "description": "Text content to append as a new paragraph"
          }
        },
        "required": ["page_id", "text"]
      },
      "handler": "http",
      "method": "PATCH",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/blocks/{{page_id}}/children",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"children\":[{\"object\":\"block\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"{{text}}\"}}]}}]}"
    },
    {
      "name": "notion_create_database_entry",
      "description": "Add a new row/entry to a Notion database with property values.",
      "parameters": {
        "type": "object",
        "properties": {
          "database_id": {
            "type": "string",
            "description": "Database UUID"
          },
          "properties_json": {
            "type": "string",
            "description": "JSON object of property values in Notion format (e.g. '{\"Name\":{\"title\":[{\"text\":{\"content\":\"My Task\"}}]},\"Status\":{\"select\":{\"name\":\"In Progress\"}}}')"
          }
        },
        "required": ["database_id", "properties_json"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/pages",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"parent\":{\"database_id\":\"{{database_id}}\"},\"properties\":{{properties_json}}}"
    },
    {
      "name": "notion_update_page_properties",
      "description": "Update properties on an existing Notion page (status, dates, selects, etc.).",
      "parameters": {
        "type": "object",
        "properties": {
          "page_id": {
            "type": "string",
            "description": "Page UUID to update"
          },
          "properties_json": {
            "type": "string",
            "description": "JSON object of properties to update in Notion format (e.g. '{\"Status\":{\"select\":{\"name\":\"Done\"}},\"Due Date\":{\"date\":{\"start\":\"2026-03-15\"}}}')"
          }
        },
        "required": ["page_id", "properties_json"]
      },
      "handler": "http",
      "method": "PATCH",
      "url_template": "{{env.NOTION_BASE_URL}}/v1/pages/{{page_id}}",
      "headers": {
        "Authorization": "Bearer {{env.NOTION_API_KEY}}",
        "Content-Type": "application/json",
        "Notion-Version": "2022-06-28"
      },
      "body_template": "{\"properties\":{{properties_json}}}"
    }
  ]
---

# Notion Integration

Use this skill to read and write documentation in the team's Notion workspace.

## Setup

Requires: `NOTION_API_KEY` — Notion integration token (Settings → Integrations → Internal integration)

The integration must be connected to the pages/databases it needs to access (Share → Invite integration).

## When to use

- Writing or updating PRDs, decision logs, meeting notes → `notion_create_page` or `notion_append_block`
- Finding existing documentation → `notion_search`
- Reading page content for context → `notion_get_page_content`
- Querying structured data (e.g. project tracker database) → `notion_query_database`
- Keeping docs current after team decisions → `notion_append_block`

## Guidelines

- Search before creating to avoid duplicate pages
- Use the team's existing page hierarchy — don't create top-level pages without asking
- When creating PRDs or decision logs, use the parent page/database the team already has
- Keep page content concise and well-structured
- Include dates and attribution when appending updates
- To add database entries, first query the database to understand its property schema
- Property format varies by type: `title`, `rich_text`, `select`, `multi_select`, `date`, `number`, `checkbox`, `url`
- Example select: `{"Status": {"select": {"name": "In Progress"}}}`
- Example date: `{"Due Date": {"date": {"start": "2026-03-15"}}}`
