use anyhow::Result;
use std::path::Path;

use crate::runner::{LifecycleStep, ScenarioResult, VariantResult};
use crate::triage::{TriageLabel, TriageMetrics, TriageResult};

/// Write scenario results to the results directory.
pub async fn write_results(result: &ScenarioResult, output_dir: &Path) -> Result<()> {
    let scenario_dir = output_dir.join(&result.scenario_id);
    tokio::fs::create_dir_all(&scenario_dir).await?;

    if let Some(ref variants) = result.variant_results {
        write_variant_results(result, variants, &scenario_dir).await?;
    } else if let (Some(ref triage_results), Some(ref metrics)) =
        (&result.triage_results, &result.triage_metrics)
    {
        write_triage_results(triage_results, metrics, &scenario_dir).await?;
    } else if let Some(ref steps) = result.lifecycle_steps {
        write_lifecycle_results(result, steps, &scenario_dir).await?;
    }

    Ok(())
}

/// Write the overall summary across all scenarios.
pub async fn write_summary(results: &[ScenarioResult], output_dir: &Path) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("# Eval Results Summary".to_string());
    lines.push(format!("\nRun: {}\n", chrono::Utc::now().to_rfc3339()));

    for result in results {
        lines.push(format!("## {}: {}", result.scenario_id, result.scenario_name));
        lines.push(format!("**Hypothesis:** {}\n", result.hypothesis));

        if let Some(ref variants) = result.variant_results {
            for v in variants {
                lines.push(format!("### {}", v.variant_label));
                lines.push(format!("- Model: {}", v.response.model));
                lines.push(format!("- Input tokens: {}", v.response.input_tokens));
                lines.push(format!("- Output tokens: {}", v.response.output_tokens));
                lines.push(format!("- Duration: {}ms", v.response.duration_ms));
                lines.push(format!(
                    "- Context: identity={}, intents={}, memory={}, retrieved={}",
                    v.compiled_context.identity_tokens,
                    v.compiled_context.intents_tokens,
                    v.compiled_context.memory_tokens,
                    v.compiled_context.retrieved_tokens
                ));
                lines.push(String::new());
            }
        }

        if let Some(ref m) = result.triage_metrics {
            lines.push("### Metrics".to_string());
            lines.push(format!(
                "- Overall agreement: {:.1}% (target: >85%)",
                m.overall_agreement * 100.0
            ));
            lines.push(format!(
                "- False negative rate (missed urgent): {:.1}% (target: <5%)",
                m.false_negative_rate * 100.0
            ));
            lines.push(format!(
                "- False positive rate (false urgent): {:.1}% (target: <20%)",
                m.false_positive_rate * 100.0
            ));
            lines.push(format!(
                "- Act-now events: {}/{}",
                m.act_now_events, m.total
            ));
            lines.push(format!("- Missed act-now: {}", m.missed_act_now));
            lines.push(format!("- False act-now: {}", m.false_act_now));

            let pass = m.overall_agreement >= 0.85 && m.false_negative_rate < 0.05;
            lines.push(format!(
                "\n**Result: {}**",
                if pass { "PASS" } else { "FAIL" }
            ));
            lines.push(String::new());
        }

        if let Some(ref steps) = result.lifecycle_steps {
            lines.push("### Steps".to_string());
            for step in steps {
                let updated = if step.updated_intents.is_some() {
                    "YES"
                } else {
                    "no"
                };
                lines.push(format!(
                    "- Event {} ({}): updated={}",
                    step.event_id, step.event_description, updated
                ));
            }
            lines.push(String::new());
        }

        lines.push("---\n".to_string());
    }

    tokio::fs::write(output_dir.join("summary.md"), lines.join("\n")).await?;
    Ok(())
}

async fn write_variant_results(
    result: &ScenarioResult,
    variants: &[VariantResult],
    dir: &Path,
) -> Result<()> {
    for v in variants {
        let content = format!(
            "# {}\n\n\
             Model: {}\n\
             Input tokens: {}\n\
             Output tokens: {}\n\
             Duration: {}ms\n\n\
             ## Context Stats\n\
             - Identity: ~{} tokens\n\
             - Intents: ~{} tokens\n\
             - Memory: ~{} tokens\n\
             - Retrieved: ~{} tokens\n\
             - Total: ~{} tokens\n\n\
             ## Response\n\n\
             {}",
            v.variant_label,
            v.response.model,
            v.response.input_tokens,
            v.response.output_tokens,
            v.response.duration_ms,
            v.compiled_context.identity_tokens,
            v.compiled_context.intents_tokens,
            v.compiled_context.memory_tokens,
            v.compiled_context.retrieved_tokens,
            v.compiled_context.total_tokens,
            v.response.content,
        );

        tokio::fs::write(dir.join(format!("{}.md", v.variant_id)), content).await?;
    }

    let comparison = build_comparison(&result.scenario_name, variants);
    tokio::fs::write(dir.join("comparison.md"), comparison).await?;

    Ok(())
}

fn build_comparison(name: &str, variants: &[VariantResult]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("# Comparison: {name}\n"));

    for v in variants {
        lines.push(format!("## {}\n", v.variant_label));
        lines.push(v.response.content.clone());
        lines.push("\n---\n".to_string());
    }

    lines.push("## Evaluation Notes\n".to_string());
    lines.push(
        "_Compare the variants above against the evaluation criteria in the scenario definition._\n"
            .to_string(),
    );
    lines.push("Questions to consider:".to_string());
    lines.push("- Is there a meaningful qualitative difference between variants?".to_string());
    lines.push(
        "- Does the hypothesis hold? Would someone unfamiliar with the setup agree?".to_string(),
    );
    lines.push("- Are there surprising similarities or differences?".to_string());

    lines.join("\n")
}

async fn write_triage_results(
    results: &[TriageResult],
    metrics: &TriageMetrics,
    dir: &Path,
) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("# Triage Results\n".to_string());
    lines.push("## Confusion Matrix\n".to_string());
    lines.push("Rows = human label, Columns = model label\n".to_string());
    lines.push("| | ignore | queue | act-now |".to_string());
    lines.push("|---|---|---|---|".to_string());

    for row in TriageLabel::all() {
        let cells: Vec<String> = TriageLabel::all()
            .iter()
            .map(|col| {
                metrics
                    .confusion_matrix
                    .get(&row)
                    .and_then(|r| r.get(col))
                    .copied()
                    .unwrap_or(0)
                    .to_string()
            })
            .collect();
        lines.push(format!("| **{row}** | {} |", cells.join(" | ")));
    }

    lines.push(format!(
        "\n## Metrics\n\n\
         - Overall agreement: **{:.1}%** (target: >85%)\n\
         - False negative rate: **{:.1}%** (target: <5%)\n\
         - False positive rate: **{:.1}%** (target: <20%)",
        metrics.overall_agreement * 100.0,
        metrics.false_negative_rate * 100.0,
        metrics.false_positive_rate * 100.0,
    ));

    lines.push("\n## Detailed Results\n".to_string());
    lines.push("| ID | Human | Model | Match | Reasoning |".to_string());
    lines.push("|---|---|---|---|---|".to_string());

    for r in results {
        let match_str = if r.correct { "yes" } else { "**NO**" };
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            r.event_id, r.human_label, r.model_label, match_str, r.model_reasoning
        ));
    }

    let misses: Vec<_> = results.iter().filter(|r| !r.correct).collect();
    if !misses.is_empty() {
        lines.push("\n## Misclassifications\n".to_string());
        for r in misses {
            lines.push(format!("### Event {}", r.event_id));
            lines.push(format!("- Human: {}", r.human_label));
            lines.push(format!("- Model: {}", r.model_label));
            lines.push(format!("- Reasoning: {}", r.model_reasoning));
            lines.push(String::new());
        }
    }

    tokio::fs::write(dir.join("triage-results.md"), lines.join("\n")).await?;
    Ok(())
}

async fn write_lifecycle_results(
    result: &ScenarioResult,
    steps: &[LifecycleStep],
    dir: &Path,
) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("# Intent Lifecycle Results\n".to_string());
    lines.push(format!("Hypothesis: {}\n", result.hypothesis));

    for step in steps {
        lines.push(format!(
            "## Event {}: {}\n",
            step.event_id, step.event_description
        ));
        lines.push("### Model Response\n".to_string());
        lines.push(step.response.content.clone());
        lines.push(String::new());

        if let Some(ref updated) = step.updated_intents {
            lines.push("### Updated INTENTS.md\n".to_string());
            lines.push("```markdown".to_string());
            lines.push(updated.clone());
            lines.push("```".to_string());
        } else {
            lines.push("_No update to INTENTS.md_".to_string());
        }

        lines.push("\n---\n".to_string());
    }

    let last_update = steps.iter().rev().find(|s| s.updated_intents.is_some());
    if let Some(step) = last_update {
        lines.push("## Final INTENTS.md State\n".to_string());
        lines.push(step.updated_intents.as_ref().unwrap().clone());
    }

    tokio::fs::write(dir.join("lifecycle-results.md"), lines.join("\n")).await?;

    // Write each step individually for easy diffing
    for step in steps {
        if let Some(ref updated) = step.updated_intents {
            let filename = format!("step-{:02}-intents.md", step.event_id);
            tokio::fs::write(dir.join(filename), updated).await?;
        }
    }

    Ok(())
}
