#!/usr/bin/env bash
set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:8080}"

echo "Resetting backend to curated story dataset..."
curl -sS -X POST "$BACKEND_URL/api/dev/story/reset" \
  -H "Content-Type: application/json"
echo
echo "Story dataset ready."
