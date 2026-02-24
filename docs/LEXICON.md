# Product Language Guide

Use this language in UI and docs so the product reads like an operating brief, not internal analytics jargon.

## Who this app is for

- CS leaders: VP CS, Head of CS, Renewals leaders
- Frontline CSMs managing strategic accounts
- RevOps/Product Ops supporting process and data quality

## Writing rules

- Lead with business impact, not metric labels.
- Use plain verbs: `assign`, `escalate`, `recover`, `expand`, `verify`.
- Show decision context first: `what changed`, `why it matters`, `what to do now`.
- Keep technical IDs (`category`, `priority`, `audience`) for APIs only.

## Preferred wording

- Say: `Revenue at risk in next 90 days`
- Not: `Renewal window risk`

- Say: `Accounts that need attention this week`
- Not: `Account signal hotspots`

- Say: `Customer feedback follow-ups due`
- Not: `NPS follow-up queue`

- Say: `Records missing accountable owner`
- Not: `Ownership coverage gap`

- Say: `Evidence is too thin for decision`
- Not: `Evidence confidence gap`

- Say: `One system is dominating evidence`
- Not: `Source dependency risk`

## API taxonomy conventions

- `audience`: `manager` | `csm`
- `priority`: `low` | `medium` | `high`
- `category`: stable snake_case ID for automation and filtering
