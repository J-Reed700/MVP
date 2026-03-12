//! Eval harness for memory recall & tool usage.
//!
//! Tests whether the agent correctly uses tools (recall_memory, save_memory,
//! set_reminder, etc.) to find information and answer multiple-choice questions.
//!
//! These tests hit a real LLM API and are marked `#[ignore]`. Run with:
//! ```sh
//! cargo test eval -- --ignored
//! ```
//!
//! Set `DELEGATE_PROVIDER` (default: "anthropic") and the corresponding API key
//! env var (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`).

mod fixtures;
mod mock;
mod persist;
mod runner;
mod scoring;

use serde_json::Value;

use fixtures::*;
use persist::{persist_run, scenario_to_json};
use runner::run_scenario;
use scoring::{check_answer, print_result};

// ── Test functions ───────────────────────────────────────────────────────

macro_rules! eval_test {
    ($name:ident, $scenario:expr) => {
        #[tokio::test]
        #[ignore]
        async fn $name() {
            let scenario = $scenario;
            let result = run_scenario(scenario).await.expect("scenario should run");
            let (answer_ok, tools_ok) = check_answer(&result, scenario);
            print_result(scenario, &result, answer_ok, tools_ok);
            assert!(answer_ok, "Wrong answer for {}", scenario.name);
        }
    };
}

// Original recall & basic scenarios
eval_test!(eval_01_recall_standup_preference, &SCENARIO_RECALL_STANDUP);
eval_test!(eval_02_recall_decision_noisy_log, &SCENARIO_RECALL_DECISION);
eval_test!(eval_03_recall_person_role, &SCENARIO_RECALL_PERSON);
eval_test!(eval_04_no_memory_available, &SCENARIO_NO_MEMORY);
eval_test!(eval_05_set_reminder_natural, &SCENARIO_SET_REMINDER);
eval_test!(eval_06_recall_team_norms, &SCENARIO_TEAM_NORMS);
eval_test!(eval_07_cross_file_synthesis, &SCENARIO_CROSS_FILE);
eval_test!(eval_08_casual_banter, &SCENARIO_CASUAL_BANTER);

// P0: save & log (ALWAYS frequency — previously untested)
eval_test!(eval_09_save_new_person_info, &SCENARIO_SAVE_PERSON_INFO);
eval_test!(eval_10_log_implicit_decision, &SCENARIO_LOG_IMPLICIT_DECISION);
eval_test!(eval_11_learn_and_acknowledge, &SCENARIO_LEARN_AND_ACKNOWLEDGE);

// P1: correction & judgment
eval_test!(eval_12_save_correction, &SCENARIO_SAVE_CORRECTION);
eval_test!(eval_13_react_only_no_reply, &SCENARIO_REACT_ONLY);
eval_test!(eval_14_ignore_bot_noise, &SCENARIO_IGNORE_BOT_NOISE);

// P2: synthesis & robustness
eval_test!(eval_15_synthesize_for_someone, &SCENARIO_SYNTHESIZE_FOR_SOMEONE);
eval_test!(eval_16_partial_info_honest, &SCENARIO_PARTIAL_INFO);
eval_test!(eval_17_multi_question, &SCENARIO_MULTI_QUESTION);

// P1: proactive outreach & mentions
eval_test!(eval_18_mention_relevant_person, &SCENARIO_MENTION_RELEVANT_PERSON);
eval_test!(eval_19_spontaneous_outreach, &SCENARIO_SPONTANEOUS_OUTREACH);

// Channel & group DM tools
eval_test!(eval_20_create_channel_for_project, &SCENARIO_CREATE_CHANNEL_FOR_PROJECT);
eval_test!(eval_21_invite_missing_person, &SCENARIO_INVITE_MISSING_PERSON);
eval_test!(eval_22_strategic_group_dm, &SCENARIO_STRATEGIC_GROUP_DM);

// Self-extending tools
eval_test!(eval_23_load_skill_progressive, &SCENARIO_LOAD_SKILL_PROGRESSIVE);
eval_test!(eval_24_http_request_api, &SCENARIO_HTTP_REQUEST_API);
eval_test!(eval_25_run_script_compute, &SCENARIO_RUN_SCRIPT_COMPUTE);
eval_test!(eval_26_skill_defined_tool, &SCENARIO_SKILL_DEFINED_TOOL);
eval_test!(eval_27_create_skill_self_extend, &SCENARIO_CREATE_SKILL_SELF_EXTEND);
eval_test!(eval_28_skill_not_found_honest, &SCENARIO_SKILL_NOT_FOUND_HONEST);

// Credential-aware integration scenarios
eval_test!(eval_29_skill_with_credentials, &SCENARIO_SKILL_WITH_CREDENTIALS);
eval_test!(eval_30_skill_missing_no_credentials, &SCENARIO_SKILL_MISSING_NO_CREDENTIALS);
eval_test!(eval_31_connect_integration, &SCENARIO_CONNECT_INTEGRATION);
eval_test!(eval_32_integration_status, &SCENARIO_INTEGRATION_STATUS);
eval_test!(eval_33_connect_google_covers_both, &SCENARIO_CONNECT_GOOGLE_COVERS_BOTH);
eval_test!(eval_34_partial_connectivity, &SCENARIO_PARTIAL_CONNECTIVITY);

// Spec-driven PM behavior scenarios
eval_test!(eval_35_synthesize_project_status, &SCENARIO_SYNTHESIZE_PROJECT_STATUS);
eval_test!(eval_36_provide_unrequested_context, &SCENARIO_PROVIDE_UNREQUESTED_CONTEXT);
eval_test!(eval_37_flag_blocker_proactively, &SCENARIO_FLAG_BLOCKER_PROACTIVELY);
eval_test!(eval_38_detect_scope_decision, &SCENARIO_DETECT_SCOPE_DECISION);
eval_test!(eval_39_write_standup_from_state, &SCENARIO_WRITE_STANDUP_FROM_STATE);
eval_test!(eval_40_tone_calibrate_executive, &SCENARIO_TONE_CALIBRATE_EXECUTIVE);
eval_test!(eval_41_memory_transparency_sources, &SCENARIO_MEMORY_TRANSPARENCY_SOURCES);
eval_test!(eval_42_recall_decision_alternatives, &SCENARIO_RECALL_DECISION_ALTERNATIVES);
eval_test!(eval_43_onboard_new_team_member, &SCENARIO_ONBOARD_NEW_TEAM_MEMBER);
eval_test!(eval_44_autonomous_action_notice, &SCENARIO_AUTONOMOUS_ACTION_NOTICE);
eval_test!(eval_45_scope_boundary_escalation, &SCENARIO_SCOPE_BOUNDARY_ESCALATION);
eval_test!(eval_46_write_status_for_human, &SCENARIO_WRITE_STATUS_FOR_HUMAN);
eval_test!(eval_47_cross_channel_digest, &SCENARIO_CROSS_CHANNEL_DIGEST);
eval_test!(eval_48_connect_related_information, &SCENARIO_CONNECT_RELATED_INFORMATION);

// Superhuman tier — 20 stories of extraordinary PM judgment
eval_test!(eval_49_silent_collision, &SCENARIO_SILENT_COLLISION);
eval_test!(eval_50_calendar_blindspot, &SCENARIO_CALENDAR_BLINDSPOT);
eval_test!(eval_51_unasked_question, &SCENARIO_UNASKED_QUESTION);
eval_test!(eval_52_misread_metric, &SCENARIO_MISREAD_METRIC);
eval_test!(eval_53_budget_interpreter, &SCENARIO_BUDGET_INTERPRETER);
eval_test!(eval_54_three_ticket_pattern, &SCENARIO_THREE_TICKET_PATTERN);
eval_test!(eval_55_meeting_assassin, &SCENARIO_MEETING_ASSASSIN);
eval_test!(eval_56_first_day_briefing, &SCENARIO_FIRST_DAY_BRIEFING);
eval_test!(eval_57_scope_surgeon, &SCENARIO_SCOPE_SURGEON);
eval_test!(eval_58_green_test_trap, &SCENARIO_GREEN_TEST_TRAP);
eval_test!(eval_59_thread_therapist, &SCENARIO_THREAD_THERAPIST);
eval_test!(eval_60_silent_failure_premortem, &SCENARIO_SILENT_FAILURE_PREMORTEM);
eval_test!(eval_61_timezone_play, &SCENARIO_TIMEZONE_PLAY);
eval_test!(eval_62_debt_ledger, &SCENARIO_DEBT_LEDGER);
eval_test!(eval_63_competitor_signal, &SCENARIO_COMPETITOR_SIGNAL);
eval_test!(eval_64_roi_translator, &SCENARIO_ROI_TRANSLATOR);
eval_test!(eval_65_reverse_escalation, &SCENARIO_REVERSE_ESCALATION);
eval_test!(eval_66_lunch_decision, &SCENARIO_LUNCH_DECISION);
eval_test!(eval_67_postmortem_reframe, &SCENARIO_POSTMORTEM_REFRAME);
eval_test!(eval_68_rate_limit_ghost, &SCENARIO_RATE_LIMIT_GHOST);

/// Runs all scenarios sequentially, prints a scorecard, and persists results
/// to `eval_results.json`.
#[tokio::test]
#[ignore]
async fn eval_scorecard() {
    let run_start = std::time::Instant::now();
    let scenarios = all_scenarios();
    let mut pass = 0;
    let mut fail = 0;
    let mut errors = 0;
    let mut run_results: Vec<Value> = Vec::new();

    println!("\n======== Eval Scorecard ========\n");

    for scenario in &scenarios {
        match run_scenario(scenario).await {
            Ok(result) => {
                let (answer_ok, tools_ok) = check_answer(&result, scenario);
                print_result(scenario, &result, answer_ok, tools_ok);
                run_results.push(scenario_to_json(scenario, &result, answer_ok, tools_ok));
                if answer_ok {
                    pass += 1;
                } else {
                    fail += 1;
                }
            }
            Err(e) => {
                println!("[ERROR] {} | {e}", scenario.name);
                run_results.push(serde_json::json!({
                    "scenario": scenario.name,
                    "status": "ERROR",
                    "error": e.to_string(),
                }));
                errors += 1;
            }
        }
    }

    let total = scenarios.len();
    let run_duration_ms = run_start.elapsed().as_millis() as u64;
    let run_secs = run_duration_ms as f64 / 1000.0;
    println!("\n--- Results: {pass}/{total} passed, {fail} failed, {errors} errors ({run_secs:.1}s) ---\n");

    persist_run(run_results, pass, fail, errors, run_duration_ms);

    assert!(
        fail == 0 && errors == 0,
        "Eval suite had {fail} failures and {errors} errors"
    );
}
