---
name: figma
description: Browse Figma files, read and post design comments, and list team projects. Use when the team discusses designs, mockups, or design review.
required_credentials: figma
tools_json: |
  [
    {
      "name": "figma_get_file",
      "description": "Get metadata and page structure for a Figma file. Returns file name, pages, components, and last modified date.",
      "parameters": {
        "type": "object",
        "properties": {
          "file_key": {
            "type": "string",
            "description": "Figma file key (the part after /file/ in the URL, e.g. 'abc123XYZ')"
          }
        },
        "required": ["file_key"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/files/{{file_key}}?depth=1",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "figma_get_comments",
      "description": "Get all comments on a Figma file. Returns comment text, author, timestamp, and resolved status.",
      "parameters": {
        "type": "object",
        "properties": {
          "file_key": {
            "type": "string",
            "description": "Figma file key"
          }
        },
        "required": ["file_key"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/files/{{file_key}}/comments",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "figma_post_comment",
      "description": "Post a comment on a Figma file. Useful for design feedback or linking Slack discussions to designs.",
      "parameters": {
        "type": "object",
        "properties": {
          "file_key": {
            "type": "string",
            "description": "Figma file key"
          },
          "message": {
            "type": "string",
            "description": "Comment text"
          }
        },
        "required": ["file_key", "message"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/files/{{file_key}}/comments",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"message\":\"{{message}}\"}"
    },
    {
      "name": "figma_get_file_versions",
      "description": "Get version history for a Figma file. Shows who changed what and when.",
      "parameters": {
        "type": "object",
        "properties": {
          "file_key": {
            "type": "string",
            "description": "Figma file key"
          }
        },
        "required": ["file_key"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/files/{{file_key}}/versions",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "figma_get_team_projects",
      "description": "List all projects for a Figma team.",
      "parameters": {
        "type": "object",
        "properties": {
          "team_id": {
            "type": "string",
            "description": "Figma team ID"
          }
        },
        "required": ["team_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/teams/{{team_id}}/projects",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "figma_get_project_files",
      "description": "List all files in a Figma project.",
      "parameters": {
        "type": "object",
        "properties": {
          "project_id": {
            "type": "string",
            "description": "Figma project ID (get from figma_get_team_projects)"
          }
        },
        "required": ["project_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.FIGMA_BASE_URL}}/v1/projects/{{project_id}}/files",
      "headers": {
        "X-Figma-Token": "{{env.FIGMA_ACCESS_TOKEN}}",
        "Accept": "application/json"
      }
    }
  ]
---

# Figma Integration

Use this skill to browse design files, review comments, and track design progress in Figma.

## Setup

**Option A: OAuth (recommended)** — Connect via `/connect` in Slack.

**Option B: Personal access token** — Set `FIGMA_ACCESS_TOKEN` to a Figma personal access token.

**Option C: Mock (local testing)** — Set `FIGMA_BASE_URL=http://localhost:18087`

## When to use

- Someone shares a Figma link → `figma_get_file` to get context about the file
- Design review discussion → `figma_get_comments` to see existing feedback
- Relay feedback from Slack to Figma → `figma_post_comment`
- Track design changes → `figma_get_file_versions`
- Browse team's design files → `figma_get_team_projects` then `figma_get_project_files`

## Guidelines

- The file key is the string after `/file/` and before the file name in Figma URLs
- Comments in Figma are threaded — posting a comment adds it at the file level
- Use `figma_get_file` with `depth=1` to avoid pulling the entire node tree (can be huge)
- When relaying Slack feedback to Figma, include the Slack user's name and context
- Design files can be large — summarize file structure rather than dumping raw JSON
