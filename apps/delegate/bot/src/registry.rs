use serde_json::Value;

// ── Action tiers ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTier {
    Autonomous,
    AutonomousWithNotice,
    RequiresApproval,
}

// ── Tool scope & frequency ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolScope {
    Event,
    Heartbeat,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Always,
    WhenRelevant,
    Sparingly,
}

impl Frequency {
    fn label(self) -> &'static str {
        match self {
            Frequency::Always => "ALWAYS",
            Frequency::WhenRelevant => "WHEN RELEVANT",
            Frequency::Sparingly => "SPARINGLY",
        }
    }
}

// ── Registry entry ─────────────────────────────────────────────────────

#[allow(dead_code)]
struct ToolEntry {
    name: &'static str,
    description: &'static str,
    scope: ToolScope,
    tier: ActionTier,
    frequency: Frequency,
    when: &'static str,
    is_information: bool,
    is_reply: bool,
    schema_fn: fn() -> Value,
}

// ── The registry ───────────────────────────────────────────────────────

static REGISTRY: &[ToolEntry] = &[
    ToolEntry {
        name: "react",
        description: "Add an emoji reaction to the triggering message.",
        scope: ToolScope::Event,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "React to acknowledge, show agreement, or signal you're thinking. Choose varied emojis — don't always use the same one.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_react,
    },
    ToolEntry {
        name: "reply",
        description: "Reply to the triggering message in a thread.",
        scope: ToolScope::Event,
        tier: ActionTier::AutonomousWithNotice,
        frequency: Frequency::WhenRelevant,
        when: "Reply when a substantive response is needed — answering a question, flagging a risk, providing context. Don't reply just to echo what was said.",
        is_information: false,
        is_reply: true,
        schema_fn: schema_reply,
    },
    ToolEntry {
        name: "post",
        description: "Post a new message to any channel (not as a thread reply).",
        scope: ToolScope::Both,
        tier: ActionTier::AutonomousWithNotice,
        frequency: Frequency::Sparingly,
        when: "Post to a different channel to surface cross-channel connections or proactively share relevant info. Don't duplicate — check if the target channel already knows.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_post,
    },
    ToolEntry {
        name: "no_action",
        description: "Explicitly take no action.",
        scope: ToolScope::Both,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Stay quiet when the message doesn't warrant response. Silence is often the right call — not everything needs a reaction.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_no_action,
    },
    ToolEntry {
        name: "create_skill",
        description: "Create or update a skill in your skill registry.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Codify a recurring behavioral pattern you want to remember. Skills are instructions for how to use your existing tools in specific situations, not new tools.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_create_skill,
    },
    ToolEntry {
        name: "read_file",
        description: "Read a file from your workspace.",
        scope: ToolScope::Both,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Check current state before making changes. Read before writing — always verify what's there first.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_read_file,
    },
    ToolEntry {
        name: "write_file",
        description: "Write a file to your workspace.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Persist structured state to workspace — tickets, notes, data files. Always read_file first to avoid overwriting.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_write_file,
    },
    ToolEntry {
        name: "dm_user",
        description: "Send a direct message to a specific user.",
        scope: ToolScope::Both,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "DM only for approval escalations, urgent notifications, or explicitly requested private messages. Always pair with a reply confirming you sent the DM.",
        is_information: false,
        is_reply: true,
        schema_fn: schema_dm_user,
    },
    ToolEntry {
        name: "channel_history",
        description: "Read recent messages from a channel.",
        scope: ToolScope::Both,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Get broader channel context beyond the current thread. During heartbeat, use this to investigate patterns — check for stale threads, unresolved questions, or cross-channel connections.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_channel_history,
    },
    ToolEntry {
        name: "lookup_user",
        description: "Search for a user by name.",
        scope: ToolScope::Both,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Look up user IDs before sending DMs. If someone says \"DM Josh\", use this first to get the correct user ID.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_lookup_user,
    },
    ToolEntry {
        name: "save_memory",
        description: "Persist a piece of knowledge to long-term memory.",
        scope: ToolScope::Both,
        tier: ActionTier::AutonomousWithNotice,
        frequency: Frequency::Always,
        when: "Save when you learn something new about people, projects, preferences, or decisions. If you might need it later, save it now. Err on the side of saving too much — memory is cheap, forgetting is expensive.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_save_memory,
    },
    ToolEntry {
        name: "recall_memory",
        description: "Search your long-term memory for information about a topic.",
        scope: ToolScope::Both,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Search memory when you need context you might have stored — names, decisions, preferences, project details. When someone asks \"what do you know about X?\", always check memory first.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_recall_memory,
    },
    ToolEntry {
        name: "log_decision",
        description: "Capture a decision that was made in a conversation.",
        scope: ToolScope::Both,
        tier: ActionTier::AutonomousWithNotice,
        frequency: Frequency::Always,
        when: "Capture any decision made in conversation: someone chose an approach, approved a plan, settled a debate, or set a direction. Decisions are easy to miss — \"let's go with X\" is a decision. Log it.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_log_decision,
    },
    ToolEntry {
        name: "update_intents",
        description: "Update INTENTS.md based on your observations.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Update when you notice a new project, priority shift, or recurring theme that should influence triage. Read INTENTS.md first. Provide the FULL updated content.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_update_intents,
    },
    ToolEntry {
        name: "create_channel",
        description: "Create a new channel.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Create a channel when a new project, workstream, or topic needs a dedicated space. Pick a clear, descriptive name. Always invite relevant people after creation.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_create_channel,
    },
    ToolEntry {
        name: "invite_to_channel",
        description: "Invite users to a channel.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Invite people to channels they should be in — e.g. when a new project channel is created, or when someone needs visibility into a conversation.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_invite_to_channel,
    },
    ToolEntry {
        name: "group_dm",
        description: "Start a group DM with multiple users.",
        scope: ToolScope::Both,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Start a group DM when a small set of specific people need to coordinate privately — e.g. pulling together the right 2-3 people for a quick decision. Not for announcements (use post). Always explain why you're grouping them.",
        is_information: false,
        is_reply: true,
        schema_fn: schema_group_dm,
    },
    ToolEntry {
        name: "run_script",
        description: "Execute a Python or shell script.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Run a script when you need to compute, transform data, or execute a skill handler. Explain what the script does before running it.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_run_script,
    },
    ToolEntry {
        name: "http_request",
        description: "Make an HTTP request to any URL.",
        scope: ToolScope::Event,
        tier: ActionTier::RequiresApproval,
        frequency: Frequency::Sparingly,
        when: "Call external APIs — GitHub, webhooks, REST services. Always explain why you're making the request before calling.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_http_request,
    },
    ToolEntry {
        name: "load_skill",
        description: "Load full instructions for a skill.",
        scope: ToolScope::Event,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Load a skill's full instructions before executing a complex workflow. Check the Skills list for available names.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_load_skill,
    },
    ToolEntry {
        name: "set_reminder",
        description: "Set a one-shot reminder that fires after a delay.",
        scope: ToolScope::Event,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Use when someone asks to be reminded of something, or when you identify a time-sensitive follow-up. Supports delays from 1 minute to 24 hours.",
        is_information: false,
        is_reply: false,
        schema_fn: schema_set_reminder,
    },
    ToolEntry {
        name: "connect_integration",
        description: "Generate an OAuth connection URL for an integration provider.",
        scope: ToolScope::Event,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Use when a user asks to connect an integration (Jira, Linear, Notion, Google Calendar, Gmail) or when a skill tool fails due to missing credentials. Returns a clickable link.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_connect_integration,
    },
    ToolEntry {
        name: "integration_status",
        description: "Check which integrations are connected via OAuth.",
        scope: ToolScope::Event,
        tier: ActionTier::Autonomous,
        frequency: Frequency::WhenRelevant,
        when: "Use when a user asks what integrations are available, or to check connection status before attempting an integration tool.",
        is_information: true,
        is_reply: false,
        schema_fn: schema_integration_status,
    },
];

// ── Derived functions ──────────────────────────────────────────────────

fn matches_scope(entry: &ToolEntry, scope: ToolScope) -> bool {
    entry.scope == ToolScope::Both || entry.scope == scope
}

/// Event tool schemas (OpenAI function-calling format). Replaces `delegate_tools()`.
pub fn event_tool_schemas() -> Vec<Value> {
    REGISTRY
        .iter()
        .filter(|e| matches_scope(e, ToolScope::Event))
        .map(|e| (e.schema_fn)())
        .collect()
}

/// Heartbeat/cron tool schemas. Replaces `heartbeat_tools()`.
pub fn heartbeat_tool_schemas() -> Vec<Value> {
    REGISTRY
        .iter()
        .filter(|e| matches_scope(e, ToolScope::Heartbeat))
        .map(|e| (e.schema_fn)())
        .collect()
}

pub fn classify_action(tool_name: &str) -> ActionTier {
    REGISTRY
        .iter()
        .find(|e| e.name == tool_name)
        .map(|e| e.tier)
        .unwrap_or(ActionTier::AutonomousWithNotice)
}

pub fn is_information_tool(name: &str) -> bool {
    REGISTRY
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.is_information)
        .unwrap_or(false)
}

pub fn is_reply_tool(name: &str) -> bool {
    REGISTRY
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.is_reply)
        .unwrap_or(false)
}

/// Generate a "Tool Playbook" section for the system prompt.
/// Only includes tools that match the given scope.
pub fn tool_playbook(scope: ToolScope) -> String {
    let mut lines = Vec::new();
    lines.push("# Tool Playbook\n".to_string());
    lines.push("Use the right tool for the job. You can call multiple tools at once — for example, react AND reply, or react AND save_memory.\n".to_string());

    for entry in REGISTRY.iter().filter(|e| matches_scope(e, scope)) {
        lines.push(format!(
            "- **{}** [{}]: {}",
            entry.name,
            entry.frequency.label(),
            entry.when,
        ));
    }

    lines.push(String::new());
    lines.push("**When someone asks a direct question, ALWAYS reply with text.** A reaction alone is never a sufficient response to a question. React + reply, or just reply — but never react-only when someone is asking you something.".to_string());
    lines.push(String::new());
    lines.push("Only say things you actually know. Never fabricate people, projects, or facts. If you don't have context, say so.".to_string());

    // Few-shot examples — models follow examples better than rules
    lines.push(String::new());
    lines.push("## Examples\n".to_string());

    lines.push(
        "**Someone shares new info casually:**\n\
         > \"heads up — Sarah's last day is Friday, she's handing off API work to Josh\"\n\
         → react(👍) + save_memory(topic: \"people\", content: updated team info) + reply(\"Got it — updated notes. Anything I should flag for the handoff?\")\n\
         *You learned something new. Save it immediately — you won't get a second chance.*\n".to_string()
    );

    lines.push(
        "**Someone announces a change that affects others:**\n\
         > \"just pushed a breaking change to the webhook payloads — field names changed from camelCase to snake_case\"\n\
         → recall_memory(\"webhook dashboard frontend\") → check who's affected → reply(\"Noted. Josh owns the dashboard and he's out until Thursday — this blocks his field mappings. I'll flag it to him.\") + save_memory\n\
         *When someone announces a change, your first instinct: who does this affect? Are they in this conversation? If not, reach out.*\n".to_string()
    );

    lines.push(
        "**Someone asks you to set up infrastructure:**\n\
         > \"we're kicking off API v2 next week — can you set up a channel and get Sarah and Alan in there?\"\n\
         → create_channel(name: \"api-v2\", purpose: \"API v2 rewrite\") + invite_to_channel(channel: \"api-v2\", users: [\"U_SARAH\", \"U_ALAN\"]) + reply(\"Done — #api-v2 is live, Sarah and Alan are in.\")\n\
         *When someone says \"set up a channel\" or \"make a channel\" — use create_channel, then invite_to_channel.*\n".to_string()
    );

    lines.join("\n")
}

// ── Schema functions ───────────────────────────────────────────────────

fn schema_react() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "react",
            "description": "Add an emoji reaction to the triggering message. Use this to acknowledge, signal thinking, show agreement, etc. Choose the emoji based on context — don't always use the same one.",
            "parameters": {
                "type": "object",
                "properties": {
                    "emoji": {
                        "type": "string",
                        "description": "Emoji name without colons. Examples: eyes, thumbsup, thinking_face, white_check_mark, wave, raised_hands, warning, memo, rocket"
                    }
                },
                "required": ["emoji"]
            }
        }
    })
}

fn schema_reply() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "reply",
            "description": "Reply to the triggering message in a thread. Use this when a substantive response is warranted — answering a question, flagging a risk, providing context, etc.",
            "parameters": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The message text to post as a threaded reply"
                    }
                },
                "required": ["text"]
            }
        }
    })
}

fn schema_post() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "post",
            "description": "Post a new message to any channel (not as a thread reply). Use this to proactively surface information in a different channel, e.g. alerting #platform-eng about something mentioned in #billing-migration.",
            "parameters": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "The channel ID or name to post to"
                    },
                    "text": {
                        "type": "string",
                        "description": "The message text to post"
                    }
                },
                "required": ["channel", "text"]
            }
        }
    })
}

fn schema_no_action() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "no_action",
            "description": "Explicitly take no action. Use this when the message doesn't warrant any response or reaction — sometimes the right move is to stay quiet.",
            "parameters": {
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "Brief internal note on why no action was taken"
                    }
                },
                "required": ["reason"]
            }
        }
    })
}

fn schema_create_skill() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "create_skill",
            "description": "Create or update a skill in your skill registry. Skills are behavioral patterns that guide how you use your tools. Use this when you notice a recurring pattern worth codifying — a type of message you handle the same way, a workflow you want to remember, or guidance from the team about how to behave. Skills are NOT new tools — they are instructions for how to use your existing tools (react, reply, post, no_action) in specific situations.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Skill name in kebab-case (e.g. summarize-thread, welcome-new-member, flag-blocker)"
                    },
                    "description": {
                        "type": "string",
                        "description": "One-line description of when this skill applies"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full skill instructions in markdown. Include: when to use, how to use your existing tools to accomplish it, what NOT to do, and any examples."
                    }
                },
                "required": ["name", "description", "content"]
            }
        }
    })
}

fn schema_read_file() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a file from your workspace. Path is relative to the workspace root. Use this to check current state before making changes — e.g. read tickets.json before updating it.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path within workspace (e.g. tickets.json, memory/people.md)"
                    }
                },
                "required": ["path"]
            }
        }
    })
}

fn schema_write_file() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write a file to your workspace. Path is relative to the workspace root. Creates parent directories if needed. Use this to persist state — tickets, notes, memory, data.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path within workspace (e.g. tickets.json, memory/people.md)"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content to write"
                    }
                },
                "required": ["path", "content"]
            }
        }
    })
}

fn schema_dm_user() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "dm_user",
            "description": "Send a direct message to a specific user. Use only for approval escalations or urgent notifications.",
            "parameters": {
                "type": "object",
                "properties": {
                    "user": {
                        "type": "string",
                        "description": "User ID to DM (e.g. U012345)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Message text to send as a DM"
                    }
                },
                "required": ["user", "text"]
            }
        }
    })
}

fn schema_channel_history() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "channel_history",
            "description": "Read recent messages from a channel. Returns the most recent messages (newest first). Use this to get broader context about what's happening in a channel beyond the current thread.",
            "parameters": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID or name to read history from"
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of messages to fetch (default 20, max 50)"
                    }
                },
                "required": ["channel"]
            }
        }
    })
}

fn schema_lookup_user() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "lookup_user",
            "description": "Search for a user by name. Returns matching user IDs and display names. Use this BEFORE dm_user when you don't have the user's ID — for example, if someone says 'DM Josh', look up 'Josh' first to get the correct user ID.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name to search for (matches against display name, real name, and username)"
                    }
                },
                "required": ["name"]
            }
        }
    })
}

fn schema_save_memory() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "save_memory",
            "description": "Persist a piece of knowledge to long-term memory. Writes to memory/{topic}.md and automatically updates MEMORY.md as a structured index. Use this when you learn something worth retaining: people's roles, project context, team preferences, decisions made, or corrections from the team. If the topic already exists, it will be overwritten — read it first if you want to append.",
            "parameters": {
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Topic slug in kebab-case (e.g. people, billing-migration, team-norms, standup-format)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Markdown content to persist. Be structured: use headings, bullets, and dates for context."
                    },
                    "summary": {
                        "type": "string",
                        "description": "One-line summary for the MEMORY.md index entry (e.g. 'Team members, roles, and working styles')"
                    }
                },
                "required": ["topic", "content", "summary"]
            }
        }
    })
}

fn schema_recall_memory() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "recall_memory",
            "description": "Search your long-term memory for information about a topic. Scans all memory files for matching content. Use this when someone asks 'what do you know about X?' or when you need context you might have stored previously. Returns matching excerpts from memory files.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for in memory (e.g. 'billing migration', 'Alan', 'team standup format')"
                    }
                },
                "required": ["query"]
            }
        }
    })
}

fn schema_log_decision() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "log_decision",
            "description": "Capture a decision that was made in a conversation. Use this when you observe a team decision: someone chose an approach, approved a plan, settled a debate, or set a direction. This creates a permanent record in memory/decisions.md with the decision, reasoning, participants, and date.",
            "parameters": {
                "type": "object",
                "properties": {
                    "decision": {
                        "type": "string",
                        "description": "What was decided (e.g. 'Use PostgreSQL for the new service instead of DynamoDB')"
                    },
                    "reasoning": {
                        "type": "string",
                        "description": "Why it was decided — the key arguments or constraints"
                    },
                    "participants": {
                        "type": "string",
                        "description": "Who was involved in making this decision (names or user IDs)"
                    },
                    "context": {
                        "type": "string",
                        "description": "Where the decision was made (channel, thread topic)"
                    }
                },
                "required": ["decision", "reasoning", "participants"]
            }
        }
    })
}

fn schema_update_intents() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "update_intents",
            "description": "Update INTENTS.md based on your observations. Use this when you notice a new project, priority shift, or recurring theme that should influence how you triage and respond. Read INTENTS.md first to understand the current state before modifying. Provide the FULL updated content — this replaces the file entirely.",
            "parameters": {
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Full updated INTENTS.md content in markdown"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Brief explanation of what changed and why (logged for auditability)"
                    }
                },
                "required": ["content", "reason"]
            }
        }
    })
}

fn schema_create_channel() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "create_channel",
            "description": "Create a new channel. Use this when a project, workstream, or topic needs a dedicated space for discussion. Pick a clear, descriptive name following team conventions.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Channel name in lowercase with hyphens (e.g. billing-migration, q2-planning, incident-2026-03-05)"
                    },
                    "purpose": {
                        "type": "string",
                        "description": "Short description of the channel's purpose"
                    }
                },
                "required": ["name"]
            }
        }
    })
}

fn schema_invite_to_channel() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "invite_to_channel",
            "description": "Invite one or more users to a channel. Use after creating a channel or when someone needs visibility into a conversation. Look up user IDs first if you only have names.",
            "parameters": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel ID or name to invite users to"
                    },
                    "users": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of user IDs to invite (e.g. [\"U012345\", \"U067890\"])"
                    }
                },
                "required": ["channel", "users"]
            }
        }
    })
}

fn schema_group_dm() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "group_dm",
            "description": "Start a group DM with multiple users and send a message. Use when a small set of specific people (2-4) need to coordinate privately — pulling together the right people for a quick decision, sensitive topic, or time-sensitive coordination. Not for announcements (use post for that).",
            "parameters": {
                "type": "object",
                "properties": {
                    "users": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of user IDs to include in the group DM (minimum 2)"
                    },
                    "text": {
                        "type": "string",
                        "description": "The message to send to the group"
                    }
                },
                "required": ["users", "text"]
            }
        }
    })
}

fn schema_run_script() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "run_script",
            "description": "Execute a Python or shell script. The script runs in the workspace directory with a 30-second timeout. Use this to compute, transform data, or run skill handler scripts.",
            "parameters": {
                "type": "object",
                "properties": {
                    "language": {
                        "type": "string",
                        "enum": ["python", "shell"],
                        "description": "Script language (python or shell)"
                    },
                    "code": {
                        "type": "string",
                        "description": "Script source code to execute"
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Command-line arguments to pass to the script"
                    }
                },
                "required": ["language", "code"]
            }
        }
    })
}

fn schema_http_request() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "http_request",
            "description": "Make an HTTP request to any URL. Use this to call external APIs (GitHub, webhooks, REST services). Returns the HTTP status code and response body.",
            "parameters": {
                "type": "object",
                "properties": {
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"],
                        "description": "HTTP method"
                    },
                    "url": {
                        "type": "string",
                        "description": "Full URL to request (e.g. https://api.github.com/repos/owner/repo/pulls)"
                    },
                    "headers": {
                        "type": "object",
                        "description": "HTTP headers as key-value pairs (e.g. {\"Authorization\": \"Bearer token\"})"
                    },
                    "body": {
                        "type": "string",
                        "description": "Request body (for POST/PUT/PATCH)"
                    }
                },
                "required": ["method", "url"]
            }
        }
    })
}

fn schema_load_skill() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "load_skill",
            "description": "Load the full instructions for a named skill. Use this to get detailed instructions before executing a skill workflow. The skill name must match one listed in the Skills section.",
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to load (e.g. 'ticket-tracker-write-json')"
                    }
                },
                "required": ["skill_name"]
            }
        }
    })
}

fn schema_set_reminder() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "set_reminder",
            "description": "Set a one-shot reminder that fires after a delay. The reminder will be posted to the target channel or DM, mentioning the user. Use this when someone asks to be reminded, or when you identify a time-sensitive follow-up.",
            "parameters": {
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "What to remind about (e.g. 'Stand-up in 5 minutes', 'Follow up on deployment')"
                    },
                    "delay_minutes": {
                        "type": "integer",
                        "description": "Minutes from now until the reminder fires (min 1, max 1440 = 24 hours)"
                    },
                    "target": {
                        "type": "string",
                        "description": "Channel name/ID or user ID to post the reminder in. Defaults to the current channel."
                    }
                },
                "required": ["message", "delay_minutes"]
            }
        }
    })
}

fn schema_connect_integration() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "connect_integration",
            "description": "Generate an OAuth connection link for an integration provider. The user clicks the link to authorize the bot. One click can cover multiple tools (e.g. 'atlassian' connects both Jira and Confluence).",
            "parameters": {
                "type": "object",
                "properties": {
                    "provider": {
                        "type": "string",
                        "description": "Provider to connect: 'atlassian' (Jira + Confluence), 'linear', 'notion', 'google' (Calendar + Gmail)",
                        "enum": ["atlassian", "linear", "notion", "google"]
                    }
                },
                "required": ["provider"]
            }
        }
    })
}

fn schema_integration_status() -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "integration_status",
            "description": "Check which integration providers are connected (via OAuth or API key/env var) and which are available to connect.",
            "parameters": {
                "type": "object",
                "properties": {}
            }
        }
    })
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_action ──

    #[test]
    fn classify_autonomous_tools() {
        assert_eq!(classify_action("react"), ActionTier::Autonomous);
        assert_eq!(classify_action("no_action"), ActionTier::Autonomous);
        assert_eq!(classify_action("read_file"), ActionTier::Autonomous);
        assert_eq!(classify_action("recall_memory"), ActionTier::Autonomous);
        assert_eq!(classify_action("channel_history"), ActionTier::Autonomous);
        assert_eq!(classify_action("lookup_user"), ActionTier::Autonomous);
        assert_eq!(classify_action("load_skill"), ActionTier::Autonomous);
        assert_eq!(classify_action("set_reminder"), ActionTier::Autonomous);
        assert_eq!(classify_action("connect_integration"), ActionTier::Autonomous);
        assert_eq!(classify_action("integration_status"), ActionTier::Autonomous);
    }

    #[test]
    fn classify_notice_tools() {
        assert_eq!(classify_action("reply"), ActionTier::AutonomousWithNotice);
        assert_eq!(classify_action("post"), ActionTier::AutonomousWithNotice);
        assert_eq!(classify_action("save_memory"), ActionTier::AutonomousWithNotice);
        assert_eq!(classify_action("log_decision"), ActionTier::AutonomousWithNotice);
    }

    #[test]
    fn classify_approval_tools() {
        assert_eq!(classify_action("dm_user"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("update_intents"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("write_file"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("create_skill"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("http_request"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("run_script"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("create_channel"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("invite_to_channel"), ActionTier::RequiresApproval);
        assert_eq!(classify_action("group_dm"), ActionTier::RequiresApproval);
    }

    #[test]
    fn classify_unknown_defaults_to_notice() {
        assert_eq!(classify_action("unknown_tool"), ActionTier::AutonomousWithNotice);
    }

    // ── is_information_tool / is_reply_tool ──

    #[test]
    fn information_tools() {
        assert!(is_information_tool("read_file"));
        assert!(is_information_tool("recall_memory"));
        assert!(is_information_tool("channel_history"));
        assert!(is_information_tool("lookup_user"));
        assert!(is_information_tool("load_skill"));
        assert!(is_information_tool("connect_integration"));
        assert!(is_information_tool("integration_status"));
        assert!(!is_information_tool("reply"));
        assert!(!is_information_tool("react"));
    }

    #[test]
    fn reply_tools() {
        assert!(is_reply_tool("reply"));
        assert!(is_reply_tool("dm_user"));
        assert!(!is_reply_tool("react"));
        assert!(!is_reply_tool("post"));
    }

    // ── schema counts ──

    #[test]
    fn event_schemas_include_all_tools() {
        let schemas = event_tool_schemas();
        assert_eq!(schemas.len(), 23, "All 23 tools should be available for events");
        // Verify each has a function name
        for s in &schemas {
            assert!(s["function"]["name"].as_str().is_some());
        }
    }

    #[test]
    fn heartbeat_schemas_are_subset() {
        let schemas = heartbeat_tool_schemas();
        assert_eq!(schemas.len(), 10, "Heartbeat should have 10 tools");
        let names: Vec<&str> = schemas
            .iter()
            .map(|s| s["function"]["name"].as_str().unwrap())
            .collect();
        // Event-only tools should not be in heartbeat
        assert!(!names.contains(&"react"));
        assert!(!names.contains(&"reply"));
        assert!(!names.contains(&"create_skill"));
        assert!(!names.contains(&"write_file"));
        assert!(!names.contains(&"update_intents"));
        assert!(!names.contains(&"set_reminder"));
        assert!(!names.contains(&"create_channel"));
        assert!(!names.contains(&"invite_to_channel"));
        // Core heartbeat tools should be present
        assert!(names.contains(&"post"));
        assert!(names.contains(&"save_memory"));
        assert!(names.contains(&"log_decision"));
        assert!(names.contains(&"no_action"));
        assert!(names.contains(&"channel_history"));
    }

    // ── tool_playbook ──

    #[test]
    fn playbook_contains_all_event_tools() {
        let playbook = tool_playbook(ToolScope::Event);
        for entry in REGISTRY {
            if matches_scope(entry, ToolScope::Event) {
                assert!(
                    playbook.contains(entry.name),
                    "Playbook missing tool: {}",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn playbook_contains_frequency_labels() {
        let playbook = tool_playbook(ToolScope::Event);
        assert!(playbook.contains("[ALWAYS]"), "Playbook should contain ALWAYS label");
        assert!(playbook.contains("[WHEN RELEVANT]"), "Playbook should contain WHEN RELEVANT label");
        assert!(playbook.contains("[SPARINGLY]"), "Playbook should contain SPARINGLY label");
    }

    #[test]
    fn playbook_save_memory_is_always() {
        let playbook = tool_playbook(ToolScope::Event);
        let line = playbook
            .lines()
            .find(|l| l.starts_with("- **save_memory**"))
            .expect("save_memory should be in playbook");
        assert!(line.contains("[ALWAYS]"), "save_memory should be ALWAYS, got: {line}");
    }

    #[test]
    fn playbook_log_decision_is_always() {
        let playbook = tool_playbook(ToolScope::Event);
        let line = playbook
            .lines()
            .find(|l| l.starts_with("- **log_decision**"))
            .expect("log_decision should be in playbook");
        assert!(line.contains("[ALWAYS]"), "log_decision should be ALWAYS, got: {line}");
    }

    #[test]
    fn heartbeat_playbook_excludes_event_only() {
        let playbook = tool_playbook(ToolScope::Heartbeat);
        assert!(!playbook.contains("- **react**"), "Heartbeat playbook should not contain react");
        assert!(!playbook.contains("- **reply**"), "Heartbeat playbook should not contain reply");
    }
}
