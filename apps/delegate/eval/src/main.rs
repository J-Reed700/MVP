mod compiler;
mod models;
mod report;
mod retriever;
mod runner;
mod triage;

use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};

use models::ModelClient;
use report::{write_results, write_summary};
use runner::{
    load_scenario, run_lifecycle_scenario, run_triage_scenario, run_variant_scenario,
    ScenarioResult,
};

#[derive(Parser)]
#[command(name = "delegate-eval", about = "Delegate Eval Harness")]
struct Cli {
    /// Run a single scenario by prefix (e.g. "01")
    #[arg(long)]
    scenario: Option<String>,

    /// Path to workspace directory
    #[arg(long, default_value = "workspace")]
    workspace: String,

    /// Override the model to use
    #[arg(long)]
    model: Option<String>,

    /// Model provider: "anthropic" or "openai"
    #[arg(long, default_value = "anthropic")]
    provider: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let workspace = std::fs::canonicalize(&cli.workspace)
        .unwrap_or_else(|_| PathBuf::from(&cli.workspace));

    println!("Delegate Eval Harness");
    println!("=====================");
    println!("Workspace: {}", workspace.display());
    println!("Provider: {}", cli.provider);
    if let Some(ref model) = cli.model {
        println!("Model override: {model}");
    }
    println!();

    // Verify workspace exists
    if !workspace.exists() {
        eprintln!("Workspace not found: {}", workspace.display());
        std::process::exit(1);
    }

    // Find scenario files
    let scenarios_dir = Path::new("scenarios");
    let pattern = if let Some(ref prefix) = cli.scenario {
        format!("{}/{}*.jsonc", scenarios_dir.display(), prefix)
    } else {
        format!("{}/*.jsonc", scenarios_dir.display())
    };
    let pattern = pattern.replace('\\', "/");

    let mut scenario_files: Vec<PathBuf> = glob::glob(&pattern)?
        .filter_map(|e| e.ok())
        .collect();
    scenario_files.sort();

    if scenario_files.is_empty() {
        eprintln!("No scenarios found matching: {pattern}");
        std::process::exit(1);
    }

    println!("Found {} scenario(s):\n", scenario_files.len());
    for f in &scenario_files {
        println!("  - {}", f.file_name().unwrap_or_default().to_string_lossy());
    }
    println!();

    // Create output directory
    let timestamp = chrono::Utc::now()
        .format("%Y-%m-%dT%H-%M-%S")
        .to_string();
    let output_dir = Path::new("results").join(&timestamp);
    tokio::fs::create_dir_all(&output_dir).await?;
    println!("Results: {}\n", output_dir.display());

    // Create model client
    let client = ModelClient::new(&cli.provider)?;

    // Run scenarios
    let mut results: Vec<ScenarioResult> = Vec::new();

    for scenario_file in &scenario_files {
        let scenario = load_scenario(scenario_file).await?;
        println!("Running: {} — {}", scenario.id, scenario.name);

        let result = if scenario.id == "04-triage" {
            run_triage_scenario(&scenario, &client, cli.model.as_deref()).await?
        } else if scenario.id == "05-intent-lifecycle" {
            run_lifecycle_scenario(
                &scenario,
                &workspace.to_string_lossy(),
                &client,
                cli.model.as_deref(),
            )
            .await?
        } else {
            run_variant_scenario(
                &scenario,
                &workspace.to_string_lossy(),
                &client,
                cli.model.as_deref(),
            )
            .await?
        };

        write_results(&result, &output_dir).await?;
        results.push(result);

        println!("  Done.\n");
    }

    // Write summary
    write_summary(&results, &output_dir).await?;

    println!("=====================");
    println!("All scenarios complete.");
    println!("Results written to: {}", output_dir.display());

    // Print quick summary for triage if applicable
    if let Some(result) = results.iter().find(|r| r.triage_metrics.is_some()) {
        let m = result.triage_metrics.as_ref().unwrap();
        println!("\nTriage Quick Summary:");
        println!(
            "  Overall agreement: {:.1}% {}",
            m.overall_agreement * 100.0,
            if m.overall_agreement >= 0.85 { "(PASS)" } else { "(FAIL)" }
        );
        println!(
            "  Missed urgent: {:.1}% {}",
            m.false_negative_rate * 100.0,
            if m.false_negative_rate < 0.05 { "(PASS)" } else { "(FAIL)" }
        );
        println!(
            "  False urgent: {:.1}% {}",
            m.false_positive_rate * 100.0,
            if m.false_positive_rate < 0.20 { "(PASS)" } else { "(FAIL)" }
        );
    }

    Ok(())
}
