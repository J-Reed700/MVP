---
name: read-file
description: Read a file from your workspace. Use to check current state before making changes.
---

# read_file

Read a file from your workspace directory.

## Parameters

- `path` — relative path within workspace (e.g. `tickets.json`, `memory/people.md`)

## When to use

- Before writing a file, read it first so you don't lose existing data
- When someone asks about stored state (tickets, notes, memory)
- To check what you've previously saved

## Notes

- All paths are relative to the workspace root
- If the file doesn't exist, you'll get an error — that's fine, it means you need to create it
