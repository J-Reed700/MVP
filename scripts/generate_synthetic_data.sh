#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JIRA_COUNT="${JIRA_SYNTH_COUNT:-1000}"
GONG_COUNT="${GONG_SYNTH_COUNT:-${PENDO_SYNTH_COUNT:-5000}}"

JIRA_FILE="$ROOT_DIR/testdata/jira/wiremock/__files/jira_search_response.json"
GONG_FILE="$ROOT_DIR/testdata/gong/wiremock/__files/gong_events.json"

mkdir -p "$(dirname "$JIRA_FILE")" "$(dirname "$GONG_FILE")"

printf '{\n  "issues": [\n' > "$JIRA_FILE"
for ((i = 1; i <= JIRA_COUNT; i++)); do
  if (( i % 3 == 0 )); then
    STATUS="Done"
  elif (( i % 2 == 0 )); then
    STATUS="In Progress"
  else
    STATUS="To Do"
  fi

  printf '    {\n' >> "$JIRA_FILE"
  printf '      "key": "MVP-%d",\n' "$i" >> "$JIRA_FILE"
  printf '      "fields": {\n' >> "$JIRA_FILE"
  printf '        "summary": "Synthetic Jira decision candidate %d",\n' "$i" >> "$JIRA_FILE"
  printf '        "description": "Synthetic issue %d generated for MVP integration testing.",\n' "$i" >> "$JIRA_FILE"
  printf '        "labels": ["synthetic", "batch", "iteration-%d"],\n' "$(( (i % 20) + 1 ))" >> "$JIRA_FILE"
  if (( i % 5 == 0 )); then
    printf '        "updated": "2025-12-%02dT%02d:%02d:00.000+0000",\n' "$(( (i % 28) + 1 ))" "$(( i % 24 ))" "$(( i % 60 ))" >> "$JIRA_FILE"
  else
    printf '        "updated": "2026-02-%02dT%02d:%02d:00.000+0000",\n' "$(( (i % 22) + 1 ))" "$(( i % 24 ))" "$(( i % 60 ))" >> "$JIRA_FILE"
  fi
  printf '        "status": {"name": "%s"},\n' "$STATUS" >> "$JIRA_FILE"
  printf '        "project": {"key": "MVP", "name": "TruthGraph MVP"}\n' >> "$JIRA_FILE"
  printf '      }\n' >> "$JIRA_FILE"

  if (( i == JIRA_COUNT )); then
    printf '    }\n' >> "$JIRA_FILE"
  else
    printf '    },\n' >> "$JIRA_FILE"
  fi
done
printf '  ]\n}\n' >> "$JIRA_FILE"

set_story_fields() {
  local trajectory="$1"
  local phase="$2"

  case "${trajectory}:${phase}" in
    recovery:0)
      CALL_TITLE="Onboarding adoption risk discovery"
      SENTIMENT="negative"
      OUTCOME="at_risk"
      TOPICS='["onboarding","activation","renewal"]'
      RISKS='["adoption_risk","renewal_risk"]'
      NEXT_STEPS='["Launch activation recovery sprint","Assign implementation owner","Review usage weekly"]'
      TRANSCRIPT_EXCERPT="Customer said onboarding is slow and activation dropped after rollout."
      TRANSCRIPT_JSON='[{"speaker":"CSM","text":"We saw activation decline after the rollout."},{"speaker":"Customer","text":"Onboarding is blocked and renewal confidence is low."}]'
      TALK_RATIO="0.68"
      NPS_LINE='      "npsScore": 4,'
      LIFECYCLE="onboarding"
      ;;
    recovery:1)
      CALL_TITLE="Executive renewal risk checkpoint"
      SENTIMENT="negative"
      OUTCOME="at_risk"
      TOPICS='["renewal","executive_escalation","value"]'
      RISKS='["renewal_risk","exec_escalation"]'
      NEXT_STEPS='["Publish executive recovery plan","Schedule sponsor checkpoint","Lock weekly risk review"]'
      TRANSCRIPT_EXCERPT="Executive sponsor requested milestones before renewal committee review."
      TRANSCRIPT_JSON='[{"speaker":"CSM","text":"We can provide a 30 day remediation plan."},{"speaker":"Executive Sponsor","text":"We need signed milestones before committee review."}]'
      TALK_RATIO="0.64"
      NPS_LINE=''
      LIFECYCLE="renewal"
      ;;
    recovery:2)
      CALL_TITLE="Mitigation plan and security controls review"
      SENTIMENT="neutral"
      OUTCOME="blocked"
      TOPICS='["security","compliance","mitigation"]'
      RISKS='["security_blocker","compliance_gap"]'
      NEXT_STEPS='["Deliver control mapping","Complete certificate review","Confirm go-live checklist"]'
      TRANSCRIPT_EXCERPT="Security team asked for updated control evidence before unblocking go-live."
      TRANSCRIPT_JSON='[{"speaker":"Security Lead","text":"We need updated control evidence before sign-off."},{"speaker":"CSM","text":"We will deliver the package before Friday."}]'
      TALK_RATIO="0.57"
      NPS_LINE=''
      LIFECYCLE="implementation"
      ;;
    recovery:3)
      CALL_TITLE="Adoption rebound validation"
      SENTIMENT="neutral"
      OUTCOME="stabilizing"
      TOPICS='["adoption","enablement","health_score"]'
      RISKS='["adoption_watch"]'
      NEXT_STEPS='["Scale admin enablement","Measure weekly active teams","Track workflow completion"]'
      TRANSCRIPT_EXCERPT="Customer confirmed adoption is improving but requested closer monitoring."
      TRANSCRIPT_JSON='[{"speaker":"Customer Admin","text":"Usage is improving across key teams."},{"speaker":"CSM","text":"We will keep weekly check-ins through renewal."}]'
      TALK_RATIO="0.52"
      NPS_LINE='      "npsScore": 7,'
      LIFECYCLE="adoption"
      ;;
    recovery:4)
      CALL_TITLE="Renewal committee readiness"
      SENTIMENT="positive"
      OUTCOME="won"
      TOPICS='["renewal","governance","success_plan"]'
      RISKS='[]'
      NEXT_STEPS='["Finalize renewal paperwork","Capture success metrics","Transition to growth plan"]'
      TRANSCRIPT_EXCERPT="Procurement confirmed the renewal is on track after recovery milestones were met."
      TRANSCRIPT_JSON='[{"speaker":"Procurement","text":"The renewal is moving forward with current terms."},{"speaker":"CSM","text":"We will send final documentation today."}]'
      TALK_RATIO="0.46"
      NPS_LINE='      "npsScore": 9,'
      LIFECYCLE="renewal"
      ;;
    recovery:5)
      CALL_TITLE="Expansion planning workshop"
      SENTIMENT="positive"
      OUTCOME="expansion"
      TOPICS='["expansion","advocacy","multi_team_rollout"]'
      RISKS='[]'
      NEXT_STEPS='["Draft expansion proposal","Plan phase-two rollout","Nominate customer advocate"]'
      TRANSCRIPT_EXCERPT="Customer requested expansion pricing and agreed to serve as a reference account."
      TRANSCRIPT_JSON='[{"speaker":"Executive Sponsor","text":"We want to expand to additional teams."},{"speaker":"CSM","text":"We will propose a phased expansion package."}]'
      TALK_RATIO="0.44"
      NPS_LINE='      "npsScore": 10,'
      LIFECYCLE="expansion"
      ;;
    stalled:0)
      CALL_TITLE="Compliance blocker escalation"
      SENTIMENT="neutral"
      OUTCOME="blocked"
      TOPICS='["compliance","audit","security"]'
      RISKS='["compliance_gap","security_blocker"]'
      NEXT_STEPS='["Submit audit evidence","Align legal controls","Set compliance war-room"]'
      TRANSCRIPT_EXCERPT="Legal and security teams blocked progress pending audit evidence."
      TRANSCRIPT_JSON='[{"speaker":"Legal","text":"We cannot proceed without complete audit evidence."},{"speaker":"CSM","text":"We will coordinate with security on missing controls."}]'
      TALK_RATIO="0.61"
      NPS_LINE=''
      LIFECYCLE="implementation"
      ;;
    stalled:1)
      CALL_TITLE="Incident and reliability review"
      SENTIMENT="negative"
      OUTCOME="escalated"
      TOPICS='["stability","incident","sla"]'
      RISKS='["stability_risk","sla_breach"]'
      NEXT_STEPS='["Open incident command cadence","Publish ETA updates","Track sev2 trend"]'
      TRANSCRIPT_EXCERPT="Customer reported repeated latency incidents and requested leadership escalation."
      TRANSCRIPT_JSON='[{"speaker":"Customer","text":"We have seen repeated latency incidents this month."},{"speaker":"CSM","text":"We are escalating this to platform leadership today."}]'
      TALK_RATIO="0.66"
      NPS_LINE=''
      LIFECYCLE="adoption"
      ;;
    stalled:2)
      CALL_TITLE="Budget and procurement delay"
      SENTIMENT="neutral"
      OUTCOME="blocked"
      TOPICS='["budget","procurement","renewal"]'
      RISKS='["budget_risk","procurement_delay"]'
      NEXT_STEPS='["Offer phased commercial plan","Align procurement checklist","Confirm legal timeline"]'
      TRANSCRIPT_EXCERPT="Finance requested phased pricing due to budget constraints this quarter."
      TRANSCRIPT_JSON='[{"speaker":"Finance","text":"Budget constraints require a phased commercial structure."},{"speaker":"CSM","text":"We can provide a staged pricing option this week."}]'
      TALK_RATIO="0.58"
      NPS_LINE=''
      LIFECYCLE="renewal"
      ;;
    stalled:3)
      CALL_TITLE="Support backlog and response-time escalation"
      SENTIMENT="negative"
      OUTCOME="escalated"
      TOPICS='["support","escalation","response_time"]'
      RISKS='["support_backlog","response_time_risk"]'
      NEXT_STEPS='["Assign named escalation owner","Clear top backlog tickets","Publish daily status"]'
      TRANSCRIPT_EXCERPT="Customer escalated unresolved tickets and asked for executive visibility."
      TRANSCRIPT_JSON='[{"speaker":"Customer","text":"Open support tickets are blocking our rollout."},{"speaker":"CSM","text":"We will assign an escalation owner and send daily updates."}]'
      TALK_RATIO="0.63"
      NPS_LINE=''
      LIFECYCLE="adoption"
      ;;
    stalled:4)
      CALL_TITLE="Sentiment drop and churn warning"
      SENTIMENT="negative"
      OUTCOME="at_risk"
      TOPICS='["sentiment","renewal","competition"]'
      RISKS='["sentiment_risk","competitive_risk","renewal_risk"]'
      NEXT_STEPS='["Run executive value review","Deliver competitive ROI brief","Schedule churn-risk checkpoint"]'
      TRANSCRIPT_EXCERPT="Champion mentioned competitor evaluation and low confidence in renewal outcomes."
      TRANSCRIPT_JSON='[{"speaker":"Champion","text":"We are evaluating alternatives due to unresolved issues."},{"speaker":"CSM","text":"We will run an executive value review this week."}]'
      TALK_RATIO="0.67"
      NPS_LINE='      "npsScore": 5,'
      LIFECYCLE="renewal"
      ;;
    stalled:5)
      CALL_TITLE="Executive retention intervention"
      SENTIMENT="negative"
      OUTCOME="at_risk"
      TOPICS='["renewal","executive_escalation","retention"]'
      RISKS='["renewal_risk","exec_escalation","retention_risk"]'
      NEXT_STEPS='["Stand up executive retention plan","Lock accountable owners","Review progress every 48 hours"]'
      TRANSCRIPT_EXCERPT="Executive team requested a formal retention plan before final renewal decision."
      TRANSCRIPT_JSON='[{"speaker":"Executive Sponsor","text":"We need a formal retention plan before deciding renewal."},{"speaker":"CSM","text":"We will align all owners and provide a daily progress report."}]'
      TALK_RATIO="0.65"
      NPS_LINE=''
      LIFECYCLE="renewal"
      ;;
    expansion:0)
      CALL_TITLE="Value realization workshop"
      SENTIMENT="neutral"
      OUTCOME="stabilizing"
      TOPICS='["value","adoption","workflow"]'
      RISKS='[]'
      NEXT_STEPS='["Document measurable outcomes","Identify expansion candidates","Set quarterly success targets"]'
      TRANSCRIPT_EXCERPT="Customer reported strong operational gains and asked for optimization guidance."
      TRANSCRIPT_JSON='[{"speaker":"Customer","text":"We are seeing measurable gains across operations."},{"speaker":"CSM","text":"We can map those outcomes to expansion opportunities."}]'
      TALK_RATIO="0.49"
      NPS_LINE=''
      LIFECYCLE="adoption"
      ;;
    expansion:1)
      CALL_TITLE="Pilot expansion success review"
      SENTIMENT="positive"
      OUTCOME="won"
      TOPICS='["pilot","adoption","roi"]'
      RISKS='[]'
      NEXT_STEPS='["Publish pilot scorecard","Prepare procurement packet","Confirm rollout timeline"]'
      TRANSCRIPT_EXCERPT="Pilot delivered strong ROI and the customer approved broader rollout planning."
      TRANSCRIPT_JSON='[{"speaker":"Product Lead","text":"Pilot outcomes exceeded target KPIs."},{"speaker":"CSM","text":"We will package this for expansion approval."}]'
      TALK_RATIO="0.45"
      NPS_LINE=''
      LIFECYCLE="expansion"
      ;;
    expansion:2)
      CALL_TITLE="Champion advocacy call"
      SENTIMENT="positive"
      OUTCOME="expansion"
      TOPICS='["advocacy","reference","expansion"]'
      RISKS='[]'
      NEXT_STEPS='["Nominate executive reference","Draft expansion quote","Align implementation capacity"]'
      TRANSCRIPT_EXCERPT="Champion offered to be a reference and requested expansion terms for new teams."
      TRANSCRIPT_JSON='[{"speaker":"Champion","text":"We are happy to serve as a customer reference."},{"speaker":"CSM","text":"We will share expansion terms for additional teams."}]'
      TALK_RATIO="0.43"
      NPS_LINE='      "npsScore": 9,'
      LIFECYCLE="expansion"
      ;;
    expansion:3)
      CALL_TITLE="Multi-team rollout readiness"
      SENTIMENT="positive"
      OUTCOME="expansion"
      TOPICS='["rollout","enablement","governance"]'
      RISKS='[]'
      NEXT_STEPS='["Schedule admin training","Define governance model","Track onboarding quality"]'
      TRANSCRIPT_EXCERPT="Customer approved rollout to new business units with shared governance."
      TRANSCRIPT_JSON='[{"speaker":"Program Manager","text":"We are ready to roll out to additional business units."},{"speaker":"CSM","text":"We will coordinate enablement and governance."}]'
      TALK_RATIO="0.47"
      NPS_LINE=''
      LIFECYCLE="expansion"
      ;;
    expansion:4)
      CALL_TITLE="Commercial expansion negotiation"
      SENTIMENT="positive"
      OUTCOME="won"
      TOPICS='["procurement","expansion","commercials"]'
      RISKS='[]'
      NEXT_STEPS='["Finalize expansion order form","Confirm legal redlines","Book kickoff"]'
      TRANSCRIPT_EXCERPT="Procurement confirmed the expansion order is ready for signature."
      TRANSCRIPT_JSON='[{"speaker":"Procurement","text":"Expansion order is ready pending final signature."},{"speaker":"CSM","text":"We will send the final order form today."}]'
      TALK_RATIO="0.42"
      NPS_LINE='      "npsScore": 10,'
      LIFECYCLE="expansion"
      ;;
    *)
      CALL_TITLE="Executive reference and growth planning"
      SENTIMENT="positive"
      OUTCOME="expansion"
      TOPICS='["advocacy","growth","reference"]'
      RISKS='[]'
      NEXT_STEPS='["Record customer success story","Plan next-quarter growth","Nominate speaker for customer summit"]'
      TRANSCRIPT_EXCERPT="Customer committed to a public success story and additional growth planning."
      TRANSCRIPT_JSON='[{"speaker":"Executive Sponsor","text":"We can share this success story publicly."},{"speaker":"CSM","text":"Great, we will coordinate growth planning and advocacy."}]'
      TALK_RATIO="0.41"
      NPS_LINE=''
      LIFECYCLE="expansion"
      ;;
  esac
}

printf '{\n  "events": [\n' > "$GONG_FILE"
for ((i = 1; i <= GONG_COUNT; i++)); do
  account_index=$(( (i - 1) % 10 ))
  cycle=$(( ((i - 1) / 10) + 1 ))
  phase=$(( (cycle - 1) % 6 ))

  case "$account_index" in
    0)
      ACCOUNT_ID="northstar_tech"
      ACCOUNT_NAME="Northstar Tech"
      OWNER="Anna CSM"
      INDUSTRY="saas"
      SEGMENT="enterprise"
      REGION="na"
      ARR="520k"
      RENEWAL_WINDOW="31-90"
      TRAJECTORY="recovery"
      ;;
    1)
      ACCOUNT_ID="meridian_health"
      ACCOUNT_NAME="Meridian Health"
      OWNER="Priya Renewals"
      INDUSTRY="healthcare"
      SEGMENT="enterprise"
      REGION="na"
      ARR="840k"
      RENEWAL_WINDOW="0-30"
      TRAJECTORY="stalled"
      ;;
    2)
      ACCOUNT_ID="atlas_pay"
      ACCOUNT_NAME="Atlas Pay"
      OWNER="Sam Renewals"
      INDUSTRY="fintech"
      SEGMENT="enterprise"
      REGION="emea"
      ARR="910k"
      RENEWAL_WINDOW="31-90"
      TRAJECTORY="stalled"
      ;;
    3)
      ACCOUNT_ID="bluefreight"
      ACCOUNT_NAME="BlueFreight"
      OWNER="Dana CSM"
      INDUSTRY="logistics"
      SEGMENT="mid_market"
      REGION="na"
      ARR="330k"
      RENEWAL_WINDOW="91-180"
      TRAJECTORY="expansion"
      ;;
    4)
      ACCOUNT_ID="ironforge"
      ACCOUNT_NAME="IronForge"
      OWNER="Mia CSM"
      INDUSTRY="manufacturing"
      SEGMENT="enterprise"
      REGION="apac"
      ARR="610k"
      RENEWAL_WINDOW="31-90"
      TRAJECTORY="stalled"
      ;;
    5)
      ACCOUNT_ID="civiccloud"
      ACCOUNT_NAME="CivicCloud"
      OWNER="Nia CSM"
      INDUSTRY="public_sector"
      SEGMENT="enterprise"
      REGION="na"
      ARR="730k"
      RENEWAL_WINDOW="0-30"
      TRAJECTORY="stalled"
      ;;
    6)
      ACCOUNT_ID="harbor_retail"
      ACCOUNT_NAME="Harbor Retail"
      OWNER="Luis CSM"
      INDUSTRY="retail"
      SEGMENT="mid_market"
      REGION="na"
      ARR="280k"
      RENEWAL_WINDOW="31-90"
      TRAJECTORY="recovery"
      ;;
    7)
      ACCOUNT_ID="nova_telco"
      ACCOUNT_NAME="NovaTelco"
      OWNER="Security Operations"
      INDUSTRY="telecom"
      SEGMENT="enterprise"
      REGION="emea"
      ARR="680k"
      RENEWAL_WINDOW="0-30"
      TRAJECTORY="stalled"
      ;;
    8)
      ACCOUNT_ID="summit_university"
      ACCOUNT_NAME="Summit University"
      OWNER="Dana CSM"
      INDUSTRY="education"
      SEGMENT="commercial"
      REGION="na"
      ARR="190k"
      RENEWAL_WINDOW="91-180"
      TRAJECTORY="expansion"
      ;;
    *)
      ACCOUNT_ID="aegis_energy"
      ACCOUNT_NAME="Aegis Energy"
      OWNER="Mia CSM"
      INDUSTRY="energy"
      SEGMENT="enterprise"
      REGION="apac"
      ARR="760k"
      RENEWAL_WINDOW="31-90"
      TRAJECTORY="recovery"
      ;;
  esac

  set_story_fields "$TRAJECTORY" "$phase"

  if (( i % 6 == 0 )); then
    TIMESTAMP=$(printf "2025-12-%02dT%02d:%02d:00Z" "$(( (i % 28) + 1 ))" "$(( i % 24 ))" "$(( i % 60 ))")
  else
    TIMESTAMP=$(printf "2026-02-%02dT%02d:%02d:00Z" "$(( (i % 22) + 1 ))" "$(( i % 24 ))" "$(( i % 60 ))")
  fi

  printf '    {\n' >> "$GONG_FILE"
  printf '      "event": "call_analyzed",\n' >> "$GONG_FILE"
  printf '      "callId": "call-%05d",\n' "$i" >> "$GONG_FILE"
  printf '      "eventId": "evt-%05d",\n' "$i" >> "$GONG_FILE"
  printf '      "timestamp": "%s",\n' "$TIMESTAMP" >> "$GONG_FILE"
  printf '      "accountId": "%s",\n' "$ACCOUNT_ID" >> "$GONG_FILE"
  printf '      "accountName": "%s",\n' "$ACCOUNT_NAME" >> "$GONG_FILE"
  printf '      "industry": "%s",\n' "$INDUSTRY" >> "$GONG_FILE"
  printf '      "segment": "%s",\n' "$SEGMENT" >> "$GONG_FILE"
  printf '      "region": "%s",\n' "$REGION" >> "$GONG_FILE"
  printf '      "arr": "%s",\n' "$ARR" >> "$GONG_FILE"
  printf '      "renewalWindow": "%s",\n' "$RENEWAL_WINDOW" >> "$GONG_FILE"
  printf '      "lifecycle": "%s",\n' "$LIFECYCLE" >> "$GONG_FILE"
  printf '      "title": "%s",\n' "$CALL_TITLE" >> "$GONG_FILE"
  printf '      "owner": "%s",\n' "$OWNER" >> "$GONG_FILE"
  printf '      "participants": ["%s", "Executive Sponsor", "Product Lead"],\n' "$OWNER" >> "$GONG_FILE"
  printf '      "topics": %s,\n' "$TOPICS" >> "$GONG_FILE"
  printf '      "riskFlags": %s,\n' "$RISKS" >> "$GONG_FILE"
  printf '      "nextSteps": %s,\n' "$NEXT_STEPS" >> "$GONG_FILE"
  printf '      "sentiment": "%s",\n' "$SENTIMENT" >> "$GONG_FILE"
  printf '      "outcome": "%s",\n' "$OUTCOME" >> "$GONG_FILE"
  printf '      "talkRatioRep": %s,\n' "$TALK_RATIO" >> "$GONG_FILE"
  if [[ -n "$NPS_LINE" ]]; then
    printf '%s\n' "$NPS_LINE" >> "$GONG_FILE"
  fi
  printf '      "transcriptExcerpt": "%s",\n' "$TRANSCRIPT_EXCERPT" >> "$GONG_FILE"
  printf '      "transcript": %s\n' "$TRANSCRIPT_JSON" >> "$GONG_FILE"

  if (( i == GONG_COUNT )); then
    printf '    }\n' >> "$GONG_FILE"
  else
    printf '    },\n' >> "$GONG_FILE"
  fi
done
printf '  ]\n}\n' >> "$GONG_FILE"

echo "Generated $JIRA_COUNT Jira issues -> $JIRA_FILE"
echo "Generated $GONG_COUNT Gong events -> $GONG_FILE"
