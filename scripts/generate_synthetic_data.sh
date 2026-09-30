#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# generate_synthetic_data.sh
#
# Generates interconnected test data across all 8 integrations that tells a
# cohesive product story. The first items in every integration are "canonical"
# story items with explicit cross-references. Bulk items reference the same
# projects and people to maintain consistency.
#
# Story: Acme product team, Sprint 12 wrapping up. Four key initiatives —
#   1. Onboarding Redesign (Jira DEV-42, Linear ENG-101, GitHub #235/PR#189, Figma abc123XYZ)
#   2. API v2 Launch (Jira DEV-50, Linear ENG-102, GitHub #230/PR#191)
#   3. Mobile Deep Link Bug (Jira DEV-34, Linear ENG-103, GitHub #234/PR#190) — blocker
#   4. Dark Mode (Jira DEV-55, Linear ENG-110, GitHub #236/PR#192) — design phase
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ── Shared universe constants ────────────────────────────────────────────────
PEOPLE_NAMES=("Alice Chen" "Bob Park" "Carlos Rivera" "Diana Wu" "Eve Zhang")
PEOPLE_EMAILS=("alice@acme.io" "bob@acme.io" "carlos@acme.io" "diana@acme.io" "eve@acme.io")
PEOPLE_GH=("alice-chen" "bob-park" "carlos-rivera" "diana-wu" "eve-zhang")

PROJECTS=("Onboarding Redesign" "API v2 Platform" "Dashboard v2" "Mobile App" "Billing" "Infrastructure" "Developer Experience")
JIRA_EPICS=("DEV-42" "DEV-50" "DEV-55" "DEV-34")
EPIC_NAMES=("Onboarding Redesign" "API v2 Launch" "Dark Mode" "Mobile Deep Link Fix")

# ── Counts (overridable via env) ─────────────────────────────────────────────
JIRA_COUNT="${JIRA_SYNTH_COUNT:-1000}"
GONG_COUNT="${GONG_SYNTH_COUNT:-5000}"
LINEAR_COUNT="${LINEAR_SYNTH_COUNT:-200}"
NOTION_COUNT="${NOTION_SYNTH_COUNT:-50}"
CONFLUENCE_COUNT="${CONFLUENCE_SYNTH_COUNT:-50}"
GITHUB_ISSUE_COUNT="${GITHUB_ISSUE_SYNTH_COUNT:-100}"
GITHUB_PR_COUNT="${GITHUB_PR_SYNTH_COUNT:-50}"
FIGMA_COMMENT_COUNT="${FIGMA_COMMENT_SYNTH_COUNT:-100}"
GCAL_COUNT="${GCAL_SYNTH_COUNT:-30}"
GMAIL_COUNT="${GMAIL_SYNTH_COUNT:-50}"

# ── Output files ─────────────────────────────────────────────────────────────
JIRA_FILE="$ROOT_DIR/testdata/jira/wiremock/__files/jira_search_response.json"
GONG_FILE="$ROOT_DIR/testdata/gong/wiremock/__files/gong_events.json"
LINEAR_FILE="$ROOT_DIR/testdata/linear/wiremock/__files/linear_issues_response.json"
NOTION_FILE="$ROOT_DIR/testdata/notion/wiremock/__files/notion_search_response.json"
CONFLUENCE_FILE="$ROOT_DIR/testdata/confluence/wiremock/__files/confluence_search_response.json"
GITHUB_ISSUES_FILE="$ROOT_DIR/testdata/github/wiremock/__files/github_issues_response.json"
GITHUB_PRS_FILE="$ROOT_DIR/testdata/github/wiremock/__files/github_prs_response.json"
GITHUB_RUNS_FILE="$ROOT_DIR/testdata/github/wiremock/__files/github_runs_response.json"
FIGMA_COMMENTS_FILE="$ROOT_DIR/testdata/figma/wiremock/__files/figma_comments_response.json"
GCAL_FILE="$ROOT_DIR/testdata/google/wiremock/__files/gcal_events_response.json"
GMAIL_LIST_FILE="$ROOT_DIR/testdata/google/wiremock/__files/gmail_messages_response.json"
GMAIL_MSG_FILE="$ROOT_DIR/testdata/google/wiremock/__files/gmail_message_response.json"

for f in "$JIRA_FILE" "$GONG_FILE" "$LINEAR_FILE" "$NOTION_FILE" "$CONFLUENCE_FILE" \
         "$GITHUB_ISSUES_FILE" "$GITHUB_PRS_FILE" "$GITHUB_RUNS_FILE" "$FIGMA_COMMENTS_FILE" \
         "$GCAL_FILE" "$GMAIL_LIST_FILE" "$GMAIL_MSG_FILE"; do
  mkdir -p "$(dirname "$f")"
done

# ═════════════════════════════════════════════════════════════════════════════
# JIRA — Canonical epics + sub-tasks with coherent cross-references
#
# Design:
#   - 4 canonical epic issues (DEV-42/50/55/34) written FIRST with hand-written
#     summary + description that matches their theme exactly.
#   - Then (JIRA_COUNT - 4) sub-tasks following a fixed summary/epic pattern.
#   - Every sub-task's description names its epic AND the correct theme name —
#     these are derived together via epic_theme_name() so they can't drift.
# ═════════════════════════════════════════════════════════════════════════════

# Map a canonical epic key to its theme name.
# This is the single source of truth — every other function derives from it.
epic_theme_name() {
  case "$1" in
    DEV-42) echo "Onboarding Redesign";;
    DEV-50) echo "API v2 Launch";;
    DEV-55) echo "Dark Mode";;
    DEV-34) echo "Mobile Deep Link Fix";;
    *)      echo "Unknown Initiative";;
  esac
}

# Sub-task summary by index. Sub-tasks 0..19 cycle through 20 story-coherent
# summaries. Each slot is paired with a specific epic in jira_story_epic().
jira_story_summary() {
  local idx=$1
  case $(( idx % 20 )) in
    0) echo "Implement welcome screen for onboarding redesign";;
    1) echo "Profile setup step with skip option";;
    2) echo "Team invite flow implementation";;
    3) echo "Onboarding progress bar component";;
    4) echo "Onboarding analytics tracking";;
    5) echo "Onboarding team invite email template";;
    6) echo "Onboarding profile step skip option (Figma abc123XYZ)";;
    7) echo "API v2 URL-based versioning with Kong";;
    8) echo "Rate limiting middleware for API v2";;
    9) echo "API v2 backward-compatible v1 proxy";;
    10) echo "API v2 request/response versioning headers";;
    11) echo "API v2 OpenAPI spec and documentation";;
    12) echo "Dark mode toggle component";;
    13) echo "Theme system with design token support";;
    14) echo "Dark mode chart color palette";;
    15) echo "Dashboard chart rendering optimization for dark mode";;
    16) echo "Android 14 deep link intent filter fix";;
    17) echo "iOS universal link configuration update";;
    18) echo "Deep link fallback to web for unsupported devices";;
    19) echo "Mobile onboarding push notification deep link";;
  esac
}

# Sub-task → epic mapping. MUST stay in sync with jira_story_summary: slots
# 0..6 are onboarding, 7..11 are API v2, 12..15 are dark mode, 16..19 are mobile.
jira_story_epic() {
  local idx=$1
  case $(( idx % 20 )) in
    0|1|2|3|4|5|6)  echo "DEV-42";;
    7|8|9|10|11)    echo "DEV-50";;
    12|13|14|15)    echo "DEV-55";;
    16|17|18|19)    echo "DEV-34";;
  esac
}

# Emit a single Jira issue record. Args: key, summary, description, status,
# assignee_idx, priority, labels_csv, updated_ts, is_last.
emit_jira_issue() {
  local key="$1" summary="$2" description="$3" status="$4"
  local assignee_idx="$5" priority="$6" labels_csv="$7" updated_ts="$8" is_last="$9"

  # Build labels JSON array from CSV
  local labels_json="["
  IFS=',' read -ra LBLS <<< "$labels_csv"
  for li in "${!LBLS[@]}"; do
    labels_json+="\"${LBLS[$li]}\""
    (( li < ${#LBLS[@]} - 1 )) && labels_json+=","
  done
  labels_json+="]"

  printf '    {\n' >> "$JIRA_FILE"
  printf '      "key": "%s",\n' "$key" >> "$JIRA_FILE"
  printf '      "fields": {\n' >> "$JIRA_FILE"
  printf '        "summary": "%s",\n' "$summary" >> "$JIRA_FILE"
  printf '        "description": "%s",\n' "$description" >> "$JIRA_FILE"
  printf '        "labels": %s,\n' "$labels_json" >> "$JIRA_FILE"
  printf '        "updated": "%s",\n' "$updated_ts" >> "$JIRA_FILE"
  printf '        "status": {"name": "%s"},\n' "$status" >> "$JIRA_FILE"
  printf '        "assignee": {"displayName": "%s", "emailAddress": "%s"},\n' \
    "${PEOPLE_NAMES[$assignee_idx]}" "${PEOPLE_EMAILS[$assignee_idx]}" >> "$JIRA_FILE"
  printf '        "priority": {"name": "%s"},\n' "$priority" >> "$JIRA_FILE"
  printf '        "project": {"key": "DEV", "name": "Product Development"}\n' >> "$JIRA_FILE"
  printf '      }\n' >> "$JIRA_FILE"
  if [[ "$is_last" == "true" ]]; then
    printf '    }\n' >> "$JIRA_FILE"
  else
    printf '    },\n' >> "$JIRA_FILE"
  fi
}

printf '{\n  "issues": [\n' > "$JIRA_FILE"

# ── Canonical epic records (first-class issues) ──────────────────────────────
# These are the issues a bot will look up when it sees "DEV-42" referenced
# elsewhere. They match their declared theme exactly.
emit_jira_issue "DEV-42" \
  "Onboarding Redesign (epic) — new user activation flow" \
  "Epic covering the onboarding redesign initiative. Ships welcome screen, profile setup, team invite, and progress tracking. Related: Linear ENG-101/103/106/113/115, GitHub PR #189, Figma abc123XYZ, Notion Q1 Roadmap." \
  "In Progress" 0 "High" "epic,sprint-12,onboarding-redesign" \
  "2026-03-08T14:00:00.000+0000" false

emit_jira_issue "DEV-50" \
  "API v2 Launch (epic) — URL-based versioning with Kong" \
  "Epic covering the API v2 launch. Kong gateway, rate limiting, backward-compat v1 proxy, OpenAPI spec. Related: Linear ENG-102/104/108/111, GitHub PR #191, Confluence 'API Gateway Architecture Decision Record'." \
  "In Progress" 1 "High" "epic,sprint-12,api-v2-launch" \
  "2026-03-09T10:30:00.000+0000" false

emit_jira_issue "DEV-55" \
  "Dark Mode (epic) — design phase" \
  "Epic covering dark mode implementation. Theme system with design tokens, chart palette, Figma component audit. Currently in design — engineering starts Sprint 13. Related: Linear ENG-105/109/114, GitHub PR #192, Figma Dark Mode Component Library." \
  "To Do" 3 "Medium" "epic,sprint-13,dark-mode" \
  "2026-03-07T16:00:00.000+0000" false

emit_jira_issue "DEV-34" \
  "Mobile Deep Link Fix (epic) — BLOCKER for onboarding" \
  "Epic covering the mobile deep link bug blocking onboarding on Android 14+. Intent filter fix, iOS universal links, web fallback. Blocks DEV-42 onboarding rollout. Related: Linear ENG-102 (flaky CI), GitHub PR #190, Gong calls with Acme Corp about mobile activation." \
  "In Review" 4 "High" "epic,sprint-12,mobile-deep-link-fix,blocker" \
  "2026-03-09T18:00:00.000+0000" false

# ── Canonical drama sub-tasks ────────────────────────────────────────────────
# A stale DEV-34 sub-task that hasn't moved in 7 days because Eve was OOO.
# This is the "bot should notice this!" signal for stale-ticket detection.
emit_jira_issue "DEV-56" \
  "Android 14 intent filter fix (STALE — Eve was OOO)" \
  "Part of epic DEV-34 (Mobile Deep Link Fix). Assigned to Eve Zhang. GlobalRetail escalated on gong-00007 — 40k users blocked. Eve was OOO Mar 5-8 per calendar. CI is flaky (Linear ENG-102). PR #190 merges EOD Friday regardless of CI." \
  "In Progress" 4 "High" "sprint-12,mobile-deep-link-fix,blocker,stale" \
  "2026-03-02T10:00:00.000+0000" false

# A conflict: DEV-42 (onboarding) and DEV-34 (mobile fix) interdep.
emit_jira_issue "DEV-57" \
  "Onboarding mobile rollout — blocked on DEV-34" \
  "Part of epic DEV-42 (Onboarding Redesign). Assigned to Alice Chen. Cannot ship onboarding mobile until DEV-34 deep link fix lands. See Confluence 'Sprint 12 Retro' for impact. Figma thread fc-canon-01 has latest." \
  "To Do" 0 "High" "sprint-12,onboarding-redesign,blocked-by-dev-34" \
  "2026-03-08T11:00:00.000+0000" false

# ── Sub-tasks ────────────────────────────────────────────────────────────────
# Issues with keys DEV-60..DEV-{60+N-1}. Skip canonical range (DEV-34/42/50/55)
# and drama sub-tasks (DEV-56/57) to avoid collisions.
subtask_count=$(( JIRA_COUNT - 6 ))
for ((i = 1; i <= subtask_count; i++)); do
  if   (( i % 4 == 0 )); then STATUS="Done"
  elif (( i % 3 == 0 )); then STATUS="In Review"
  elif (( i % 2 == 0 )); then STATUS="In Progress"
  else                         STATUS="To Do"
  fi

  epic=$(jira_story_epic "$i")
  theme=$(epic_theme_name "$epic")
  summary=$(jira_story_summary "$i")
  assignee_idx=$(( i % 5 ))
  day=$(( (i % 28) + 1 ))
  hr=$(( i % 24 ))
  mn=$(( i % 60 ))
  updated=$(printf "2026-03-%02dT%02d:%02d:00.000+0000" "$(( (day % 9) + 1 ))" "$hr" "$mn")

  priority="Normal"
  (( i % 7 == 0 )) && priority="High"
  (( i % 3 == 0 && i % 7 != 0 )) && priority="Medium"

  # Slug theme for label
  theme_label=$(echo "$theme" | tr '[:upper:]' '[:lower:]' | tr ' ' '-')
  description="Part of epic ${epic} (${theme}). Assigned to ${PEOPLE_NAMES[$assignee_idx]}."

  key=$(printf "DEV-%d" "$((i + 59))")  # start at DEV-60

  is_last="false"
  (( i == subtask_count )) && is_last="true"

  emit_jira_issue "$key" "$summary" "$description" "$STATUS" \
    "$assignee_idx" "$priority" "sprint-12,$theme_label" "$updated" "$is_last"
done

printf '  ]\n}\n' >> "$JIRA_FILE"
echo "Generated $JIRA_COUNT Jira issues (4 canonical epics + $subtask_count sub-tasks) -> $JIRA_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# GONG — Customer calls referencing product initiatives
# ═════════════════════════════════════════════════════════════════════════════
GONG_CUSTOMERS=("Acme Corp" "BigCo" "StartupXYZ" "Enterprise Co" "MidMarket Inc" "TechForward" "GlobalRetail" "HealthTech" "FinServe" "EduPlatform")
GONG_CONTACTS=("Jane Smith" "Tom Lee" "Mike Johnson" "Alex Tran" "Sara Kim" "Pat Chen" "Robin Hart" "Sam Costa" "Lee Park" "Jordan Reeves")
GONG_TOPICS_SET=(
  "onboarding,activation,mobile deep links"
  "dashboard,analytics,chart performance,dark mode"
  "renewal,pricing,onboarding ROI,API docs"
  "API architecture,security,PKCE,SSO"
  "onboarding,team management,admin dashboard"
  "API v2,rate limiting,versioning"
  "mobile,deep links,Android 14,password reset"
  "dark mode,theming,accessibility"
  "billing,upgrade path,enterprise features"
  "integration,webhooks,API access"
)
GONG_REFS=(
  "Jira DEV-42 (onboarding), DEV-34 (deep link bug), GitHub #235, Figma abc123XYZ"
  "Jira DEV-55 (dark mode), GitHub #228 (chart perf), #236"
  "Jira DEV-42 (onboarding), DEV-50 (API v2), Notion roadmap"
  "Linear ENG-101 (PKCE), GitHub PR #189, #191, Confluence tech specs"
  "Jira DEV-42 (onboarding), GitHub #235, Notion PRD"
  "Jira DEV-50 (API v2), Linear ENG-102, Confluence ADR"
  "Jira DEV-34, GitHub #234, PR #190"
  "Jira DEV-55, GitHub #236, Figma Dashboard Redesign"
  "Notion roadmap, Confluence pricing docs"
  "Jira DEV-50, GitHub #230, Confluence API docs"
)

printf '{\n  "events": [\n' > "$GONG_FILE"
for ((i = 1; i <= GONG_COUNT; i++)); do
  customer_idx=$(( (i - 1) % 10 ))
  owner_idx=$(( i % 5 ))
  topic_idx=$(( (i - 1) % 10 ))
  day=$(( (i % 28) + 1 ))
  hour=$(( i % 24 ))
  minute=$(( i % 60 ))

  if (( i % 6 == 0 )); then SENTIMENT="negative"; OUTCOME="at_risk"
  elif (( i % 4 == 0 )); then SENTIMENT="positive"; OUTCOME="won"
  elif (( i % 3 == 0 )); then SENTIMENT="neutral"; OUTCOME="stabilizing"
  else SENTIMENT="positive"; OUTCOME="expansion"; fi

  # Canonical-call overrides: these IDs have matching transcript bodies in
  # gong_transcript.json. Sentiment/outcome must agree with the transcript.
  case "$i" in
    1)  SENTIMENT="positive"; OUTCOME="expansion" ;;      # Acme Corp onboarding check-in
    2)  SENTIMENT="neutral";  OUTCOME="stabilizing" ;;    # BigCo dashboard perf + dark mode ask
    3)  SENTIMENT="negative"; OUTCOME="at_risk" ;;        # StartupXYZ renewal + API docs gap
    4)  SENTIMENT="positive"; OUTCOME="expansion" ;;      # Enterprise Co PKCE/SSO review
    5)  SENTIMENT="positive"; OUTCOME="expansion" ;;      # MidMarket Inc admin dashboard
    7)  SENTIMENT="negative"; OUTCOME="at_risk" ;;        # GlobalRetail deep-link escalation
    8)  SENTIMENT="positive"; OUTCOME="expansion" ;;      # HealthTech dark mode accessibility
  esac

  IFS=',' read -ra TOPICS <<< "${GONG_TOPICS_SET[$topic_idx]}"
  TOPICS_JSON="["
  for t_idx in "${!TOPICS[@]}"; do
    TOPICS_JSON+="\"${TOPICS[$t_idx]}\""
    if (( t_idx < ${#TOPICS[@]} - 1 )); then TOPICS_JSON+=","; fi
  done
  TOPICS_JSON+="]"

  if (( i % 6 == 0 )); then
    TIMESTAMP=$(printf "2025-12-%02dT%02d:%02d:00Z" "$(( (i % 28) + 1 ))" "$hour" "$minute")
  else
    TIMESTAMP=$(printf "2026-03-%02dT%02d:%02d:00Z" "$(( (day % 9) + 1 ))" "$hour" "$minute")
  fi

  printf '    {\n' >> "$GONG_FILE"
  printf '      "id": "gong-%05d",\n' "$i" >> "$GONG_FILE"
  printf '      "title": "Call with %s — %s",\n' "${GONG_CUSTOMERS[$customer_idx]}" "$(echo "${GONG_TOPICS_SET[$topic_idx]}" | cut -d',' -f1)" >> "$GONG_FILE"
  printf '      "scheduled": "%s",\n' "$TIMESTAMP" >> "$GONG_FILE"
  printf '      "duration": %d,\n' "$(( 900 + (i % 3) * 900 ))" >> "$GONG_FILE"
  printf '      "parties": [\n' >> "$GONG_FILE"
  printf '        {"name": "%s", "email": "%s", "role": "owner"},\n' "${PEOPLE_NAMES[$owner_idx]}" "${PEOPLE_EMAILS[$owner_idx]}" >> "$GONG_FILE"
  printf '        {"name": "%s", "email": "%s@%s.com", "role": "customer"}\n' "${GONG_CONTACTS[$customer_idx]}" "$(echo "${GONG_CONTACTS[$customer_idx]}" | tr '[:upper:]' '[:lower:]' | tr ' ' '.')" "$(echo "${GONG_CUSTOMERS[$customer_idx]}" | tr '[:upper:]' '[:lower:]' | tr -d ' ')" >> "$GONG_FILE"
  printf '      ],\n' >> "$GONG_FILE"
  printf '      "topics": %s,\n' "$TOPICS_JSON" >> "$GONG_FILE"
  printf '      "sentiment": "%s",\n' "$SENTIMENT" >> "$GONG_FILE"
  printf '      "outcome": "%s",\n' "$OUTCOME" >> "$GONG_FILE"
  printf '      "product_refs": "%s"\n' "${GONG_REFS[$topic_idx]}" >> "$GONG_FILE"

  if (( i == GONG_COUNT )); then printf '    }\n' >> "$GONG_FILE"
  else printf '    },\n' >> "$GONG_FILE"; fi
done
printf '  ]\n}\n' >> "$GONG_FILE"
echo "Generated $GONG_COUNT Gong events -> $GONG_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# LINEAR — Engineering issues referencing Jira epics and GitHub PRs
# ═════════════════════════════════════════════════════════════════════════════
LINEAR_STATES=("Backlog" "Todo" "In Progress" "In Review" "Done" "Cancelled")
LINEAR_STATE_TYPES=("backlog" "unstarted" "started" "started" "completed" "cancelled")

linear_title() {
  local idx=$1
  case $(( idx % 15 )) in
    0) echo "Migrate auth to OAuth 2.0 PKCE for onboarding (DEV-42, PR #189)";;
    1) echo "Add rate limiting to API v2 endpoints (DEV-50, PR #191)";;
    2) echo "Fix flaky CI tests blocking deep link PR #190 (DEV-34)";;
    3) echo "WebSocket reconnection for onboarding progress sync (DEV-42)";;
    4) echo "PgBouncer connection pooling for API v2 scale (DEV-50)";;
    5) echo "Dark mode theme system with design tokens (DEV-55, PR #192)";;
    6) echo "Onboarding analytics event tracking (DEV-42)";;
    7) echo "API v2 OpenAPI spec generation (DEV-50)";;
    8) echo "Mobile deep link intent filter for Android 14+ (DEV-34)";;
    9) echo "Chart rendering virtualization for dashboard (DEV-55, #228)";;
    10) echo "Onboarding profile step skip option (DEV-42, Figma abc123XYZ)";;
    11) echo "API v2 backward compatibility layer (DEV-50)";;
    12) echo "Deep link fallback web handler (DEV-34)";;
    13) echo "Dark mode Figma component audit (DEV-55)";;
    14) echo "Onboarding team invite email template (DEV-42)";;
  esac
}

linear_project() {
  local idx=$1
  case $(( idx % 15 )) in
    0|3|6|10|14) echo "Onboarding Redesign";;
    1|4|7|11) echo "API v2 Platform";;
    2|8|12) echo "Developer Experience";;
    5|9|13) echo "Dashboard v2";;
  esac
}

printf '{"data":{"issueSearch":{"nodes":[\n' > "$LINEAR_FILE"
for ((i = 1; i <= LINEAR_COUNT; i++)); do
  state_idx=$(( i % 6 ))
  assignee_idx=$(( i % 5 ))
  priority=$(( (i % 4) + 1 ))
  day=$(( (i % 28) + 1 ))
  title=$(linear_title "$i")
  project=$(linear_project "$i")

  assignee_json="null"
  if (( i % 8 != 0 )); then
    assignee_json=$(printf '{"name":"%s","email":"%s"}' "${PEOPLE_NAMES[$assignee_idx]}" "${PEOPLE_EMAILS[$assignee_idx]}")
  fi

  printf '{"id":"lin-%04d","identifier":"ENG-%d","title":"%s",' "$i" "$(( 100 + i ))" "$title" >> "$LINEAR_FILE"
  printf '"state":{"name":"%s","type":"%s"},' "${LINEAR_STATES[$state_idx]}" "${LINEAR_STATE_TYPES[$state_idx]}" >> "$LINEAR_FILE"
  printf '"priority":%d,"assignee":%s,' "$priority" "$assignee_json" >> "$LINEAR_FILE"
  printf '"project":{"name":"%s"},' "$project" >> "$LINEAR_FILE"
  printf '"createdAt":"2026-02-%02dT10:00:00Z",' "$day" >> "$LINEAR_FILE"
  printf '"updatedAt":"2026-03-%02dT14:00:00Z",' "$(( (day % 9) + 1 ))" >> "$LINEAR_FILE"
  printf '"labels":{"nodes":[{"name":"%s"}]}}' "$(echo "$project" | tr '[:upper:]' '[:lower:]' | tr ' ' '-')" >> "$LINEAR_FILE"

  if (( i < LINEAR_COUNT )); then printf ',\n' >> "$LINEAR_FILE"; else printf '\n' >> "$LINEAR_FILE"; fi
done
printf '],"pageInfo":{"hasNextPage":false}}}}\n' >> "$LINEAR_FILE"
echo "Generated $LINEAR_COUNT Linear issues -> $LINEAR_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# NOTION — Pages referencing Jira epics, Figma, and GitHub
# ═════════════════════════════════════════════════════════════════════════════
NOTION_PAGES=(
  "Q1 2026 Product Roadmap"
  "PRD: User Onboarding Redesign"
  "API v2 Launch Plan"
  "Sprint 12 Retro Notes"
  "Dark Mode Feature Brief"
  "Decision Log — March 2026"
  "Customer Feedback Summary — Q1"
  "Competitive Analysis 2026"
  "Team OKRs — Q1 2026"
  "Release Checklist — v2.1.0"
  "Onboarding Metrics Dashboard Spec"
  "API v2 Migration Guide"
  "Mobile App Release Notes"
  "Design Review Process"
  "Stakeholder Communication Plan"
)
NOTION_REFS=(
  "References: Jira DEV-42/50/55/34, Figma abc123XYZ, Confluence specs"
  "References: Jira DEV-42, Figma abc123XYZ, GitHub #235, PR #189"
  "References: Jira DEV-50, GitHub #230, PR #191, Confluence ADR, Linear ENG-102/105"
  "References: Sprint 12 work — DEV-42 onboarding, DEV-34 deep link bug, ENG-104 shipped"
  "References: Jira DEV-55, GitHub #236, PR #192, Figma Dashboard Redesign"
  "References: Confluence ADR (Kong), DEV-42 skip option, DEV-34 Android 14"
  "References: Gong calls — Acme Corp, BigCo, StartupXYZ, Enterprise prospect"
  "References: Customer feedback from Gong, product roadmap priorities"
  "References: Onboarding (DEV-42), API v2 (DEV-50), Dark mode (DEV-55)"
  "References: Blocked by DEV-34 deep link bug. PR #190 needs CI fix (ENG-103)"
  "References: Onboarding redesign DEV-42, analytics tracking in PR #189"
  "References: Jira DEV-50, Confluence ADR, GitHub PR #191"
  "References: DEV-34 deep link fix, Android 14 support"
  "References: Figma review process, onboarding review March 11"
  "References: Stakeholder update March 12, Gong customer insights"
)

printf '{"object":"list","results":[\n' > "$NOTION_FILE"
for ((i = 1; i <= NOTION_COUNT; i++)); do
  page_idx=$(( (i - 1) % ${#NOTION_PAGES[@]} ))
  day=$(( (i % 28) + 1 ))
  uuid=$(printf '%08x-%04x-4000-%04x-%012x' "$i" "$(( i * 7 ))" "$(( 0xa000 + i ))" "$(( i * 31 ))")

  printf '{"object":"page","id":"%s",' "$uuid" >> "$NOTION_FILE"
  printf '"created_time":"2026-02-%02dT10:00:00.000Z",' "$day" >> "$NOTION_FILE"
  printf '"last_edited_time":"2026-03-%02dT14:00:00.000Z",' "$(( (day % 9) + 1 ))" >> "$NOTION_FILE"
  printf '"parent":{"type":"workspace","workspace":true},' >> "$NOTION_FILE"
  printf '"properties":{"title":{"id":"title","type":"title","title":[{"type":"text","text":{"content":"%s"}}]},' "${NOTION_PAGES[$page_idx]}" >> "$NOTION_FILE"
  printf '"refs":{"rich_text":[{"text":{"content":"%s"}}]}},' "${NOTION_REFS[$page_idx]}" >> "$NOTION_FILE"
  printf '"url":"https://www.notion.so/page-%s"}' "$uuid" >> "$NOTION_FILE"

  if (( i < NOTION_COUNT )); then printf ',\n' >> "$NOTION_FILE"; else printf '\n' >> "$NOTION_FILE"; fi
done
printf '],"has_more":false,"type":"page_or_database"}\n' >> "$NOTION_FILE"
echo "Generated $NOTION_COUNT Notion pages -> $NOTION_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# CONFLUENCE — Technical docs referencing the same initiatives
# ═════════════════════════════════════════════════════════════════════════════
CONFLUENCE_PAGES=(
  "API Gateway Architecture Decision Record"
  "Onboarding Service Technical Spec"
  "Production Runbook — Incident Response"
  "Load Testing Results — API v2 Beta"
  "Q1 Sprint Ceremonies Schedule"
  "Mobile Deep Link Configuration Guide"
  "Security Hardening Checklist"
  "Database Migration Strategy — Onboarding Tables"
  "API v2 Rate Limiting Policy"
  "Dark Mode Implementation Guide"
  "WebSocket Infrastructure Spec"
  "On-Call Rotation Guide"
  "Monitoring and Alerting Configuration"
  "Code Review Standards"
  "Deployment Pipeline Documentation"
)
CONFLUENCE_REFS=(
  "Jira DEV-50, GitHub PR #191, Linear ENG-102. Kong gateway for API v2 routing."
  "Jira DEV-42, Notion PRD, Figma abc123XYZ, Linear ENG-101/104. PKCE auth + WebSocket progress."
  "Updated for DEV-34 Android deep link incident. GitHub #234, PR #190, Linear ENG-103."
  "API v2 endpoints (DEV-50). Confirms need for PgBouncer (ENG-105). Rate limiting under SLA."
  "Sprint 12 ends March 10. Design review for onboarding (Figma abc123XYZ) March 11."
  "DEV-34 Android 14 fix. Deep link verification, intent filters, asset links."
  "Pre-launch checklist for API v2 (DEV-50) and onboarding (DEV-42)."
  "New tables for onboarding redesign (DEV-42). Migration in PR #189."
  "API v2 rate limiting rules (Linear ENG-102). 429 response strategy."
  "Theme system for DEV-55. Design tokens from Figma Design System project."
  "Completed in ENG-104. Used by onboarding progress sync (DEV-42)."
  "Escalation paths for mobile incidents like DEV-34."
  "Dashboard alerts for API v2 endpoints and onboarding funnel."
  "Review guidelines for PRs #189, #190, #191, #192."
  "CI/CD pipeline. Flaky test issue ENG-103 blocking PR #190."
)
CONFLUENCE_SPACES=("ENG" "OPS" "SEC" "TEAM" "INFRA")

printf '{"results":[\n' > "$CONFLUENCE_FILE"
for ((i = 1; i <= CONFLUENCE_COUNT; i++)); do
  page_idx=$(( (i - 1) % ${#CONFLUENCE_PAGES[@]} ))
  space_idx=$(( i % ${#CONFLUENCE_SPACES[@]} ))
  day=$(( (i % 28) + 1 ))

  printf '{"id":"%d","type":"page","title":"%s",' "$(( 65540 + i ))" "${CONFLUENCE_PAGES[$page_idx]}" >> "$CONFLUENCE_FILE"
  printf '"status":"current","space":{"key":"%s","name":"%s"},' "${CONFLUENCE_SPACES[$space_idx]}" "${CONFLUENCE_SPACES[$space_idx]}" >> "$CONFLUENCE_FILE"
  printf '"_links":{"webui":"/spaces/%s/pages/%d"},' "${CONFLUENCE_SPACES[$space_idx]}" "$(( 65540 + i ))" >> "$CONFLUENCE_FILE"
  printf '"lastModified":{"when":"2026-03-%02dT10:00:00Z"},' "$(( (day % 9) + 1 ))" >> "$CONFLUENCE_FILE"
  printf '"excerpt":"%s"}' "${CONFLUENCE_REFS[$page_idx]}" >> "$CONFLUENCE_FILE"

  if (( i < CONFLUENCE_COUNT )); then printf ',\n' >> "$CONFLUENCE_FILE"; else printf '\n' >> "$CONFLUENCE_FILE"; fi
done
printf '],"totalSize":%d,"start":0,"limit":100,"size":%d}\n' "$CONFLUENCE_COUNT" "$CONFLUENCE_COUNT" >> "$CONFLUENCE_FILE"
echo "Generated $CONFLUENCE_COUNT Confluence pages -> $CONFLUENCE_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# GITHUB ISSUES — Tied to Jira epics, Linear issues, Figma
# ═════════════════════════════════════════════════════════════════════════════
GH_LABELS=("bug" "feature" "enhancement" "performance" "security" "api-v2" "onboarding" "mobile" "ux" "dashboard")

gh_issue_title() {
  local idx=$1
  case $(( idx % 15 )) in
    0) echo "Users unable to reset password via mobile deep link";;
    1) echo "Implement new user onboarding flow";;
    2) echo "Implement API v2 versioning strategy";;
    3) echo "Dashboard charts render slowly with >10k data points";;
    4) echo "Add dark mode toggle and theme system";;
    5) echo "Onboarding: skip option for profile setup step";;
    6) echo "API v2: rate limiting returns incorrect headers";;
    7) echo "Mobile: deep link fallback for unsupported OS versions";;
    8) echo "Onboarding: team invite email not sending";;
    9) echo "API v2: pagination cursor for list endpoints";;
    10) echo "Dashboard: chart tooltip positioning on mobile";;
    11) echo "Onboarding: analytics event tracking";;
    12) echo "API v2: webhook delivery retry logic";;
    13) echo "Dark mode: high contrast accessibility mode";;
    14) echo "Mobile: push notification for onboarding completion";;
  esac
}

gh_issue_body() {
  local idx=$1
  case $(( idx % 15 )) in
    0) echo "Blocking v2.1.0 release. Jira DEV-34, Linear ENG-103, PR #190. Customer reported in Gong call.";;
    1) echo "Main onboarding epic. Jira DEV-42, Figma abc123XYZ, Notion PRD, PR #189.";;
    2) echo "API v2 versioning. Jira DEV-50, Confluence ADR, Linear ENG-102, PR #191.";;
    3) echo "BigCo flagged in Gong demo call. Eve investigating. Related to dark mode #236.";;
    4) echo "Jira DEV-55, Linear ENG-110, Figma Dashboard Redesign. Depends on #228 perf fix.";;
    5) echo "From Figma feedback (alice.chen). Jira DEV-44, part of onboarding epic DEV-42.";;
    6) echo "Part of API v2 (DEV-50). Rate limiting headers per Confluence ADR.";;
    7) echo "Fallback for DEV-34 fix. Deep link to web when native not supported.";;
    8) echo "Part of onboarding (DEV-42, DEV-45). Email service integration needed.";;
    9) echo "API v2 (DEV-50). Cursor-based pagination for large result sets.";;
    10) echo "Dashboard v2 (DEV-55). Touch target sizing for mobile charts.";;
    11) echo "Onboarding (DEV-42). Track funnel metrics per Notion PRD requirements.";;
    12) echo "API v2 (DEV-50). Exponential backoff for webhook delivery.";;
    13) echo "Dark mode (DEV-55). WCAG AAA contrast ratios per Figma design system.";;
    14) echo "Onboarding (DEV-42). Notify users who started but did not complete.";;
  esac
}

printf '{"total_count":%d,"incomplete_results":false,"items":[\n' "$GITHUB_ISSUE_COUNT" > "$GITHUB_ISSUES_FILE"
for ((i = 1; i <= GITHUB_ISSUE_COUNT; i++)); do
  user_idx=$(( i % 5 ))
  assignee_idx=$(( (i + 1) % 5 ))
  label_idx=$(( i % 10 ))
  state="open"; if (( i % 4 == 0 )); then state="closed"; fi
  day=$(( (i % 28) + 1 ))
  title=$(gh_issue_title "$i")
  body=$(gh_issue_body "$i")

  printf '{"id":%d,"number":%d,"title":"%s",' "$(( 1000 + i ))" "$(( 220 + i ))" "$title" >> "$GITHUB_ISSUES_FILE"
  printf '"state":"%s","html_url":"https://github.com/acme/product/issues/%d",' "$state" "$(( 220 + i ))" >> "$GITHUB_ISSUES_FILE"
  printf '"user":{"login":"%s"},' "${PEOPLE_GH[$user_idx]}" >> "$GITHUB_ISSUES_FILE"
  printf '"labels":[{"name":"%s"}],' "${GH_LABELS[$label_idx]}" >> "$GITHUB_ISSUES_FILE"
  printf '"assignees":[{"login":"%s"}],' "${PEOPLE_GH[$assignee_idx]}" >> "$GITHUB_ISSUES_FILE"
  printf '"created_at":"2026-02-%02dT10:00:00Z","updated_at":"2026-03-%02dT15:00:00Z",' "$day" "$(( (day % 9) + 1 ))" >> "$GITHUB_ISSUES_FILE"
  printf '"comments":%d,"body":"%s"}' "$(( i % 10 ))" "$body" >> "$GITHUB_ISSUES_FILE"

  if (( i < GITHUB_ISSUE_COUNT )); then printf ',\n' >> "$GITHUB_ISSUES_FILE"; else printf '\n' >> "$GITHUB_ISSUES_FILE"; fi
done
printf ']}\n' >> "$GITHUB_ISSUES_FILE"
echo "Generated $GITHUB_ISSUE_COUNT GitHub issues -> $GITHUB_ISSUES_FILE"

# ── GitHub PRs ───────────────────────────────────────────────────────────────
GH_PR_TITLES=(
  "feat: implement new user onboarding flow (DEV-42)"
  "fix: resolve deep link handling on Android 14+ (DEV-34)"
  "feat: API v2 versioning with Kong gateway (DEV-50)"
  "feat: add dark mode toggle component (DEV-55)"
  "fix: flaky CI integration tests (ENG-103)"
  "feat: onboarding analytics tracking (DEV-42)"
  "feat: API v2 rate limiting middleware (ENG-102)"
  "fix: chart rendering perf with large datasets (#228)"
  "feat: onboarding team invite flow (DEV-42, DEV-45)"
  "feat: API v2 backward compatibility proxy (DEV-50)"
)
GH_PR_BRANCHES=("feat/onboarding-v3" "fix/android-deeplinks" "feat/api-v2-versioning" "feat/dark-mode" "fix/flaky-ci" "feat/onboarding-analytics" "feat/api-v2-rate-limit" "fix/chart-perf" "feat/onboarding-invite" "feat/api-v2-compat")

printf '[\n' > "$GITHUB_PRS_FILE"
for ((i = 1; i <= GITHUB_PR_COUNT; i++)); do
  pr_idx=$(( (i - 1) % ${#GH_PR_TITLES[@]} ))
  user_idx=$(( i % 5 ))
  reviewer_idx=$(( (i + 2) % 5 ))
  state="open"; if (( i % 3 == 0 )); then state="closed"; fi
  day=$(( (i % 28) + 1 ))
  adds=$(( (i * 37) % 500 + 10 ))
  dels=$(( (i * 13) % 200 + 5 ))

  printf '{"id":%d,"number":%d,"title":"%s",' "$(( 2000 + i ))" "$(( 185 + i ))" "${GH_PR_TITLES[$pr_idx]}" >> "$GITHUB_PRS_FILE"
  printf '"state":"%s","html_url":"https://github.com/acme/product/pull/%d",' "$state" "$(( 185 + i ))" >> "$GITHUB_PRS_FILE"
  printf '"user":{"login":"%s"},' "${PEOPLE_GH[$user_idx]}" >> "$GITHUB_PRS_FILE"
  printf '"head":{"ref":"%s","sha":"sha%04x"},"base":{"ref":"main"},' "${GH_PR_BRANCHES[$pr_idx]}" "$i" >> "$GITHUB_PRS_FILE"
  printf '"draft":false,"mergeable":true,' >> "$GITHUB_PRS_FILE"
  printf '"requested_reviewers":[{"login":"%s"}],' "${PEOPLE_GH[$reviewer_idx]}" >> "$GITHUB_PRS_FILE"
  printf '"labels":[{"name":"%s"}],' "${GH_LABELS[$(( i % 10 ))]}" >> "$GITHUB_PRS_FILE"
  printf '"created_at":"2026-02-%02dT10:00:00Z","updated_at":"2026-03-%02dT14:00:00Z",' "$day" "$(( (day % 9) + 1 ))" >> "$GITHUB_PRS_FILE"
  printf '"additions":%d,"deletions":%d,"changed_files":%d}' "$adds" "$dels" "$(( (i % 15) + 1 ))" >> "$GITHUB_PRS_FILE"

  if (( i < GITHUB_PR_COUNT )); then printf ',\n' >> "$GITHUB_PRS_FILE"; else printf '\n' >> "$GITHUB_PRS_FILE"; fi
done
printf ']\n' >> "$GITHUB_PRS_FILE"
echo "Generated $GITHUB_PR_COUNT GitHub PRs -> $GITHUB_PRS_FILE"

# ── GitHub Actions runs tied to PR branches ──────────────────────────────────
GH_RUN_COUNT=30
CONCLUSIONS=("success" "failure" "success" "success" "success")
printf '{"total_count":%d,"workflow_runs":[\n' "$GH_RUN_COUNT" > "$GITHUB_RUNS_FILE"
for ((i = 1; i <= GH_RUN_COUNT; i++)); do
  branch_idx=$(( (i - 1) % ${#GH_PR_BRANCHES[@]} ))
  user_idx=$(( i % 5 ))
  conclusion="${CONCLUSIONS[$(( i % 5 ))]}"
  # Make the deep link branch always fail (simulates ENG-103 flaky tests)
  if [[ "${GH_PR_BRANCHES[$branch_idx]}" == "fix/android-deeplinks" ]]; then conclusion="failure"; fi
  day=$(( (i % 9) + 1 ))
  hour=$(( i % 24 ))

  printf '{"id":%d,"name":"CI","head_branch":"%s","head_sha":"sha%04x",' "$(( 3000 + i ))" "${GH_PR_BRANCHES[$branch_idx]}" "$i" >> "$GITHUB_RUNS_FILE"
  printf '"status":"completed","conclusion":"%s",' "$conclusion" >> "$GITHUB_RUNS_FILE"
  printf '"html_url":"https://github.com/acme/product/actions/runs/%d",' "$(( 3000 + i ))" >> "$GITHUB_RUNS_FILE"
  printf '"created_at":"2026-03-%02dT%02d:00:00Z","updated_at":"2026-03-%02dT%02d:08:00Z",' "$day" "$hour" "$day" "$hour" >> "$GITHUB_RUNS_FILE"
  printf '"run_attempt":1,"event":"push","actor":{"login":"%s"}}' "${PEOPLE_GH[$user_idx]}" >> "$GITHUB_RUNS_FILE"

  if (( i < GH_RUN_COUNT )); then printf ',\n' >> "$GITHUB_RUNS_FILE"; else printf '\n' >> "$GITHUB_RUNS_FILE"; fi
done
printf ']}\n' >> "$GITHUB_RUNS_FILE"
echo "Generated $GH_RUN_COUNT GitHub Actions runs -> $GITHUB_RUNS_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# FIGMA — Design comments referencing Jira, PRs, and other tools
# ═════════════════════════════════════════════════════════════════════════════
FIGMA_COMMENTS_TEXT=(
  "Welcome screen illustration too busy — customer feedback from Gong says onboarding feels overwhelming (DEV-42)"
  "Updated to minimal version per design system. Faster mobile load per Confluence tech spec."
  "Profile setup needs skip option — Notion PRD says name+email required, rest optional (DEV-44)"
  "Added SkipLink component. Updated Jira DEV-44 description. PR #189 will implement."
  "ProgressBar needs design tokens — hardcoded colors will break dark mode (DEV-55, #236)"
  "Mobile variant page needs update after deep link fix (DEV-34, PR #190) lands"
  "Confirmed: after PR #190, users enter at Welcome Screen. Test in March 11 design review."
  "Team invite step should match the email template from Confluence (DEV-45)"
  "Completion screen should show dashboard preview — connects to chart perf work (#228)"
  "Design token colors exported. Eve can use these in dark mode PR #192."
)
FIGMA_COMMENTERS=("bob.park" "diana.wu" "alice.chen" "diana.wu" "eve.zhang" "bob.park" "carlos.rivera" "diana.wu" "bob.park" "diana.wu")

printf '{"comments":[\n' > "$FIGMA_COMMENTS_FILE"

# ── Canonical drama thread: DEV-34 escalation visible in Figma ────────────
cat >> "$FIGMA_COMMENTS_FILE" <<'FIGMA_DRAMA'
{"id":"fc-canon-01","message":"Mobile variant blocked — DEV-34 still not merged. GlobalRetail just escalated on Gong (gong-00007). Need this fix this week or we can't ship onboarding v3 mobile.","file_key":"abc123XYZ","parent_id":"","user":{"handle":"carlos.rivera","img_url":""},"created_at":"2026-03-09T21:30:00Z","resolved_at":null,"order_id":900},
{"id":"fc-canon-02","message":"@eve — back from OOO, what's the status on PR #190? CI still failing? Customer is asking for daily updates.","file_key":"abc123XYZ","parent_id":"fc-canon-01","user":{"handle":"alice.chen","img_url":""},"created_at":"2026-03-10T08:15:00Z","resolved_at":null,"order_id":901},
{"id":"fc-canon-03","message":"PR #190 merging EOD Friday regardless of CI — Linear ENG-102 tracks the flaky test separately. Will confirm customer-facing fix works Monday.","file_key":"abc123XYZ","parent_id":"fc-canon-01","user":{"handle":"eve.zhang","img_url":""},"created_at":"2026-03-10T14:45:00Z","resolved_at":null,"order_id":902},
FIGMA_DRAMA

for ((i = 1; i <= FIGMA_COMMENT_COUNT; i++)); do
  comment_idx=$(( (i - 1) % ${#FIGMA_COMMENTS_TEXT[@]} ))
  day=$(( (i % 9) + 1 ))
  hour=$(( i % 24 ))
  resolved="null"
  if (( comment_idx == 1 || comment_idx == 3 )); then
    resolved=$(printf '"2026-03-%02dT%02d:30:00Z"' "$day" "$hour")
  fi

  printf '{"id":"fc%04d","message":"%s",' "$i" "${FIGMA_COMMENTS_TEXT[$comment_idx]}" >> "$FIGMA_COMMENTS_FILE"
  printf '"file_key":"abc123XYZ","parent_id":"%s",' "$(if (( comment_idx % 2 == 1 )); then printf 'fc%04d' "$(( i - 1 ))"; fi)" >> "$FIGMA_COMMENTS_FILE"
  printf '"user":{"handle":"%s","img_url":""},' "${FIGMA_COMMENTERS[$comment_idx]}" >> "$FIGMA_COMMENTS_FILE"
  printf '"created_at":"2026-03-%02dT%02d:00:00Z",' "$day" "$hour" >> "$FIGMA_COMMENTS_FILE"
  printf '"resolved_at":%s,"order_id":%d}' "$resolved" "$i" >> "$FIGMA_COMMENTS_FILE"

  if (( i < FIGMA_COMMENT_COUNT )); then printf ',\n' >> "$FIGMA_COMMENTS_FILE"; else printf '\n' >> "$FIGMA_COMMENTS_FILE"; fi
done
printf ']}\n' >> "$FIGMA_COMMENTS_FILE"
echo "Generated $FIGMA_COMMENT_COUNT Figma comments -> $FIGMA_COMMENTS_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# GOOGLE CALENDAR — Meetings about the product initiatives
# ═════════════════════════════════════════════════════════════════════════════
MEETING_TITLES=(
  "Sprint 13 Planning"
  "1:1 — Bob / Alice (onboarding status)"
  "Design Review — Onboarding v3 (Figma abc123XYZ)"
  "Stakeholder Update — Q1 Progress"
  "Sprint 12 Retro"
  "Architecture Review — API v2 Rate Limiting"
  "Deep Link Bug Triage (DEV-34)"
  "Team Standup"
  "Customer Call Prep — Acme Corp"
  "Dark Mode Design Kickoff"
)
MEETING_DESCS=(
  "Plan Sprint 13. Review: onboarding (DEV-42), deep link fix (DEV-34), API v2 (DEV-50), dark mode (DEV-55)."
  "Onboarding PR #189 status, ENG-101 auth migration, Acme Corp Gong feedback."
  "Review Figma abc123XYZ designs. Sign-off needed for PR #189. Agenda: welcome, skip option, progress bar, mobile."
  "Q1 progress for leadership. Onboarding 70%, API v2 on track, v2.1.0 delayed by DEV-34."
  "Sprint 12 retro. Wins: ENG-104 shipped. Issues: DEV-34 slipped, flaky CI. Notes in Notion."
  "Review Carlos PR #191 rate limiting. Confluence ADR reference. Linear ENG-102."
  "Triage deep link bug DEV-34. CI blocked (ENG-103). Blocking v2.1.0 and onboarding mobile."
  "Daily standup. Check progress across DEV-42, DEV-50, DEV-34, DEV-55."
  "Prep for Acme Corp follow-up. Review Gong call notes, onboarding timeline, deep link fix ETA."
  "Kick off dark mode design with Diana. Figma Dashboard Redesign file. DEV-55, Linear ENG-110."
)

printf '{"kind":"calendar#events","summary":"bob@acme.io","timeZone":"America/New_York","items":[\n' > "$GCAL_FILE"

# ── Canonical drama events (first, before the bulk loop) ──────────────────
# Eve was OOO Mar 5-8, which is why the deep-link fix (DEV-34) stalled.
cat >> "$GCAL_FILE" <<'CAL_DRAMA'
{"id":"gcal-canon-01","status":"confirmed","summary":"OOO — Eve Zhang","description":"Eve is out Mar 5-8. Deep link fix (DEV-34) owner.","start":{"date":"2026-03-05"},"end":{"date":"2026-03-09"},"attendees":[{"email":"eve@acme.io","displayName":"Eve Zhang","responseStatus":"accepted"}]},
{"id":"gcal-canon-02","status":"confirmed","summary":"ESCALATION — GlobalRetail deep link (DEV-34)","description":"Robin Hart (GlobalRetail) escalated DEV-34 on Gong call gong-00007. 40k store employees blocked on Android 14 password reset. Threatening contract pause. VP escalation if fix doesn't ship this week.","start":{"dateTime":"2026-03-09T21:00:00-05:00"},"end":{"dateTime":"2026-03-09T22:00:00-05:00"},"attendees":[{"email":"carlos@acme.io","displayName":"Carlos Rivera","responseStatus":"accepted"},{"email":"eve@acme.io","displayName":"Eve Zhang","responseStatus":"accepted"},{"email":"alice@acme.io","displayName":"Alice Chen","responseStatus":"tentative"}]},
{"id":"gcal-canon-03","status":"confirmed","summary":"Daily update — GlobalRetail DEV-34 fix status","description":"Per commitment to Robin Hart: daily status on deep link fix until ship + post-mortem in 2 weeks.","start":{"dateTime":"2026-03-10T09:00:00-05:00"},"end":{"dateTime":"2026-03-10T09:15:00-05:00"},"attendees":[{"email":"carlos@acme.io","displayName":"Carlos Rivera","responseStatus":"accepted"}],"recurrence":["RRULE:FREQ=DAILY;UNTIL=20260315T000000Z"]},
CAL_DRAMA

for ((i = 1; i <= GCAL_COUNT; i++)); do
  title_idx=$(( (i - 1) % ${#MEETING_TITLES[@]} ))
  day=$(( 10 + (i - 1) / 4 ))
  if (( day > 28 )); then day=28; fi
  hour=$(( 9 + (i % 8) ))
  att1_idx=$(( i % 5 ))
  att2_idx=$(( (i + 1) % 5 ))

  printf '{"id":"gcal%04d","status":"confirmed",' "$i" >> "$GCAL_FILE"
  printf '"summary":"%s",' "${MEETING_TITLES[$title_idx]}" >> "$GCAL_FILE"
  printf '"description":"%s",' "${MEETING_DESCS[$title_idx]}" >> "$GCAL_FILE"
  printf '"start":{"dateTime":"2026-03-%02dT%02d:00:00-05:00"},' "$day" "$hour" >> "$GCAL_FILE"
  printf '"end":{"dateTime":"2026-03-%02dT%02d:00:00-05:00"},' "$day" "$(( hour + 1 ))" >> "$GCAL_FILE"
  printf '"attendees":[{"email":"%s","displayName":"%s","responseStatus":"accepted"},' "${PEOPLE_EMAILS[$att1_idx]}" "${PEOPLE_NAMES[$att1_idx]}" >> "$GCAL_FILE"
  printf '{"email":"%s","displayName":"%s","responseStatus":"accepted"}]}' "${PEOPLE_EMAILS[$att2_idx]}" "${PEOPLE_NAMES[$att2_idx]}" >> "$GCAL_FILE"

  if (( i < GCAL_COUNT )); then printf ',\n' >> "$GCAL_FILE"; else printf '\n' >> "$GCAL_FILE"; fi
done
printf ']}\n' >> "$GCAL_FILE"
echo "Generated $GCAL_COUNT Google Calendar events (+3 drama events) -> $GCAL_FILE"

# ═════════════════════════════════════════════════════════════════════════════
# GMAIL — Emails about the same product work
# ═════════════════════════════════════════════════════════════════════════════
EMAIL_SUBJECTS=(
  "Weekly Status — Sprint 12"
  "Re: Onboarding Redesign — Design Review Prep"
  "Action Required: Deep Link Fix Blocker (DEV-34)"
  "FYI: Customer Feedback from Gong — Acme Corp at-risk"
  "Re: API v2 Architecture Review — Confluence ADR"
  "Sprint 13 Planning Agenda"
  "Re: Figma Review Comments — Onboarding v3"
  "Incident: Android 14 Deep Link Regression"
  "Re: Dark Mode Feature Brief — Notion"
  "Stakeholder Update Prep — Q1 Progress"
)

printf '{"messages":[\n' > "$GMAIL_LIST_FILE"
for ((i = 1; i <= GMAIL_COUNT; i++)); do
  printf '{"id":"msg%04d","threadId":"thr%04d"}' "$i" "$(( ((i - 1) % 10) + 1 ))" >> "$GMAIL_LIST_FILE"
  if (( i < GMAIL_COUNT )); then printf ',\n' >> "$GMAIL_LIST_FILE"; else printf '\n' >> "$GMAIL_LIST_FILE"; fi
done
printf '],"nextPageToken":null,"resultSizeEstimate":%d}\n' "$GMAIL_COUNT" >> "$GMAIL_LIST_FILE"
echo "Generated $GMAIL_COUNT Gmail messages -> $GMAIL_LIST_FILE"

# Single message detail — the weekly status email that ties everything together
printf '{"id":"msg0001","threadId":"thr0001","labelIds":["INBOX","UNREAD"],' > "$GMAIL_MSG_FILE"
printf '"snippet":"Weekly status: Onboarding 70%% complete, API v2 PR ready, deep link bug blocking v2.1.0...",' >> "$GMAIL_MSG_FILE"
printf '"payload":{"mimeType":"text/plain","headers":[' >> "$GMAIL_MSG_FILE"
printf '{"name":"From","value":"Bob Park <bob@acme.io>"},' >> "$GMAIL_MSG_FILE"
printf '{"name":"To","value":"product-team@acme.io"},' >> "$GMAIL_MSG_FILE"
printf '{"name":"Subject","value":"Weekly Status — Sprint 12 (March 9)"},' >> "$GMAIL_MSG_FILE"
printf '{"name":"Date","value":"Mon, 09 Mar 2026 09:00:00 -0500"}],' >> "$GMAIL_MSG_FILE"
printf '"body":{"size":1024,"data":"V2Vla2x5IFN0YXR1cyAtIFNwcmludCAxMgoKMS4gT25ib2FyZGluZyBSZWRlc2lnbiAoREVWLTQyKTogNzAlIGNvbXBsZXRlCiAgIC0gUFIgIzE4OSB1cCBmb3IgcmV2aWV3IChBbGljZSkKICAgLSBGaWdtYSBkZXNpZ25zIHVwZGF0ZWQgKGFiYzEyM1hZWikKICAgLSBEZXNpZ24gcmV2aWV3IE1hcmNoIDExCiAgIC0gQmxvY2tlZDogTW9iaWxlIG5lZWRzIGRlZXAgbGluayBmaXggKERFVi0zNCkKCjIuIEFQSSB2MiAoREVWLTUwKTogT24gdHJhY2sKICAgLSBQUiAjMTkxIHJlYWR5IChDYXJsb3MpCiAgIC0gQXJjaCByZXZpZXcgTWFyY2ggMTEKCjMuIERlZXAgTGluayBCdWcgKERFVi0zNCk6IEJMT0NLRUQKICAgLSBQUiAjMTkwIENJIGZhaWxpbmcgKEVORy0xMDMgZmxha3kgdGVzdHMpCiAgIC0gQmxvY2tpbmcgdjIuMS4wIHJlbGVhc2UKICAgLSBBY21lIENvcnAgcmVwb3J0ZWQgaW4gR29uZyBjYWxsCgo0LiBEYXJrIE1vZGUgKERFVi01NSk6IERlc2lnbiBwaGFzZQogICAtIERpYW5hIGluIEZpZ21hLCBFdmUgZHJhZnQgUFIgIzE5Mg=="}},' >> "$GMAIL_MSG_FILE"
printf '"sizeEstimate":2048,"internalDate":"1741522800000"}\n' >> "$GMAIL_MSG_FILE"
echo "Generated Gmail message detail -> $GMAIL_MSG_FILE"

echo ""
echo "=== All synthetic data generated ==="
echo "Story: Acme product team, Sprint 12. Initiatives: Onboarding (DEV-42), API v2 (DEV-50), Deep Link Bug (DEV-34), Dark Mode (DEV-55)"
echo "All integrations cross-reference the same people, tickets, PRs, designs, and meetings."
