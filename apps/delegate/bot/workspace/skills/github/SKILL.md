---
name: github
description: Search, create, and manage GitHub issues, pull requests, and CI runs. Use when the team discusses code, PRs, releases, or CI/CD.
required_credentials: github
tools_json: |
  [
    {
      "name": "github_search_repos",
      "description": "Search GitHub repositories by keyword.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "Search query (e.g. 'org:myorg language:rust')"
          }
        },
        "required": ["query"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/search/repositories?q={{query}}&per_page=10",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_search_issues",
      "description": "Search GitHub issues and pull requests across repos. Returns title, state, assignee, labels.",
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "Search query (e.g. 'repo:org/repo is:open label:bug')"
          }
        },
        "required": ["query"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/search/issues?q={{query}}&per_page=20",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_get_issue",
      "description": "Get full details of a GitHub issue or PR by number.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner (org or user)"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "number": {
            "type": "integer",
            "description": "Issue or PR number"
          }
        },
        "required": ["owner", "repo", "number"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/issues/{{number}}",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_create_issue",
      "description": "Create a new GitHub issue.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "title": {
            "type": "string",
            "description": "Issue title"
          },
          "body": {
            "type": "string",
            "description": "Issue body (markdown)"
          },
          "assignee": {
            "type": "string",
            "description": "GitHub username to assign"
          }
        },
        "required": ["owner", "repo", "title"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/issues",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Content-Type": "application/json",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      },
      "body_template": "{\"title\":\"{{title}}\",\"body\":\"{{body}}\",\"assignee\":\"{{assignee}}\"}"
    },
    {
      "name": "github_update_issue",
      "description": "Update a GitHub issue (title, body, state, labels, assignees).",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "number": {
            "type": "integer",
            "description": "Issue number"
          },
          "fields_json": {
            "type": "string",
            "description": "JSON object of fields to update (e.g. '{\"state\":\"closed\",\"labels\":[\"done\"]}')"
          }
        },
        "required": ["owner", "repo", "number", "fields_json"]
      },
      "handler": "http",
      "method": "PATCH",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/issues/{{number}}",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Content-Type": "application/json",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      },
      "body_template": "{{fields_json}}"
    },
    {
      "name": "github_add_comment",
      "description": "Add a comment to a GitHub issue or PR.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "number": {
            "type": "integer",
            "description": "Issue or PR number"
          },
          "body": {
            "type": "string",
            "description": "Comment text (markdown)"
          }
        },
        "required": ["owner", "repo", "number", "body"]
      },
      "handler": "http",
      "method": "POST",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/issues/{{number}}/comments",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Content-Type": "application/json",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      },
      "body_template": "{\"body\":\"{{body}}\"}"
    },
    {
      "name": "github_list_prs",
      "description": "List pull requests for a repository. Filter by state (open, closed, all).",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "state": {
            "type": "string",
            "description": "Filter by state: open, closed, all (default: open)"
          }
        },
        "required": ["owner", "repo"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/pulls?state={{state}}&per_page=20",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_get_pr",
      "description": "Get full details of a pull request including diff stats, reviewers, and merge status.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "number": {
            "type": "integer",
            "description": "PR number"
          }
        },
        "required": ["owner", "repo", "number"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/pulls/{{number}}",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_list_pr_reviews",
      "description": "List reviews on a pull request to check approval status.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "number": {
            "type": "integer",
            "description": "PR number"
          }
        },
        "required": ["owner", "repo", "number"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/pulls/{{number}}/reviews",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    },
    {
      "name": "github_list_actions_runs",
      "description": "List recent GitHub Actions workflow runs for a repository. Check CI/CD status.",
      "parameters": {
        "type": "object",
        "properties": {
          "owner": {
            "type": "string",
            "description": "Repository owner"
          },
          "repo": {
            "type": "string",
            "description": "Repository name"
          },
          "status": {
            "type": "string",
            "description": "Filter by status: completed, in_progress, queued, failure, success (optional)"
          }
        },
        "required": ["owner", "repo"]
      },
      "handler": "http",
      "method": "GET",
      "url_template": "{{env.GITHUB_BASE_URL}}/repos/{{owner}}/{{repo}}/actions/runs?status={{status}}&per_page=10",
      "headers": {
        "Authorization": "Bearer {{env.GITHUB_TOKEN}}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
      }
    }
  ]
---

# GitHub Integration

Use this skill to interact with GitHub repositories, issues, pull requests, and CI/CD pipelines.

## Setup

**Option A: OAuth (recommended)** — Connect via `/connect` in Slack.

**Option B: Personal access token** — Set `GITHUB_TOKEN` to a GitHub PAT with `repo` and `read:org` scopes.

**Option C: Mock (local testing)** — Set `GITHUB_BASE_URL=http://localhost:18086`

## When to use

- Someone asks about PR status or code review → `github_list_prs`, `github_get_pr`, `github_list_pr_reviews`
- A decision creates engineering work → `github_create_issue`
- CI is failing or someone asks about builds → `github_list_actions_runs`
- Need to find issues across repos → `github_search_issues`
- Thread discussion resolves a question about an issue → `github_add_comment`
- Closing or updating issue state → `github_update_issue`

## Search query examples

- Open bugs in a repo: `repo:org/repo is:issue is:open label:bug`
- PRs awaiting review: `repo:org/repo is:pr is:open review:required`
- Issues assigned to someone: `repo:org/repo is:issue assignee:username`
- Recently created issues: `repo:org/repo is:issue created:>2026-03-01`

## Guidelines

- Always search before creating issues to avoid duplicates
- When creating issues from Slack conversations, include context and a link to the thread
- Use existing labels and milestones — don't invent new ones
- For PR status, check both the review state and CI status (actions runs)
- Close issues with `github_update_issue` by setting `{"state":"closed"}`
