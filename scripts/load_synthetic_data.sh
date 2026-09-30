#!/usr/bin/env bash
set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:8080}"
JIRA_JQL="${JIRA_JQL:-project is not EMPTY ORDER BY updated DESC}"
JIRA_LIMIT="${JIRA_LIMIT:-1000}"
GONG_MOCK_URL="${GONG_MOCK_URL:-${PENDO_MOCK_URL:-http://localhost:18082/events}}"
GONG_INGEST_KEY="${SIGNALOPS_WEBHOOK_INGEST_KEY:-${GONG_WEBHOOK_INGEST_KEY:-${PENDO_WEBHOOK_INGEST_KEY:-test-ingest-key}}}"

TMP_PAYLOAD="$(mktemp)"
trap 'rm -f "$TMP_PAYLOAD"' EXIT

echo "Syncing Jira synthetic data..."
curl -fsS -X POST "$BACKEND_URL/api/integrations/jira/sync" \
  -H "Content-Type: application/json" \
  -d "{\"jql\":\"$JIRA_JQL\",\"limit\":$JIRA_LIMIT}" \
  | tee /dev/stderr >/dev/null

echo "Fetching synthetic Gong payload from mock service..."
curl -fsS "$GONG_MOCK_URL" -o "$TMP_PAYLOAD"

echo "Ingesting synthetic Gong payload..."
curl -fsS -X POST "$BACKEND_URL/api/integrations/gong/webhook" \
  -H "Content-Type: application/json" \
  -H "x-signalops-ingest-key: $GONG_INGEST_KEY" \
  --data-binary "@$TMP_PAYLOAD" \
  | tee /dev/stderr >/dev/null

echo "Synthetic integration load complete."
