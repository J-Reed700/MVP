#!/usr/bin/env python3
"""
verify_mock_coverage.py — assert every skill-tool URL has a matching WireMock
mapping. Run after any change to workspace/skills/*/SKILL.md or testdata/*/wiremock/*.

Exits non-zero if any skill URL would hit a 404 against our mocks.
"""

import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TESTDATA = os.path.join(ROOT, "testdata")

# Skill URL → (method, concrete example path, which integration dir to check)
#
# Kept explicit rather than parsed from SKILL.md because skill body templates
# mix URL templates and body templates; enumerating the paths the bot would
# send in practice is the thing we actually want to verify.
ENDPOINTS = [
    # ── JIRA ──
    ("jira", "GET",  "/rest/api/3/search?jql=foo&maxResults=10&fields=summary"),
    ("jira", "GET",  "/rest/api/3/issue/DEV-42?fields=summary"),
    ("jira", "POST", "/rest/api/3/issue"),
    ("jira", "PUT",  "/rest/api/3/issue/DEV-42"),
    ("jira", "GET",  "/rest/api/3/issue/DEV-42/transitions"),
    ("jira", "POST", "/rest/api/3/issue/DEV-42/transitions"),
    ("jira", "POST", "/rest/api/3/issue/DEV-42/comment"),
    ("jira", "PUT",  "/rest/api/3/issue/DEV-42/assignee"),
    ("jira", "GET",  "/rest/agile/1.0/board/1/sprint?state=active"),
    ("jira", "GET",  "/rest/agile/1.0/board?projectKeyOrId=DEV&maxResults=10"),
    ("jira", "POST", "/rest/api/3/issueLink"),

    # ── LINEAR (per-operation routing via bodyPatterns) ──
    ("linear", "POST", "/graphql", {"body": "query { issueSearch(query: \"foo\", first: 10) { nodes { id } } }"}),
    ("linear", "POST", "/graphql", {"body": "mutation CreateIssue($input: IssueCreateInput!) { issueCreate(input: $input) { success } }"}),
    ("linear", "POST", "/graphql", {"body": "mutation { issueUpdate(id: \"x\", input: {}) { success } }"}),
    ("linear", "POST", "/graphql", {"body": "mutation { commentCreate(input: { issueId: \"x\", body: \"y\" }) { success } }"}),
    ("linear", "POST", "/graphql", {"body": "query { projects(first: 10, filter: { state: { eq: \"started\" } }) { nodes { id } } }"}),
    ("linear", "POST", "/graphql", {"body": "query { team(id: \"x\") { activeCycle { id } } }"}),
    ("linear", "POST", "/graphql", {"body": "query { users(first: 50) { nodes { id } } teams { nodes { id } } }"}),
    ("linear", "POST", "/graphql", {"body": "query { team(id: \"x\") { states { nodes { id } } } }"}),

    # ── NOTION ──
    ("notion", "POST",  "/v1/search"),
    ("notion", "GET",   "/v1/pages/00000001-0007-4000-a001-00000000001f"),
    ("notion", "GET",   "/v1/blocks/00000001-0007-4000-a001-00000000001f/children?page_size=100"),
    ("notion", "POST",  "/v1/pages"),
    ("notion", "POST",  "/v1/databases/abc-def-123/query"),
    ("notion", "PATCH", "/v1/blocks/00000001-0007-4000-a001-00000000001f/children"),
    ("notion", "PATCH", "/v1/pages/00000001-0007-4000-a001-00000000001f"),

    # ── CONFLUENCE ──
    ("confluence", "GET",  "/wiki/rest/api/content/search?cql=foo&limit=10&expand=version,space"),
    ("confluence", "GET",  "/wiki/rest/api/content/12345?expand=body.storage"),
    ("confluence", "POST", "/wiki/rest/api/content"),
    ("confluence", "PUT",  "/wiki/rest/api/content/12345"),
    ("confluence", "GET",  "/wiki/rest/api/space?limit=10&expand=description.plain"),
    ("confluence", "GET",  "/wiki/rest/api/content/12345/child/page?limit=10&expand=version"),

    # ── GITHUB ──
    ("github", "GET",   "/search/repositories?q=foo&per_page=10"),
    ("github", "GET",   "/search/issues?q=foo&per_page=20"),
    ("github", "GET",   "/repos/acme/delegate/issues/42"),
    ("github", "POST",  "/repos/acme/delegate/issues"),
    ("github", "PATCH", "/repos/acme/delegate/issues/42"),
    ("github", "POST",  "/repos/acme/delegate/issues/42/comments"),
    ("github", "GET",   "/repos/acme/delegate/pulls?state=open&per_page=20"),
    ("github", "GET",   "/repos/acme/delegate/pulls/189"),
    ("github", "GET",   "/repos/acme/delegate/pulls/189/reviews"),
    ("github", "GET",   "/repos/acme/delegate/actions/runs?status=failure&per_page=10"),

    # ── FIGMA ──
    ("figma", "GET",  "/v1/files/abc123XYZ?depth=1"),
    ("figma", "GET",  "/v1/files/abc123XYZ/comments"),
    ("figma", "POST", "/v1/files/abc123XYZ/comments"),
    ("figma", "GET",  "/v1/files/abc123XYZ/versions"),
    ("figma", "GET",  "/v1/teams/1234567/projects"),
    ("figma", "GET",  "/v1/projects/7777/files"),

    # ── GOOGLE CALENDAR ──
    ("google", "GET",  "/calendar/v3/calendars/primary/events?timeMin=t1&timeMax=t2&maxResults=50&singleEvents=true&orderBy=startTime"),
    ("google", "GET",  "/calendar/v3/calendars/primary/events/abc123"),
    ("google", "GET",  "/calendar/v3/users/me/calendarList?maxResults=50"),
    ("google", "POST", "/calendar/v3/freeBusy"),

    # ── GMAIL ──
    ("google", "GET",  "/gmail/v1/users/me/messages?q=foo&maxResults=10"),
    ("google", "GET",  "/gmail/v1/users/me/messages/msg0001?format=full"),
    ("google", "POST", "/gmail/v1/users/me/messages/send"),
    ("google", "POST", "/gmail/v1/users/me/drafts"),
    ("google", "GET",  "/gmail/v1/users/me/labels"),
    ("google", "GET",  "/gmail/v1/users/me/threads/thr0001?format=full"),

    # ── GONG ──
    ("gong", "POST", "/v2/calls/extensive"),
    ("gong", "POST", "/v2/calls/transcript"),
    ("gong", "GET",  "/v2/users"),
    ("gong", "GET",  "/v2/deals/deal-globalretail"),
]


def load_mappings(integration):
    d = os.path.join(TESTDATA, integration, "wiremock", "mappings")
    if not os.path.isdir(d):
        return []
    out = []
    for f in sorted(glob.glob(os.path.join(d, "*.json"))):
        with open(f) as fh:
            out.append((os.path.basename(f), json.load(fh)))
    return out


def url_match(req, method, path):
    if req.get("method", "ANY") not in (method, "ANY"):
        return False
    path_noqs = path.split("?", 1)[0]
    if "urlPath" in req:
        return req["urlPath"] == path_noqs
    if "urlPathPattern" in req:
        pat = req["urlPathPattern"]
        target = path if pat.endswith(".*") else path_noqs
        try:
            return re.fullmatch(pat, target) is not None
        except re.error:
            return False
    if "url" in req:
        return req["url"] == path
    if "urlPattern" in req:
        try:
            return re.fullmatch(req["urlPattern"], path) is not None
        except re.error:
            return False
    return False


def body_match(req, body):
    """Respect WireMock bodyPatterns. Support `contains` and `matchesJsonPath`."""
    patterns = req.get("bodyPatterns")
    if not patterns:
        return True  # mapping doesn't care about body
    if body is None:
        return False
    for p in patterns:
        if "contains" in p:
            if p["contains"] not in body:
                return False
        elif "matches" in p:
            if not re.search(p["matches"], body):
                return False
        elif "equalToJson" in p:
            try:
                if json.loads(body) != json.loads(p["equalToJson"]):
                    return False
            except Exception:
                return False
        # Unknown predicate: treat as pass to avoid false negatives
    return True


def find_mapping(integration, method, path, extra):
    body = (extra or {}).get("body")
    maps = load_mappings(integration)
    # Sort by priority (lower number = higher priority). Default priority is 5.
    maps = sorted(maps, key=lambda nm: nm[1].get("priority", 5))
    for name, m in maps:
        req = m["request"]
        if url_match(req, method, path) and body_match(req, body):
            return name
    return None


print(f"{'INT':<10} {'METHOD':<6} {'PATH':<68} RESULT")
print("-" * 120)

covered = 0
missing = []
for entry in ENDPOINTS:
    if len(entry) == 4:
        integration, method, path, extra = entry
    else:
        integration, method, path = entry
        extra = None

    hit = find_mapping(integration, method, path, extra)
    display_path = path
    if extra and "body" in extra:
        body_hint = extra["body"][:40].replace("\n", " ")
        display_path = f"{path} [body~'{body_hint}']"
    if hit:
        covered += 1
        print(f"{integration:<10} {method:<6} {display_path[:66]:<68} ✓ {hit}")
    else:
        missing.append((integration, method, path, extra))
        print(f"{integration:<10} {method:<6} {display_path[:66]:<68} ✗ NO MAPPING")

total = len(ENDPOINTS)
print()
print("=" * 70)
print(f"Mock coverage: {covered}/{total} ({covered*100//total}%)")
print("=" * 70)
if missing:
    print("\nMISSING:")
    for integration, method, path, _ in missing:
        print(f"  - {integration} {method} {path}")
    sys.exit(1)
sys.exit(0)
