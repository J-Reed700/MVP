param(
  [string]$BackendUrl = $(if ($env:BACKEND_URL) { $env:BACKEND_URL } else { "http://localhost:8080" }),
  [string]$JiraJql = $(if ($env:JIRA_JQL) { $env:JIRA_JQL } else { "project is not EMPTY ORDER BY updated DESC" }),
  [int]$JiraLimit = $(if ($env:JIRA_LIMIT) { [int]$env:JIRA_LIMIT } else { 1000 }),
  [string]$GongMockUrl = $(if ($env:GONG_MOCK_URL) { $env:GONG_MOCK_URL } elseif ($env:PENDO_MOCK_URL) { $env:PENDO_MOCK_URL } else { "http://localhost:18082/events" }),
  [string]$GongIngestKey = $(
    if ($env:SIGNALOPS_WEBHOOK_INGEST_KEY) {
      $env:SIGNALOPS_WEBHOOK_INGEST_KEY
    } elseif ($env:GONG_WEBHOOK_INGEST_KEY) {
      $env:GONG_WEBHOOK_INGEST_KEY
    } elseif ($env:PENDO_WEBHOOK_INGEST_KEY) {
      $env:PENDO_WEBHOOK_INGEST_KEY
    } else {
      "test-ingest-key"
    }
  )
)

$ErrorActionPreference = "Stop"

Write-Host "Syncing Jira synthetic data..."
$jiraPayload = @{
  jql = $JiraJql
  limit = $JiraLimit
} | ConvertTo-Json

$jiraResponse = Invoke-RestMethod `
  -Method Post `
  -Uri "$BackendUrl/api/integrations/jira/sync" `
  -ContentType "application/json" `
  -Body $jiraPayload
$jiraResponse | ConvertTo-Json -Depth 10

Write-Host "Fetching synthetic Gong payload from mock service..."
$gongPayloadRaw = Invoke-RestMethod -Method Get -Uri $GongMockUrl
$gongPayload = $gongPayloadRaw | ConvertTo-Json -Depth 20

Write-Host "Ingesting synthetic Gong payload..."
$gongResponse = Invoke-RestMethod `
  -Method Post `
  -Uri "$BackendUrl/api/integrations/gong/webhook" `
  -ContentType "application/json" `
  -Headers @{ "x-signalops-ingest-key" = $GongIngestKey } `
  -Body $gongPayload
$gongResponse | ConvertTo-Json -Depth 10

Write-Host "Synthetic integration load complete."
