---
name: post
description: Posts a new message to any channel you're in. Use for cross-pollinating information — when something said in one place is relevant to another.
---

# post

Post a new message to a different channel.

## Parameters

- `channel` — the channel ID to post to
- `text` — the message content (Slack markdown supported)

## When to use

- Something said in one channel is directly relevant to a conversation in another
- A blocker surfaced in one thread that affects a different project
- Information needs to reach people who aren't in the current channel

## When NOT to use

- When the information is only relevant to the current conversation (use `reply` instead)
- When you're unsure if it's actually relevant — don't spam channels
- When you're guessing about what a channel cares about

## Guidelines

- Always provide enough context that the message stands alone — the reader doesn't have access to the source thread
- Keep it concise. Link back to the source if there's a deeper conversation happening
- Don't cross-post noise. Only signal.
