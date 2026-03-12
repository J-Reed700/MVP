# Delegate Evaluation Harness

Tests the core hypothesis: context assembly (IDENTITY.md + INTENTS.md + intent-biased retrieval + audience framing) makes a frontier model act like a great PM.

## Setup

```bash
cd apps/delegate/eval
cargo build
```

Set your API key:
```bash
export ANTHROPIC_API_KEY=sk-...
```

## Running

```bash
# Run all scenarios
cargo run

# Run a single scenario
cargo run -- --scenario 01

# Use a different model
cargo run -- --model claude-sonnet-4-6

# Use a different workspace
cargo run -- --workspace ./alt-workspace

# Use OpenAI instead
cargo run -- --provider openai
```

## Scenarios

| # | Name | Hypothesis | Kill Condition |
|---|------|-----------|----------------|
| 01 | Intent Impact | INTENTS.md dramatically changes output quality | No meaningful difference between variants |
| 02 | Audience Framing | Identity + framing produce audience-appropriate writing | Outputs feel like same text at different verbosity |
| 03 | Retrieval Bias | Intent-biased retrieval produces strategically better answers | Both variants produce equivalent responses |
| 04 | Triage Classification | Cheap model + intent summary classifies events accurately | <85% agreement or >5% missed urgent |
| 05 | Intent Lifecycle | Model can maintain INTENTS.md as events unfold | Updates miss obvious signals or drift from reality |

## Workspace

The `workspace/` directory contains a simulated team workspace for "Platform Team at Acme" — a realistic engineering team with 7 days of activity logs, team dynamics, and two active projects.

## Results

Results are written to `results/{timestamp}/` with side-by-side comparisons and agreement matrices.
