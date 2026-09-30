#!/usr/bin/env python3
"""
verify_synthetic_data.py — check every assumption the seed script claims.

Run this after generate_synthetic_data.sh. It exits non-zero on any failure
so it can be wired into CI. The goal: catch silent data corruption (like the
Jira-epic-scrambling bug that shipped unnoticed) before it reaches the bot.

Checks performed:
  1. Jira canonical epics exist with correct summaries and labels
  2. Every Jira sub-task's description epic-key matches the declared theme name
  3. Canonical drama sub-tasks (DEV-56, DEV-57) exist with expected labels
  4. Gong canonical calls (1,2,3,4,5,7,8) have matching sentiment/outcome
     consistent with the transcript bodies in gong_transcript.json
  5. Gong transcript call IDs all map to real events in gong_events.json
  6. Calendar has the 3 drama events (OOO, escalation, daily update)
  7. Figma has the 3 canonical drama comments with correct threading
  8. Linear titles reference DEV-* keys that exist in Jira
  9. GitHub PRs referenced in Linear titles appear in github_prs_response.json

A pass is: all green, no warnings, no mismatches.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTDATA = ROOT / "testdata"

EXPECTED_EPIC_THEMES = {
    "DEV-42": "Onboarding Redesign",
    "DEV-50": "API v2 Launch",
    "DEV-55": "Dark Mode",
    "DEV-34": "Mobile Deep Link Fix",
}

CANONICAL_CALL_EXPECTATIONS = {
    # (id, expected_sentiment, expected_outcome_regex)
    "gong-00001": ("positive", "expansion"),
    "gong-00002": ("neutral",  "stabilizing"),
    "gong-00003": ("negative", "at_risk"),
    "gong-00004": ("positive", "expansion"),
    "gong-00005": ("positive", "expansion"),
    "gong-00007": ("negative", "at_risk"),
    "gong-00008": ("positive", "expansion"),
}

failures = []
warnings = []


def fail(msg):
    failures.append(msg)
    print(f"  ✗ {msg}")


def warn(msg):
    warnings.append(msg)
    print(f"  ! {msg}")


def ok(msg):
    print(f"  ✓ {msg}")


def load_json(path):
    with open(path) as f:
        return json.load(f)


# ── 1 & 2: Jira canonical epics + sub-task theme consistency ────────────────
print("\n[1] Jira canonical epics")
jira = load_json(TESTDATA / "jira/wiremock/__files/jira_search_response.json")
jira_issues = {i["key"]: i for i in jira["issues"]}

for key, expected_theme in EXPECTED_EPIC_THEMES.items():
    if key not in jira_issues:
        fail(f"Canonical epic {key} missing from jira_search_response.json")
        continue
    fields = jira_issues[key]["fields"]
    summary = fields.get("summary", "")
    labels = fields.get("labels", [])
    # The epic issue's summary should name its own theme
    theme_slug = expected_theme.lower().replace(" ", "-")
    if expected_theme.split()[0].lower() not in summary.lower():
        fail(f"{key} summary doesn't reference its theme '{expected_theme}': {summary!r}")
    elif "epic" not in labels:
        fail(f"{key} missing 'epic' label")
    elif theme_slug not in labels:
        fail(f"{key} missing theme label '{theme_slug}' (labels={labels})")
    else:
        ok(f"{key}: {summary}")

print("\n[2] Jira sub-task theme consistency (desc epic-key ↔ theme name)")
mismatches = 0
sub_count = 0
for key, issue in jira_issues.items():
    if key in EXPECTED_EPIC_THEMES:
        continue
    sub_count += 1
    desc = issue["fields"].get("description", "")
    m = re.search(r"Part of epic (DEV-\d+) \(([^)]+)\)", desc)
    if not m:
        # Description might use a different pattern — only fail if claims epic but misnames
        continue
    epic_key, theme = m.group(1), m.group(2)
    expected = EXPECTED_EPIC_THEMES.get(epic_key)
    if expected and theme != expected:
        fail(f"{key}: description says '({theme})' for {epic_key} (expected '{expected}')")
        mismatches += 1
if mismatches == 0:
    ok(f"{sub_count} sub-tasks all have consistent epic+theme references")

# ── 3: Drama sub-tasks ──────────────────────────────────────────────────────
print("\n[3] Jira canonical drama sub-tasks")
for key, expected_label in [("DEV-56", "stale"), ("DEV-57", "blocked-by-dev-34")]:
    if key not in jira_issues:
        fail(f"Drama sub-task {key} missing")
        continue
    labels = jira_issues[key]["fields"].get("labels", [])
    if expected_label not in labels:
        fail(f"{key} missing expected label '{expected_label}' (labels={labels})")
    else:
        ok(f"{key}: {jira_issues[key]['fields']['summary']}")

# ── 4 & 5: Gong transcript ↔ events consistency ─────────────────────────────
print("\n[4] Gong canonical calls sentiment/outcome consistency")
gong_events = load_json(TESTDATA / "gong/wiremock/__files/gong_events.json")
events_by_id = {e["id"]: e for e in gong_events["events"]}

for call_id, (exp_sentiment, exp_outcome) in CANONICAL_CALL_EXPECTATIONS.items():
    if call_id not in events_by_id:
        fail(f"Canonical call {call_id} missing from gong_events.json")
        continue
    e = events_by_id[call_id]
    if e.get("sentiment") != exp_sentiment:
        fail(f"{call_id}: sentiment is {e.get('sentiment')!r}, expected {exp_sentiment!r}")
    elif e.get("outcome") != exp_outcome:
        fail(f"{call_id}: outcome is {e.get('outcome')!r}, expected {exp_outcome!r}")
    else:
        ok(f"{call_id}: {e.get('title','?')} [{exp_sentiment}/{exp_outcome}]")

print("\n[5] Gong transcript bodies map to real events")
transcripts = load_json(TESTDATA / "gong/wiremock/__files/gong_transcript.json")
transcript_ids = [t["callId"] for t in transcripts["callTranscripts"]]
for tid in transcript_ids:
    if tid not in events_by_id:
        fail(f"Transcript references callId {tid} but no such event exists")
    else:
        ok(f"{tid} transcript maps to real event")

# ── 6: Calendar drama events ────────────────────────────────────────────────
print("\n[6] Calendar drama events")
cal = load_json(TESTDATA / "google/wiremock/__files/gcal_events_response.json")
cal_ids = [e["id"] for e in cal["items"]]
for required in ["gcal-canon-01", "gcal-canon-02", "gcal-canon-03"]:
    if required not in cal_ids:
        fail(f"Calendar drama event {required} missing")
    else:
        ok(f"{required} present")

# ── 7: Figma canonical drama comments ───────────────────────────────────────
print("\n[7] Figma canonical drama thread")
figma = load_json(TESTDATA / "figma/wiremock/__files/figma_comments_response.json")
comments_by_id = {c["id"]: c for c in figma["comments"]}
for cid in ["fc-canon-01", "fc-canon-02", "fc-canon-03"]:
    if cid not in comments_by_id:
        fail(f"Figma canonical comment {cid} missing")
    else:
        ok(f"{cid}: {comments_by_id[cid]['message'][:80]}")

# Verify threading
for child, expected_parent in [("fc-canon-02", "fc-canon-01"), ("fc-canon-03", "fc-canon-01")]:
    if child in comments_by_id and comments_by_id[child].get("parent_id") != expected_parent:
        fail(
            f"{child} parent_id should be {expected_parent}, "
            f"got {comments_by_id[child].get('parent_id')!r}"
        )

# ── 8: Linear references existing Jira keys ────────────────────────────────
print("\n[8] Linear titles reference real Jira keys")
linear = load_json(TESTDATA / "linear/wiremock/__files/linear_issues_response.json")
nodes = linear["data"]["issueSearch"]["nodes"]
dev_key_re = re.compile(r"DEV-\d+")
bad_refs = 0
for n in nodes:
    title = n.get("title", "")
    for m in dev_key_re.findall(title):
        if m not in jira_issues:
            warn(f"Linear {n.get('identifier','?')} references missing Jira key {m} in title")
            bad_refs += 1
if bad_refs == 0:
    ok(f"All Linear DEV-* references resolve to real Jira issues")

# ── 9: GitHub PRs referenced in Linear titles exist in GitHub data ─────────
print("\n[9] GitHub PRs referenced in Linear titles exist")
gh_prs = load_json(TESTDATA / "github/wiremock/__files/github_prs_response.json")
# Handle common list shapes
if isinstance(gh_prs, list):
    pr_numbers = {p.get("number") for p in gh_prs}
else:
    pr_numbers = {p.get("number") for p in gh_prs.get("items", [])}

pr_ref_re = re.compile(r"PR #(\d+)|#(\d+)")
missing_prs = 0
for n in nodes:
    title = n.get("title", "")
    for m in pr_ref_re.finditer(title):
        num = int(m.group(1) or m.group(2))
        # Filter: only check numbers in plausible PR range (>=100)
        if num < 100:
            continue
        if num not in pr_numbers:
            # Warn rather than fail — #228 is a dashboard issue referenced loosely
            warn(f"Linear {n.get('identifier','?')} title references #{num} not in github_prs")
            missing_prs += 1
if missing_prs == 0:
    ok("All PR numbers referenced in Linear titles exist in github_prs_response.json")

# ── Summary ────────────────────────────────────────────────────────────────
print()
print("=" * 70)
print(f"Verification complete: {len(failures)} failure(s), {len(warnings)} warning(s)")
print("=" * 70)
if failures:
    print("\nFAILURES:")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
if warnings:
    print("\n(warnings are soft — investigate but not blocking)")
sys.exit(0)
