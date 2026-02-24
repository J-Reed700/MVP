use serde_json::Value;

#[derive(Debug, Clone)]
pub struct GongEvent {
    pub event_name: String,
    pub call_id: Option<String>,
    pub timestamp: Option<String>,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub industry: Option<String>,
    pub segment: Option<String>,
    pub region: Option<String>,
    pub arr: Option<String>,
    pub renewal_window: Option<String>,
    pub lifecycle: Option<String>,
    pub call_title: Option<String>,
    pub owner_name: Option<String>,
    pub participants: Vec<String>,
    pub transcript_excerpt: Option<String>,
    pub next_steps: Vec<String>,
    pub outcome: Option<String>,
    pub sentiment: Option<String>,
    pub risk_flags: Vec<String>,
    pub topics: Vec<String>,
    pub nps_score: Option<i64>,
    pub talk_ratio_rep: Option<f64>,
}

pub fn extract_events(payload: &Value) -> Vec<GongEvent> {
    if let Some(events) = payload.get("events").and_then(Value::as_array) {
        return events.iter().filter_map(parse_event).collect();
    }

    if let Some(items) = payload.as_array() {
        return items.iter().filter_map(parse_event).collect();
    }

    parse_event(payload).into_iter().collect()
}

fn parse_event(value: &Value) -> Option<GongEvent> {
    let object = value.as_object()?;

    let event_name = string_from_any(
        object,
        &[
            "event",
            "event_name",
            "eventType",
            "event_type",
            "type",
            "kind",
        ],
    )
    .unwrap_or_else(|| "unknown_event".to_string());

    Some(GongEvent {
        event_name,
        call_id: string_from_any(object, &["callId", "call_id", "id", "eventId", "event_id"]),
        timestamp: string_from_any(
            object,
            &["timestamp", "time", "occurredAt", "startedAt", "createdAt"],
        ),
        account_id: string_from_any(object, &["accountId", "account_id", "account"]),
        account_name: string_from_any(
            object,
            &["accountName", "account_name", "customer", "customerName"],
        ),
        industry: string_from_any(object, &["industry", "vertical"]),
        segment: string_from_any(object, &["segment", "customerSegment"]),
        region: string_from_any(object, &["region", "geo", "territory"]),
        arr: string_from_any(object, &["arr", "arrBand", "annualRecurringRevenue"]),
        renewal_window: string_from_any(object, &["renewalWindow", "renewal_window"]),
        lifecycle: string_from_any(object, &["lifecycle", "lifecycleStage"]),
        call_title: string_from_any(object, &["title", "callTitle", "call_title", "subject"]),
        owner_name: string_from_any(object, &["owner", "ownerName", "rep", "csm"]),
        participants: strings_from_any(object, &["participants", "attendees", "speakers"]),
        transcript_excerpt: transcript_excerpt_from_any(
            object,
            &["transcriptExcerpt", "transcript_excerpt", "transcriptSnippet"],
        ),
        next_steps: strings_from_any(object, &["nextSteps", "actionItems", "followUps"]),
        outcome: string_from_any(object, &["outcome", "dealOutcome", "callOutcome"]),
        sentiment: string_from_any(object, &["sentiment", "sentimentLabel"]),
        risk_flags: strings_from_any(object, &["riskFlags", "risks"]),
        topics: strings_from_any(object, &["topics", "themes"]),
        nps_score: integer_from_any(object, &["npsScore", "nps_score", "score", "rating"]),
        talk_ratio_rep: float_from_any(
            object,
            &["talkRatioRep", "talk_ratio_rep", "repTalkRatio"],
        ),
    })
}

fn string_from_any(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(boolean) => Some(boolean.to_string()),
            _ => None,
        })
    })
}

fn integer_from_any(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        })
    })
}

fn float_from_any(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.trim().parse::<f64>().ok(),
            _ => None,
        })
    })
}

fn strings_from_any(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Vec<String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            match value {
                Value::Array(items) => {
                    let values = items
                        .iter()
                        .filter_map(|item| match item {
                            Value::String(text) => Some(text.trim().to_string()),
                            Value::Number(number) => Some(number.to_string()),
                            Value::Bool(boolean) => Some(boolean.to_string()),
                            _ => None,
                        })
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>();
                    if !values.is_empty() {
                        return values;
                    }
                }
                Value::String(text) => {
                    let parts = text
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>();
                    if !parts.is_empty() {
                        return parts;
                    }
                }
                _ => {}
            }
        }
    }
    Vec::new()
}

fn transcript_excerpt_from_any(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    if let Some(value) = string_from_any(object, keys) {
        return Some(value);
    }

    for transcript_key in ["transcript", "conversation"] {
        let Some(value) = object.get(transcript_key) else {
            continue;
        };

        match value {
            Value::String(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            Value::Array(items) => {
                let lines = transcript_lines(items);
                if !lines.is_empty() {
                    return Some(lines.join(" "));
                }
            }
            Value::Object(map) => {
                for nested_key in ["segments", "utterances", "lines"] {
                    if let Some(Value::Array(items)) = map.get(nested_key) {
                        let lines = transcript_lines(items);
                        if !lines.is_empty() {
                            return Some(lines.join(" "));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    string_from_any(object, &["summary"])
}

fn transcript_lines(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Value::Object(map) => {
                let text = map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;
                let speaker = map
                    .get("speaker")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                Some(match speaker {
                    Some(name) => format!("{name}: {text}"),
                    None => text.to_string(),
                })
            }
            _ => None,
        })
        .take(4)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::extract_events;

    #[test]
    fn extracts_events_from_events_array() {
        let payload = json!({
            "events": [
                {
                    "event": "call_analyzed",
                    "callId": "call-1",
                    "accountId": "northstar_tech",
                    "accountName": "Northstar Tech",
                    "industry": "saas",
                    "segment": "enterprise",
                    "region": "na",
                    "arr": "520k",
                    "renewalWindow": "31-90",
                    "lifecycle": "adoption",
                    "title": "QBR Renewal Risk Review",
                    "participants": ["Anna CSM", "CIO"],
                    "topics": ["renewal", "adoption"],
                    "riskFlags": ["renewal_risk"],
                    "transcript": [
                        {"speaker": "Anna CSM", "text": "We need adoption recovery before renewal committee."},
                        {"speaker": "CIO", "text": "Renewal is at risk unless activation rebounds this month."}
                    ]
                },
                {"event": "call_analyzed", "callId": "call-2", "accountId": "atlas_pay"}
            ]
        });

        let events = extract_events(&payload);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_name, "call_analyzed");
        assert_eq!(events[1].call_id.as_deref(), Some("call-2"));
        assert_eq!(events[0].participants.len(), 2);
        assert_eq!(events[0].topics[0], "renewal");
        assert_eq!(events[0].industry.as_deref(), Some("saas"));
        assert_eq!(events[0].renewal_window.as_deref(), Some("31-90"));
        assert!(events[0]
            .transcript_excerpt
            .as_deref()
            .unwrap_or_default()
            .contains("Renewal is at risk"));
    }

    #[test]
    fn extracts_single_event_payload() {
        let payload =
            json!({"type": "call_analyzed", "id": "call-9", "accountId": "a-1", "npsScore": 4, "talkRatioRep": 0.71});
        let events = extract_events(&payload);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "call_analyzed");
        assert_eq!(events[0].account_id.as_deref(), Some("a-1"));
        assert_eq!(events[0].nps_score, Some(4));
        assert_eq!(events[0].talk_ratio_rep, Some(0.71));
    }
}
