//! Scoring — check answers, format results, status labels.

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

pub(crate) fn check_answer(result: &EvalResult, scenario: &Scenario) -> (bool, bool) {
    let tools_correct = scenario
        .expected_tools
        .iter()
        .all(|t| result.tools_called.iter().any(|c| c == t));

    let answer_correct = if scenario.correct_answer.is_empty() && scenario.expected_tools.is_empty() {
        // Lightweight-only check: pass if agent only used lightweight tools.
        // For casual banter, bot noise, etc. where the right move is minimal action.
        let heavy_tools: Vec<&str> = result
            .tools_called
            .iter()
            .filter(|t| !matches!(t.as_str(), "react" | "no_action" | "reply"))
            .map(|s| s.as_str())
            .collect();
        heavy_tools.is_empty()
    } else if scenario.correct_answer.is_empty() {
        // Action-only check: no text to verify, pass if expected tools were called.
        // For scenarios like "save this info" or "create a channel" where the action IS the answer.
        tools_correct
    } else {
        // Text check: verify the agent's output contains the expected answer.
        let output = agent_output(result).to_lowercase();
        let answer_lower = scenario.correct_answer.to_lowercase();
        output.contains(&answer_lower)
    };

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
