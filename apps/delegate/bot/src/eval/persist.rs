//! Results persistence — append eval runs to eval_results.json.

use std::path::PathBuf;

use serde_json::Value;

use super::runner::{EvalResult, Scenario};
use super::scoring::{agent_output, status_label};

/// Path to the results file, next to Cargo.toml.
pub(crate) fn results_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("eval_results.json")
}

/// A single scenario result in the persisted JSON.
pub(crate) fn scenario_to_json(
    scenario: &Scenario,
    result: &EvalResult,
    answer_ok: bool,
    tools_ok: bool,
) -> Value {
    let output = agent_output(result);
    let excerpt: String = output.chars().take(200).collect();
    serde_json::json!({
        "scenario": scenario.name,
        "status": status_label(answer_ok, tools_ok),
        "answer_correct": answer_ok,
        "tools_correct": tools_ok,
        "tools_called": result.tools_called,
        "expected_tools": scenario.expected_tools,
        "tokens_used": result.tokens_used,
        "duration_ms": result.duration_ms,
        "response_excerpt": excerpt,
    })
}

/// Read a git value via `git` CLI. Returns empty string on failure.
fn git_val(args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Append a run entry to eval_results.json.
pub(crate) fn persist_run(
    scenario_results: Vec<Value>,
    pass: usize,
    fail: usize,
    errors: usize,
    run_duration_ms: u64,
) {
    let path = results_path();

    // Load existing runs or start fresh
    let mut runs: Vec<Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let provider = std::env::var("DELEGATE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
    let model = std::env::var("DELEGATE_MODEL").unwrap_or_else(|_| "(default)".to_string());
    let total = pass + fail + errors;
    let total_tokens: u64 = scenario_results
        .iter()
        .filter_map(|r| r["tokens_used"].as_u64())
        .sum();
    let pass_rate = if total > 0 {
        (pass as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let run = serde_json::json!({
        "run_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Local::now().to_rfc3339(),
        "git": {
            "sha": git_val(&["rev-parse", "--short", "HEAD"]),
            "branch": git_val(&["rev-parse", "--abbrev-ref", "HEAD"]),
            "dirty": !git_val(&["status", "--porcelain"]).is_empty(),
        },
        "provider": provider,
        "model": model,
        "results": scenario_results,
        "summary": {
            "total": total,
            "pass": pass,
            "fail": fail,
            "errors": errors,
            "pass_rate_pct": (pass_rate * 10.0).round() / 10.0,
            "total_tokens": total_tokens,
            "duration_ms": run_duration_ms,
        }
    });

    runs.push(run);

    match std::fs::write(&path, serde_json::to_string_pretty(&runs).unwrap_or_default()) {
        Ok(_) => println!("Results saved to {}", path.display()),
        Err(e) => eprintln!("Failed to save results: {e}"),
    }
}
