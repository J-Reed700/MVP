param(
  [string]$BackendUrl = $(if ($env:BACKEND_URL) { $env:BACKEND_URL } else { "http://localhost:8080" })
)

$ErrorActionPreference = "Stop"

Write-Host "Resetting backend to curated story dataset..."
$response = Invoke-RestMethod `
  -Method Post `
  -Uri "$BackendUrl/api/dev/story/reset" `
  -ContentType "application/json"
$response | ConvertTo-Json -Depth 10
Write-Host "Story dataset ready."
