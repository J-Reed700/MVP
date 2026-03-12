---
name: ticket-tracker-write-json
description: Track tickets in-channel and persist the canonical tickets state as a simple JSON file on disk (tickets.json).
---

# ticket-tracker-write-json

What this skill does

- Track tickets (mini‑Jira) in-channel and persist the canonical ticket state to a simple JSON file on disk.
- Supported commands (copyable):
  - create: Title | short description | @assignee | priority | due YYYY-MM-DD
  - update: T-002 | status=In progress | assignee=@alex | priority=high
  - comment: T-002 | This is a status update
  - list: status=To do | assignee=@josh | tag=backend
  - show: T-002
  - export: csv or export: json

Command handling

- Parse fields, validate basic types (dates, priorities, ticket IDs). Return a clear error if validation fails.
- Reply in-thread confirming the change, or list an error explaining what to fix.
- Use a quick react (thumbsup or eyes) where appropriate for ACKs.

In-memory model

- Maintain an in-process tickets map keyed by ticket ID (T-001, T-002...).
- On startup, attempt to load canonical state from disk. If the file is missing, start empty and write a new file on first commit.

Ticket schema (per-ticket example)

{
  "id": "T-001",
  "title": "Example",
  "description": "...",
  "assignee": "@alex",
  "status": "To do",
  "priority": "medium",
  "tags": ["backend"],
  "due": "2026-03-01",
  "history": [
    {"when":"2026-02-26T10:00:00Z","who":"@josh","action":"created","changes":{...}}
  ],
  "comments": [
    {"when":"2026-02-26T11:00:00Z","who":"@alex","text":"Working on it"}
  ]
}

Persistence (write to disk)

- Canonical file: default ./data/tickets.json. If you want a different path, provide it and I will validate write permission before using it.
- Write format: UTF-8 JSON. Top-level structure:
  {
    "next_id": 3,
    "tickets": { ... }
  }
- Atomic write: write to a temp file in the same directory (tickets.json.tmp) then rename to tickets.json.
- Concurrency: use a simple lock file (tickets.json.lock) around read-modify-write. If the lock exists, retry a few times with short backoff then fail with a clear error.
- Backup: before overwriting, save the previous file as tickets.json.bak and rotate up to 3 backups.
- On every create/update/comment/move, persist immediately (synchronous write) and confirm success in the reply.
- If the environment disallows disk writes, fall back to in-channel persistence and warn the team.

Exports and interoperability

- Support export: csv and export: json commands. Write exports to the same directory as tickets-export-YYYYMMDD.json/csv.
- Can post summaries to other channels on request (daily/weekly or when overdue tickets appear).

Reminders and stale-ticket nudges

- Optionally schedule daily/weekly reminders (configurable). Persist schedule config alongside tickets.json.

How the bot uses existing tools

- reply: confirmations, errors, and detailed outputs (show/list/update confirmations).
- react: lightweight ACKs when commands are accepted/processed.
- post: proactive summaries or overdue alerts to other channels.
- no_action: when received messages are unrelated to ticket commands.

What NOT to do

- Do not write outside the configured directory or to paths that require elevated permissions without explicit approval.
- Do not persist secrets or private credentials in tickets.json.
- Do not assume file system access exists — if write fails, notify and fall back to in-channel storage.
- Never create ticket IDs that collide with existing ones — always use the next_id counter.

Examples

- create: Fix login bug | Users can't log in with SSO | @alex | high | due 2026-03-02
  -> reply: Created T-001 — Fix login bug — assignee @alex — priority high — due 2026-03-02

- update: T-001 | status=In progress | assignee=@alex
  -> reply: Updated T-001: status="In progress", assignee="@alex" (changed by @josh)

- comment: T-001 | Quick update: PR opened
  -> reply: Comment added to T-001

Operational notes for humans (setup)

- Ensure the bot process has a writable directory. Recommended default: ./data/ (will be created if missing) with owner set to the bot process.
- If you want a different absolute path (e.g., /mnt/shared/delegate/tickets.json) tell the bot; it will validate permission before using it.

Options I will ask when you say “start MVP”

- Storage: keep tickets here in-channel OR persist to disk at a path you provide (default ./data/tickets.json).
- Workflow: simple (To do / In progress / Done) OR expanded (Backlog / Ready / In progress / Review / Done).
- Who can create/assign, priority levels, and whether you want scheduled reminders (daily at X).

If disk persistence is chosen I will:
- Create ./data/ if missing (or the provided dir), validate write permissions, create tickets.json, and create an example ticket to show the workflow.

If you prefer I can instead start with in-channel persistence and provide exports on demand.

---

End of skill: ticket-tracker-write-json

