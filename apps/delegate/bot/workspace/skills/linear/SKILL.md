---
name: linear
description: Search, create, and update Linear issues. Use when the team discusses tickets, cycles, or project tracking in Linear.
required_credentials: linear
tools_json: |
  [
    {
      "name": "linear_search_issues",
      "description": "Search Linear issues by filter criteria. Returns id, title, state, assignee, priority.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "Search query text to match against issue titles and descriptions"
          },
          "first": {
            "type": "integer",
            "description": "Number of results (default 10)"
          }
        },
        "required": ["query"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { issueSearch(query: \\\"{{query}}\\\", first: {{first}}) { nodes { id identifier title state { name } assignee { name } priority priorityLabel updatedAt } } }\"}"
    },
    {
      "name": "linear_get_issue",
      "description": "Get full details of a Linear issue by identifier (e.g. ENG-123).",
      "parameters": {
        "type": "object",
        "properties": {
          "identifier": {
            "type": "string",
            "description": "Issue identifier (e.g. ENG-123)"
          }
        },
        "required": ["identifier"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { issueSearch(query: \\\"{{identifier}}\\\", first: 1) { nodes { id identifier title description state { name } assignee { name } priority priorityLabel labels { nodes { name } } cycle { name number } project { name } createdAt updatedAt comments { nodes { body user { name } createdAt } } } } }\"}"
    },
    {
      "name": "linear_create_issue",
      "description": "Create a new Linear issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "team_key": {
            "type": "string",
            "description": "Team key (e.g. ENG)"
          },
          "title": {
            "type": "string",
            "description": "Issue title"
          },
          "description": {
            "type": "string",
            "description": "Issue description (markdown)"
          },
          "priority": {
            "type": "integer",
            "description": "Priority: 0=none, 1=urgent, 2=high, 3=medium, 4=low"
          }
        },
        "required": ["team_key", "title"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"mutation { issueCreate(input: { teamId: \\\"{{team_key}}\\\", title: \\\"{{title}}\\\", description: \\\"{{description}}\\\", priority: {{priority}} }) { success issue { id identifier title url } } }\"}"
    },
    {
      "name": "linear_update_issue",
      "description": "Update a Linear issue's fields.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_id": {
            "type": "string",
            "description": "Linear issue UUID (get from search/get first)"
          },
          "input_json": {
            "type": "string",
            "description": "JSON fields to update (e.g. '{\"title\":\"New title\",\"priority\":2}')"
          }
        },
        "required": ["issue_id", "input_json"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"mutation { issueUpdate(id: \\\"{{issue_id}}\\\", input: {{input_json}}) { success issue { id identifier title state { name } } } }\"}"
    },
    {
      "name": "linear_add_comment",
      "description": "Add a comment to a Linear issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "issue_id": {
            "type": "string",
            "description": "Linear issue UUID"
          },
          "body": {
            "type": "string",
            "description": "Comment body (markdown)"
          }
        },
        "required": ["issue_id", "body"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"mutation { commentCreate(input: { issueId: \\\"{{issue_id}}\\\", body: \\\"{{body}}\\\" }) { success comment { id } } }\"}"
    },
    {
      "name": "linear_list_projects",
      "description": "List active Linear projects with their progress.",
      "parameters": {
        "type": "object",
        "properties": {
          "first": {
            "type": "integer",
            "description": "Number of projects to return (default 10)"
          }
        }
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { projects(first: {{first}}, filter: { state: { eq: \\\"started\\\" } }) { nodes { id name description progress state startDate targetDate lead { name } } } }\"}"
    },
    {
      "name": "linear_get_cycle",
      "description": "Get the active cycle for a team, including issues in it.",
      "parameters": {
        "type": "object",
        "properties": {
          "team_id": {
            "type": "string",
            "description": "Team UUID (get from linear_list_members)"
          }
        },
        "required": ["team_id"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { team(id: \\\"{{team_id}}\\\") { activeCycle { id name number startsAt endsAt progress issueCountHistory completedIssueCountHistory issues { nodes { id identifier title state { name } assignee { name } priority priorityLabel } } } } }\"}"
    },
    {
      "name": "linear_list_members",
      "description": "List team members with their IDs and names. Also returns team IDs.",
      "parameters": {
        "type": "object",
        "properties": {
          "first": {
            "type": "integer",
            "description": "Number of results (default 50)"
          }
        }
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { users(first: {{first}}) { nodes { id name email active } } teams { nodes { id name key } } }\"}"
    },
    {
      "name": "linear_list_states",
      "description": "List workflow states for a team (to find state IDs for transitions).",
      "parameters": {
        "type": "object",
        "properties": {
          "team_id": {
            "type": "string",
            "description": "Team UUID"
          }
        },
        "required": ["team_id"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "https://api.linear.app/graphql",
      "headers": {
        "Authorization": "{{env.LINEAR_API_KEY}}",
        "Content-Type": "application/json"
      },
      "body_template": "{\"query\":\"query { team(id: \\\"{{team_id}}\\\") { states { nodes { id name type position } } } }\"}"
    }
  ]
---

# Linear Integration

Use this skill to interact with the team's Linear workspace.

## Setup

Requires: `LINEAR_API_KEY` — Linear API key (Settings → API → Personal API keys)

## When to use

- Someone asks about ticket/issue status → `linear_search_issues` or `linear_get_issue`
- A decision creates work → `linear_create_issue`
- Status changes or priority shifts → `linear_update_issue`
- Project overview needed → `linear_list_projects`
- Discussion resolves something about an issue → `linear_add_comment`

## Important Notes

- Linear uses UUIDs internally — always search/get first to find the ID before updating
- `team_key` for creation is the team's UUID, not the short key — search for a team issue first to discover it
- Priority is numeric: 0=none, 1=urgent, 2=high, 3=medium, 4=low
- Descriptions support markdown

## Guidelines

- Search before creating to avoid duplicates
- Include Slack thread context in descriptions when creating from conversations
- Respect the team's existing labels and workflow states
- Use `linear_list_members` to resolve names to user UUIDs for assignment
- Use `linear_list_states` to find state IDs before transitioning issues
- Use `linear_get_cycle` to see what's in the active sprint/cycle
