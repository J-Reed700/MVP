---
name: write-file
description: Write a file to your workspace. Use to persist state — tickets, notes, memory. Changes are auto-committed and pushed to git.
---

# write_file

Write a file to your workspace directory. Changes are automatically committed and pushed to git.

## Parameters

- `path` — relative path within workspace (e.g. `tickets.json`, `memory/people.md`)
- `content` — the full file content to write

## When to use

- Persisting structured data (tickets, tasks, project state)
- Saving notes or memory for future reference
- Updating existing files with new information

## Important

- Always `read_file` first if the file might already exist — you're doing a full overwrite, not a patch
- All paths are relative to the workspace root
- Parent directories are created automatically
- Every write triggers a git add + commit + push
