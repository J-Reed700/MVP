//! Scoring — check answers, format results, status labels.
//! Enhanced with content-quality validation: non-empty replies, no thinking
//! tag leaks, refusal detection, upload verification, and tool ordering.

use super::runner::{EvalResult, Scenario};

/// The full text the agent produced: content + reply tool text.
pub(crate) fn agent_output(result: &EvalResult) -> String {
    let mut parts = Vec::new();
    if !result.response.is_empty() {
        parts.push(result.response.as_str());
    }
    if !result.reply_text.is_empty() {
        parts.push(result.reply_text.as_str());
    }
    parts.join("\n")
}

// ── Content-quality checks ──────────────────────────────────────────────

/// Detect thinking/reasoning tag leaks in text sent to users.
/// These XML tags should never appear in outbound messages.
fn has_thinking_tags(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("<thinking>") || lower.contains("</thinking>")
        || lower.contains("<reasoning>") || lower.contains("</reasoning>")
        || lower.contains("<inner_monologue>")
}

/// Detect "I can't do that" refusals when the bot should be resourceful.
fn has_refusal(text: &str) -> bool {
    let lower = text.to_lowercase();
    let refusal_phrases = [
        "i can't do that",
        "i'm not able to",
        "i don't have the ability",
        "i cannot do that",
        "i'm unable to",
        "that's not something i can",
        "i don't have access to do",
        "outside my capabilities",
        "beyond my capabilities",
        "i lack the ability",
    ];
    refusal_phrases.iter().any(|p| lower.contains(p))
}

/// Check that the agent actually sent a visible reply (not just a reaction).
/// Returns false if the only outbound action was a reaction with no text.
fn has_visible_reply(result: &EvalResult) -> bool {
    // If there's text content in the response, that counts
    if !result.response.trim().is_empty() {
        return true;
    }
    // If there's text from reply/post tools, that counts
    if !result.reply_text.trim().is_empty() {
        return true;
    }
    // Check messenger log for any post_message or send_dm
    result.messenger_log.iter().any(|entry| {
        entry.starts_with("post_message(") || entry.starts_with("send_dm(")
    })
}

/// Check that upload_file was called (for scenarios that expect file output).
fn has_upload(result: &EvalResult) -> bool {
    result.messenger_log.iter().any(|entry| entry.starts_with("upload_file("))
}

// ── Scenario flags ──────────────────────────────────────────────────────

/// Scenarios can encode extra validation requirements in their name using
/// suffixes. This keeps the Scenario struct unchanged while enabling richer
/// scoring.
///
/// Conventions:
///   name contains "must_reply"     → agent MUST produce a visible text reply
///   name contains "must_upload"    → agent MUST call upload_file
///   name contains "no_refusal"     → agent MUST NOT refuse (resourcefulness)
///   name contains "no_tags"        → agent MUST NOT leak thinking tags
///
/// All scenarios always get thinking-tag checks (it's always a bug).

pub(crate) fn check_answer(result: &EvalResult, scenario: &Scenario) -> (bool, bool) {
    let name = scenario.name;
    let tools_correct = scenario
        .expected_tools
        .iter()
        .all(|t| result.tools_called.iter().any(|c| c == t));

    let has_flags = name.contains("must_reply") || name.contains("must_upload")
        || name.contains("no_refusal") || name.contains("no_tags");

    let answer_correct = if scenario.correct_answer.is_empty() && scenario.expected_tools.is_empty() && !has_flags {
        // Lightweight-only check: pass if agent only used lightweight tools.
        let heavy_tools: Vec<&str> = result
            .tools_called
            .iter()
            .filter(|t| !matches!(t.as_str(), "react" | "no_action" | "reply"))
            .map(|s| s.as_str())
            .collect();
        heavy_tools.is_empty()
    } else if scenario.correct_answer.is_empty() && scenario.expected_tools.is_empty() && has_flags {
        // Flag-gated scenario: base check passes, flags below handle validation.
        true
    } else if scenario.correct_answer.is_empty() {
        // Action-only check: no text to verify, pass if expected tools were called.
        tools_correct
    } else {
        // Text check: verify the agent's output contains the expected answer.
        let output = agent_output(result).to_lowercase();
        let answer_lower = scenario.correct_answer.to_lowercase();
        output.contains(&answer_lower)
    };

    // ── Content-quality gates (always checked) ──────────────────────────

    let output = agent_output(result);

    // Thinking tags should NEVER leak — fail any scenario that leaks them
    if has_thinking_tags(&output) {
        return (false, tools_correct);
    }

    // Check for thinking tags in messenger log text too
    for entry in &result.messenger_log {
        if has_thinking_tags(entry) {
            return (false, tools_correct);
        }
    }

    // Scenarios flagged "must_reply" must produce a visible text reply
    if name.contains("must_reply") && !has_visible_reply(result) {
        return (false, tools_correct);
    }

    // Scenarios flagged "must_upload" must call upload_file
    if name.contains("must_upload") && !has_upload(result) {
        return (false, tools_correct);
    }

    // Scenarios flagged "no_refusal" must not contain refusal language
    if name.contains("no_refusal") && has_refusal(&output) {
        return (false, tools_correct);
    }

    (answer_correct, tools_correct)
}

pub(crate) fn status_label(answer_ok: bool, tools_ok: bool) -> &'static str {
    if answer_ok && tools_ok {
        "PASS"
    } else if answer_ok {
        "PARTIAL"
    } else {
        "FAIL"
    }
}

pub(crate) fn print_result(scenario: &Scenario, result: &EvalResult, answer_ok: bool, tools_ok: bool) {
    let status = status_label(answer_ok, tools_ok);
    let output = agent_output(result);
    let excerpt: String = output.chars().take(120).collect();
    let secs = result.duration_ms as f64 / 1000.0;
    println!(
        "[{status}] {name} | {secs:.1}s | tools: {tools:?} | tokens: {tokens} | {excerpt:?}",
        name = scenario.name,
        tools = result.tools_called,
        tokens = result.tokens_used,
    );
}
