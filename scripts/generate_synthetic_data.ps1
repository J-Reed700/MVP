param(
  [int]$JiraCount = $(if ($env:JIRA_SYNTH_COUNT) { [int]$env:JIRA_SYNTH_COUNT } else { 1000 }),
  [int]$GongCount = $(if ($env:GONG_SYNTH_COUNT) { [int]$env:GONG_SYNTH_COUNT } elseif ($env:PENDO_SYNTH_COUNT) { [int]$env:PENDO_SYNTH_COUNT } else { 5000 })
)

$ErrorActionPreference = "Stop"

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$jiraFile = Join-Path $rootDir "testdata/jira/wiremock/__files/jira_search_response.json"
$gongFile = Join-Path $rootDir "testdata/gong/wiremock/__files/gong_events.json"

New-Item -ItemType Directory -Force -Path (Split-Path $jiraFile -Parent) | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path $gongFile -Parent) | Out-Null

$jiraIssues = @()
for ($i = 1; $i -le $JiraCount; $i++) {
  if ($i % 3 -eq 0) {
    $status = "Done"
  } elseif ($i % 2 -eq 0) {
    $status = "In Progress"
  } else {
    $status = "To Do"
  }

  if ($i % 5 -eq 0) {
    $updated = "2025-12-{0:00}T{1:00}:{2:00}:00.000+0000" -f (($i % 28) + 1), ($i % 24), ($i % 60)
  } else {
    $updated = "2026-02-{0:00}T{1:00}:{2:00}:00.000+0000" -f (($i % 22) + 1), ($i % 24), ($i % 60)
  }

  $jiraIssues += @{
    key = "MVP-$i"
    fields = @{
      summary = "Synthetic Jira decision candidate $i"
      description = "Synthetic issue $i generated for MVP integration testing."
      labels = @("synthetic", "batch", "iteration-$((($i % 20) + 1))")
      updated = $updated
      status = @{ name = $status }
      project = @{ key = "MVP"; name = "TruthGraph MVP" }
    }
  }
}

$jiraPayload = @{ issues = $jiraIssues } | ConvertTo-Json -Depth 8
Set-Content -Path $jiraFile -Value $jiraPayload -Encoding UTF8

$accounts = @(
  @{ Id = "northstar_tech"; Name = "Northstar Tech"; Owner = "Anna CSM"; Industry = "saas"; Segment = "enterprise"; Region = "na"; Arr = "520k"; RenewalWindow = "31-90"; Trajectory = "recovery" },
  @{ Id = "meridian_health"; Name = "Meridian Health"; Owner = "Priya Renewals"; Industry = "healthcare"; Segment = "enterprise"; Region = "na"; Arr = "840k"; RenewalWindow = "0-30"; Trajectory = "stalled" },
  @{ Id = "atlas_pay"; Name = "Atlas Pay"; Owner = "Sam Renewals"; Industry = "fintech"; Segment = "enterprise"; Region = "emea"; Arr = "910k"; RenewalWindow = "31-90"; Trajectory = "stalled" },
  @{ Id = "bluefreight"; Name = "BlueFreight"; Owner = "Dana CSM"; Industry = "logistics"; Segment = "mid_market"; Region = "na"; Arr = "330k"; RenewalWindow = "91-180"; Trajectory = "expansion" },
  @{ Id = "ironforge"; Name = "IronForge"; Owner = "Mia CSM"; Industry = "manufacturing"; Segment = "enterprise"; Region = "apac"; Arr = "610k"; RenewalWindow = "31-90"; Trajectory = "stalled" },
  @{ Id = "civiccloud"; Name = "CivicCloud"; Owner = "Nia CSM"; Industry = "public_sector"; Segment = "enterprise"; Region = "na"; Arr = "730k"; RenewalWindow = "0-30"; Trajectory = "stalled" },
  @{ Id = "harbor_retail"; Name = "Harbor Retail"; Owner = "Luis CSM"; Industry = "retail"; Segment = "mid_market"; Region = "na"; Arr = "280k"; RenewalWindow = "31-90"; Trajectory = "recovery" },
  @{ Id = "nova_telco"; Name = "NovaTelco"; Owner = "Security Operations"; Industry = "telecom"; Segment = "enterprise"; Region = "emea"; Arr = "680k"; RenewalWindow = "0-30"; Trajectory = "stalled" },
  @{ Id = "summit_university"; Name = "Summit University"; Owner = "Dana CSM"; Industry = "education"; Segment = "commercial"; Region = "na"; Arr = "190k"; RenewalWindow = "91-180"; Trajectory = "expansion" },
  @{ Id = "aegis_energy"; Name = "Aegis Energy"; Owner = "Mia CSM"; Industry = "energy"; Segment = "enterprise"; Region = "apac"; Arr = "760k"; RenewalWindow = "31-90"; Trajectory = "recovery" }
)

function Get-StoryFields {
  param(
    [string]$Trajectory,
    [int]$Phase
  )

  switch ("$Trajectory:$Phase") {
    "recovery:0" {
      return @{
        Title = "Onboarding adoption risk discovery"
        Sentiment = "negative"
        Outcome = "at_risk"
        Topics = @("onboarding", "activation", "renewal")
        RiskFlags = @("adoption_risk", "renewal_risk")
        NextSteps = @("Launch activation recovery sprint", "Assign implementation owner", "Review usage weekly")
        TranscriptExcerpt = "Customer said onboarding is slow and activation dropped after rollout."
        Transcript = @(
          @{ speaker = "CSM"; text = "We saw activation decline after the rollout." },
          @{ speaker = "Customer"; text = "Onboarding is blocked and renewal confidence is low." }
        )
        TalkRatioRep = 0.68
        NpsScore = 4
        Lifecycle = "onboarding"
      }
    }
    "recovery:1" {
      return @{
        Title = "Executive renewal risk checkpoint"
        Sentiment = "negative"
        Outcome = "at_risk"
        Topics = @("renewal", "executive_escalation", "value")
        RiskFlags = @("renewal_risk", "exec_escalation")
        NextSteps = @("Publish executive recovery plan", "Schedule sponsor checkpoint", "Lock weekly risk review")
        TranscriptExcerpt = "Executive sponsor requested milestones before renewal committee review."
        Transcript = @(
          @{ speaker = "CSM"; text = "We can provide a 30 day remediation plan." },
          @{ speaker = "Executive Sponsor"; text = "We need signed milestones before committee review." }
        )
        TalkRatioRep = 0.64
        NpsScore = $null
        Lifecycle = "renewal"
      }
    }
    "recovery:2" {
      return @{
        Title = "Mitigation plan and security controls review"
        Sentiment = "neutral"
        Outcome = "blocked"
        Topics = @("security", "compliance", "mitigation")
        RiskFlags = @("security_blocker", "compliance_gap")
        NextSteps = @("Deliver control mapping", "Complete certificate review", "Confirm go-live checklist")
        TranscriptExcerpt = "Security team asked for updated control evidence before unblocking go-live."
        Transcript = @(
          @{ speaker = "Security Lead"; text = "We need updated control evidence before sign-off." },
          @{ speaker = "CSM"; text = "We will deliver the package before Friday." }
        )
        TalkRatioRep = 0.57
        NpsScore = $null
        Lifecycle = "implementation"
      }
    }
    "recovery:3" {
      return @{
        Title = "Adoption rebound validation"
        Sentiment = "neutral"
        Outcome = "stabilizing"
        Topics = @("adoption", "enablement", "health_score")
        RiskFlags = @("adoption_watch")
        NextSteps = @("Scale admin enablement", "Measure weekly active teams", "Track workflow completion")
        TranscriptExcerpt = "Customer confirmed adoption is improving but requested closer monitoring."
        Transcript = @(
          @{ speaker = "Customer Admin"; text = "Usage is improving across key teams." },
          @{ speaker = "CSM"; text = "We will keep weekly check-ins through renewal." }
        )
        TalkRatioRep = 0.52
        NpsScore = 7
        Lifecycle = "adoption"
      }
    }
    "recovery:4" {
      return @{
        Title = "Renewal committee readiness"
        Sentiment = "positive"
        Outcome = "won"
        Topics = @("renewal", "governance", "success_plan")
        RiskFlags = @()
        NextSteps = @("Finalize renewal paperwork", "Capture success metrics", "Transition to growth plan")
        TranscriptExcerpt = "Procurement confirmed the renewal is on track after recovery milestones were met."
        Transcript = @(
          @{ speaker = "Procurement"; text = "The renewal is moving forward with current terms." },
          @{ speaker = "CSM"; text = "We will send final documentation today." }
        )
        TalkRatioRep = 0.46
        NpsScore = 9
        Lifecycle = "renewal"
      }
    }
    "recovery:5" {
      return @{
        Title = "Expansion planning workshop"
        Sentiment = "positive"
        Outcome = "expansion"
        Topics = @("expansion", "advocacy", "multi_team_rollout")
        RiskFlags = @()
        NextSteps = @("Draft expansion proposal", "Plan phase-two rollout", "Nominate customer advocate")
        TranscriptExcerpt = "Customer requested expansion pricing and agreed to serve as a reference account."
        Transcript = @(
          @{ speaker = "Executive Sponsor"; text = "We want to expand to additional teams." },
          @{ speaker = "CSM"; text = "We will propose a phased expansion package." }
        )
        TalkRatioRep = 0.44
        NpsScore = 10
        Lifecycle = "expansion"
      }
    }
    "stalled:0" {
      return @{
        Title = "Compliance blocker escalation"
        Sentiment = "neutral"
        Outcome = "blocked"
        Topics = @("compliance", "audit", "security")
        RiskFlags = @("compliance_gap", "security_blocker")
        NextSteps = @("Submit audit evidence", "Align legal controls", "Set compliance war-room")
        TranscriptExcerpt = "Legal and security teams blocked progress pending audit evidence."
        Transcript = @(
          @{ speaker = "Legal"; text = "We cannot proceed without complete audit evidence." },
          @{ speaker = "CSM"; text = "We will coordinate with security on missing controls." }
        )
        TalkRatioRep = 0.61
        NpsScore = $null
        Lifecycle = "implementation"
      }
    }
    "stalled:1" {
      return @{
        Title = "Incident and reliability review"
        Sentiment = "negative"
        Outcome = "escalated"
        Topics = @("stability", "incident", "sla")
        RiskFlags = @("stability_risk", "sla_breach")
        NextSteps = @("Open incident command cadence", "Publish ETA updates", "Track sev2 trend")
        TranscriptExcerpt = "Customer reported repeated latency incidents and requested leadership escalation."
        Transcript = @(
          @{ speaker = "Customer"; text = "We have seen repeated latency incidents this month." },
          @{ speaker = "CSM"; text = "We are escalating this to platform leadership today." }
        )
        TalkRatioRep = 0.66
        NpsScore = $null
        Lifecycle = "adoption"
      }
    }
    "stalled:2" {
      return @{
        Title = "Budget and procurement delay"
        Sentiment = "neutral"
        Outcome = "blocked"
        Topics = @("budget", "procurement", "renewal")
        RiskFlags = @("budget_risk", "procurement_delay")
        NextSteps = @("Offer phased commercial plan", "Align procurement checklist", "Confirm legal timeline")
        TranscriptExcerpt = "Finance requested phased pricing due to budget constraints this quarter."
        Transcript = @(
          @{ speaker = "Finance"; text = "Budget constraints require a phased commercial structure." },
          @{ speaker = "CSM"; text = "We can provide a staged pricing option this week." }
        )
        TalkRatioRep = 0.58
        NpsScore = $null
        Lifecycle = "renewal"
      }
    }
    "stalled:3" {
      return @{
        Title = "Support backlog and response-time escalation"
        Sentiment = "negative"
        Outcome = "escalated"
        Topics = @("support", "escalation", "response_time")
        RiskFlags = @("support_backlog", "response_time_risk")
        NextSteps = @("Assign named escalation owner", "Clear top backlog tickets", "Publish daily status")
        TranscriptExcerpt = "Customer escalated unresolved tickets and asked for executive visibility."
        Transcript = @(
          @{ speaker = "Customer"; text = "Open support tickets are blocking our rollout." },
          @{ speaker = "CSM"; text = "We will assign an escalation owner and send daily updates." }
        )
        TalkRatioRep = 0.63
        NpsScore = $null
        Lifecycle = "adoption"
      }
    }
    "stalled:4" {
      return @{
        Title = "Sentiment drop and churn warning"
        Sentiment = "negative"
        Outcome = "at_risk"
        Topics = @("sentiment", "renewal", "competition")
        RiskFlags = @("sentiment_risk", "competitive_risk", "renewal_risk")
        NextSteps = @("Run executive value review", "Deliver competitive ROI brief", "Schedule churn-risk checkpoint")
        TranscriptExcerpt = "Champion mentioned competitor evaluation and low confidence in renewal outcomes."
        Transcript = @(
          @{ speaker = "Champion"; text = "We are evaluating alternatives due to unresolved issues." },
          @{ speaker = "CSM"; text = "We will run an executive value review this week." }
        )
        TalkRatioRep = 0.67
        NpsScore = 5
        Lifecycle = "renewal"
      }
    }
    "stalled:5" {
      return @{
        Title = "Executive retention intervention"
        Sentiment = "negative"
        Outcome = "at_risk"
        Topics = @("renewal", "executive_escalation", "retention")
        RiskFlags = @("renewal_risk", "exec_escalation", "retention_risk")
        NextSteps = @("Stand up executive retention plan", "Lock accountable owners", "Review progress every 48 hours")
        TranscriptExcerpt = "Executive team requested a formal retention plan before final renewal decision."
        Transcript = @(
          @{ speaker = "Executive Sponsor"; text = "We need a formal retention plan before deciding renewal." },
          @{ speaker = "CSM"; text = "We will align all owners and provide a daily progress report." }
        )
        TalkRatioRep = 0.65
        NpsScore = $null
        Lifecycle = "renewal"
      }
    }
    "expansion:0" {
      return @{
        Title = "Value realization workshop"
        Sentiment = "neutral"
        Outcome = "stabilizing"
        Topics = @("value", "adoption", "workflow")
        RiskFlags = @()
        NextSteps = @("Document measurable outcomes", "Identify expansion candidates", "Set quarterly success targets")
        TranscriptExcerpt = "Customer reported strong operational gains and asked for optimization guidance."
        Transcript = @(
          @{ speaker = "Customer"; text = "We are seeing measurable gains across operations." },
          @{ speaker = "CSM"; text = "We can map those outcomes to expansion opportunities." }
        )
        TalkRatioRep = 0.49
        NpsScore = $null
        Lifecycle = "adoption"
      }
    }
    "expansion:1" {
      return @{
        Title = "Pilot expansion success review"
        Sentiment = "positive"
        Outcome = "won"
        Topics = @("pilot", "adoption", "roi")
        RiskFlags = @()
        NextSteps = @("Publish pilot scorecard", "Prepare procurement packet", "Confirm rollout timeline")
        TranscriptExcerpt = "Pilot delivered strong ROI and the customer approved broader rollout planning."
        Transcript = @(
          @{ speaker = "Product Lead"; text = "Pilot outcomes exceeded target KPIs." },
          @{ speaker = "CSM"; text = "We will package this for expansion approval." }
        )
        TalkRatioRep = 0.45
        NpsScore = $null
        Lifecycle = "expansion"
      }
    }
    "expansion:2" {
      return @{
        Title = "Champion advocacy call"
        Sentiment = "positive"
        Outcome = "expansion"
        Topics = @("advocacy", "reference", "expansion")
        RiskFlags = @()
        NextSteps = @("Nominate executive reference", "Draft expansion quote", "Align implementation capacity")
        TranscriptExcerpt = "Champion offered to be a reference and requested expansion terms for new teams."
        Transcript = @(
          @{ speaker = "Champion"; text = "We are happy to serve as a customer reference." },
          @{ speaker = "CSM"; text = "We will share expansion terms for additional teams." }
        )
        TalkRatioRep = 0.43
        NpsScore = 9
        Lifecycle = "expansion"
      }
    }
    "expansion:3" {
      return @{
        Title = "Multi-team rollout readiness"
        Sentiment = "positive"
        Outcome = "expansion"
        Topics = @("rollout", "enablement", "governance")
        RiskFlags = @()
        NextSteps = @("Schedule admin training", "Define governance model", "Track onboarding quality")
        TranscriptExcerpt = "Customer approved rollout to new business units with shared governance."
        Transcript = @(
          @{ speaker = "Program Manager"; text = "We are ready to roll out to additional business units." },
          @{ speaker = "CSM"; text = "We will coordinate enablement and governance." }
        )
        TalkRatioRep = 0.47
        NpsScore = $null
        Lifecycle = "expansion"
      }
    }
    "expansion:4" {
      return @{
        Title = "Commercial expansion negotiation"
        Sentiment = "positive"
        Outcome = "won"
        Topics = @("procurement", "expansion", "commercials")
        RiskFlags = @()
        NextSteps = @("Finalize expansion order form", "Confirm legal redlines", "Book kickoff")
        TranscriptExcerpt = "Procurement confirmed the expansion order is ready for signature."
        Transcript = @(
          @{ speaker = "Procurement"; text = "Expansion order is ready pending final signature." },
          @{ speaker = "CSM"; text = "We will send the final order form today." }
        )
        TalkRatioRep = 0.42
        NpsScore = 10
        Lifecycle = "expansion"
      }
    }
    default {
      return @{
        Title = "Executive reference and growth planning"
        Sentiment = "positive"
        Outcome = "expansion"
        Topics = @("advocacy", "growth", "reference")
        RiskFlags = @()
        NextSteps = @("Record customer success story", "Plan next-quarter growth", "Nominate speaker for customer summit")
        TranscriptExcerpt = "Customer committed to a public success story and additional growth planning."
        Transcript = @(
          @{ speaker = "Executive Sponsor"; text = "We can share this success story publicly." },
          @{ speaker = "CSM"; text = "Great, we will coordinate growth planning and advocacy." }
        )
        TalkRatioRep = 0.41
        NpsScore = $null
        Lifecycle = "expansion"
      }
    }
  }
}

$gongEvents = @()
$accountCount = $accounts.Count

for ($i = 1; $i -le $GongCount; $i++) {
  $account = $accounts[(($i - 1) % $accountCount)]
  $cycle = [math]::Floor(($i - 1) / $accountCount) + 1
  $phase = ($cycle - 1) % 6
  $story = Get-StoryFields -Trajectory $account.Trajectory -Phase $phase

  if ($i % 6 -eq 0) {
    $timestamp = "2025-12-{0:00}T{1:00}:{2:00}:00Z" -f (($i % 28) + 1), ($i % 24), ($i % 60)
  } else {
    $timestamp = "2026-02-{0:00}T{1:00}:{2:00}:00Z" -f (($i % 22) + 1), ($i % 24), ($i % 60)
  }

  $event = [ordered]@{
    event = "call_analyzed"
    callId = "call-{0:00000}" -f $i
    eventId = "evt-{0:00000}" -f $i
    timestamp = $timestamp
    accountId = $account.Id
    accountName = $account.Name
    industry = $account.Industry
    segment = $account.Segment
    region = $account.Region
    arr = $account.Arr
    renewalWindow = $account.RenewalWindow
    lifecycle = $story.Lifecycle
    title = $story.Title
    owner = $account.Owner
    participants = @($account.Owner, "Executive Sponsor", "Product Lead")
    topics = $story.Topics
    riskFlags = $story.RiskFlags
    nextSteps = $story.NextSteps
    sentiment = $story.Sentiment
    outcome = $story.Outcome
    talkRatioRep = $story.TalkRatioRep
    transcriptExcerpt = $story.TranscriptExcerpt
    transcript = $story.Transcript
  }

  if ($null -ne $story.NpsScore) {
    $event["npsScore"] = $story.NpsScore
  }

  $gongEvents += $event
}

$gongPayload = @{ events = $gongEvents } | ConvertTo-Json -Depth 10
Set-Content -Path $gongFile -Value $gongPayload -Encoding UTF8

Write-Host "Generated $JiraCount Jira issues -> $jiraFile"
Write-Host "Generated $GongCount Gong events -> $gongFile"
