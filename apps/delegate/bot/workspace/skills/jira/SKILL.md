---
name: jira
description: Search, create, update, and transition Jira issues. Use when the team discusses tickets, blockers, or backlog work.
required_credentials: atlassian
tools_json: |
  [
    {
      "name": "jira_search",
      "description": "Search Jira issues using JQL. Returns key, summary, status, assignee, and priority.",
      "parameters": {
        "type": "object",
        "properties": {
          "jql": {
            "type": "string",
            "description": "JQL query string (e.g. 'project = DEV AND status = \"In Progress\"')"
          },
          "max_results": {
            "type": "integer",
            "description": "Max results to return (default 10)"
          }
        },
        "required": ["jql"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/search?jql={{jql}}&maxResults={{max_results}}&fields=summary,status,assignee,priority,updated",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "jira_get_issue",
      "description": "Get full details of a Jira issue by key (e.g. DEV-123).",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          }
        },
        "required": ["issue_key"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}?fields=summary,status,assignee,priority,description,comment,updated,created",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "jira_create_issue",
      "description": "Create a new Jira issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "project_key": {
            "type": "string",
            "description": "Project key (e.g. DEV)"
          },
          "summary": {
            "type": "string",
            "description": "Issue title"
          },
          "description": {
            "type": "string",
            "description": "Issue description (plain text)"
          },
          "issue_type": {
            "type": "string",
            "description": "Issue type: Task, Bug, Story, Epic (default: Task)"
          }
        },
        "required": ["project_key", "summary"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"fields\":{\"project\":{\"key\":\"{{project_key}}\"},\"summary\":\"{{summary}}\",\"description\":{\"type\":\"doc\",\"version\":1,\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"{{description}}\"}]}]},\"issuetype\":{\"name\":\"{{issue_type}}\"}}}"
    },
    {
      "name": "jira_update_issue",
      "description": "Update fields on an existing Jira issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          },
          "fields_json": {
            "type": "string",
            "description": "JSON object of fields to update (e.g. '{\"summary\":\"New title\",\"priority\":{\"name\":\"High\"}}')"
          }
        },
        "required": ["issue_key", "fields_json"]
      },
      "handler": "http",
      "method": "PUT",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"fields\":{{fields_json}}}"
    },
    {
      "name": "jira_transition_issue",
      "description": "Transition a Jira issue to a new status. First use jira_get_transitions to find the transition ID.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          },
          "transition_id": {
            "type": "string",
            "description": "Transition ID (get from jira_get_transitions)"
          }
        },
        "required": ["issue_key", "transition_id"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}/transitions",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"transition\":{\"id\":\"{{transition_id}}\"}}"
    },
    {
      "name": "jira_get_transitions",
      "description": "Get available transitions for a Jira issue (to find transition IDs for status changes).",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          }
        },
        "required": ["issue_key"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}/transitions",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "jira_add_comment",
      "description": "Add a comment to a Jira issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          },
          "comment": {
            "type": "string",
            "description": "Comment text"
          }
        },
        "required": ["issue_key", "comment"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}/comment",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"body\":{\"type\":\"doc\",\"version\":1,\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"{{comment}}\"}]}]}}"
    },
    {
      "name": "jira_assign_issue",
      "description": "Assign a Jira issue to a user by their Atlassian account ID.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_key": {
            "type": "string",
            "description": "Issue key (e.g. DEV-123)"
          },
          "account_id": {
            "type": "string",
            "description": "Atlassian account ID of the assignee (get from jira_get_issue or jira_search_users)"
          }
        },
        "required": ["issue_key", "account_id"]
      },
      "handler": "http",
      "method": "PUT",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issue/{{issue_key}}/assignee",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"accountId\":\"{{account_id}}\"}"
    },
    {
      "name": "jira_get_sprint",
      "description": "Get the active sprint for a Jira board, including all issues in it.",
      "parameters": {
        "type": "object",
        "properties": {
          "board_id": {
            "type": "string",
            "description": "Jira board ID (get from jira_get_boards)"
          }
        },
        "required": ["board_id"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/agile/1.0/board/{{board_id}}/sprint?state=active",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "jira_get_boards",
      "description": "List Jira boards to find board IDs for sprint queries.",
      "parameters": {
        "type": "object",
        "properties": {
          "project_key": {
            "type": "string",
            "description": "Filter boards by project key (optional)"
          }
        }
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/agile/1.0/board?projectKeyOrId={{project_key}}&maxResults=10",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Accept": "application/json"
      }
    },
    {
      "name": "jira_link_issues",
      "description": "Create a link between two Jira issues (e.g. blocks, is blocked by, relates to, is parent of).",
      "parameters": {
        "type": "object",
        "properties": {
          "link_type": {
            "type": "string",
            "description": "Link type name: 'Blocks', 'Cloners', 'Duplicate', 'Relates' (or use jira_get_link_types)"
          },
          "inward_issue": {
            "type": "string",
            "description": "Issue key for the inward side (e.g. DEV-123 'is blocked by')"
          },
          "outward_issue": {
            "type": "string",
            "description": "Issue key for the outward side (e.g. DEV-456 'blocks')"
          }
        },
        "required": ["link_type", "inward_issue", "outward_issue"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.JIRA_BASE_URL}}/rest/api/3/issueLink",
      "headers": {
        "Authorization": "{{env.JIRA_AUTHORIZATION}}",
        "Content-Type": "application/json",
        "Accept": "application/json"
      },
      "body_template": "{\"type\":{\"name\":\"{{link_type}}\"},\"inwardIssue\":{\"key\":\"{{inward_issue}}\"},\"outwardIssue\":{\"key\":\"{{outward_issue}}\"}}"
    }
  ]
---

# Jira Integration

Use this skill to interact with the team's Jira instance.

## Setup

**Option A: OAuth (recommended)** — Connect via `/connect` in Slack. One click for both Jira and Confluence.

**Option B: API token** — Set these environment variables:
- `JIRA_BASE_URL` — e.g. `https://yourteam.atlassian.net`
- `JIRA_AUTH` — Base64-encoded `email:api_token`

## When to use

- Someone asks about ticket status → `jira_search` or `jira_get_issue`
- A decision creates work → `jira_create_issue`
- A blocker is resolved or priority changes → `jira_update_issue` or `jira_transition_issue`
- Standup/digest needs ticket data → `jira_search` with JQL for recent activity
- Thread discussion resolves a question about a ticket → `jira_add_comment`

## JQL Examples

- Open issues in a project: `project = DEV AND status != Done ORDER BY updated DESC`
- Assigned to someone: `assignee = "user@email.com" AND status != Done`
- Stale in-progress: `status = "In Progress" AND updated < -3d`
- Recently created: `project = DEV AND created >= -7d`
- Blockers: `priority = Blocker AND status != Done`

## Guidelines

- Always search before creating to avoid duplicates
- When creating issues from conversations, include a link back to the Slack thread in the description
- Use the team's existing issue types and workflows — don't invent new ones
- When transitioning issues, get transitions first to find the right ID
- To assign issues, you need the user's Atlassian account ID — get it from `jira_get_issue` (assignee field) or `jira_search` results
- To query sprints, first find the board ID with `jira_get_boards`, then use `jira_get_sprint`
- Link types: Blocks (A blocks B), Relates (A relates to B), Duplicate (A duplicates B)
