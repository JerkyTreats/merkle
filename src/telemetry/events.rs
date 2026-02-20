//! Event schema for telemetry.

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub ts: String,
    pub session: String,
    pub seq: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct ProgressEnvelope {
    pub ts: String,
    pub session: String,
    pub event_type: String,
    pub data: Value,
}

impl ProgressEnvelope {
    pub fn new(ts: String, session: String, event_type: impl Into<String>, data: Value) -> Self {
        Self {
            ts,
            session,
            event_type: event_type.into(),
            data,
        }
    }

    pub fn with_now(
        session: impl Into<String>,
        event_type: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            session: session.into(),
            event_type: event_type.into(),
            data,
        }
    }
}

impl ProgressEvent {
    pub fn from_envelope(envelope: ProgressEnvelope, seq: u64) -> Self {
        Self {
            ts: envelope.ts,
            session: envelope.session,
            seq,
            event_type: envelope.event_type,
            data: envelope.data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartedData {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndedData {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEventData {
    pub node_id: String,
    pub agent_id: String,
    pub provider_name: String,
    pub frame_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatsEventData {
    pub pending: usize,
    pub processing: usize,
    pub completed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderLifecycleEventData {
    pub node_id: String,
    pub agent_id: String,
    pub provider_name: String,
    pub frame_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryEventData {
    pub command: String,
    pub ok: bool,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_round_trip() {
        let event = ProgressEvent {
            ts: "2026-02-14T12:34:56.789Z".to_string(),
            session: "s1".to_string(),
            seq: 1,
            event_type: "session_started".to_string(),
            data: json!({ "command": "scan" }),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        let parsed: ProgressEvent = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed.session, "s1");
        assert_eq!(parsed.seq, 1);
        assert_eq!(parsed.event_type, "session_started");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let raw = r#"{"ts":"2026-02-14T12:34:56.789Z","session":"s1","seq":1,"type":"session_started","data":{"command":"scan"},"future":"ok"}"#;
        let parsed: ProgressEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.session, "s1");
    }

    #[test]
    fn timestamp_is_iso_8601_with_milliseconds() {
        let env = ProgressEnvelope::with_now("s1", "session_started", json!({}));
        let parsed = chrono::DateTime::parse_from_rfc3339(&env.ts).unwrap();
        assert_eq!(env.ts.len(), 24);
        assert_eq!(env.ts.chars().nth(19), Some('.'));
        assert!(env.ts.ends_with('Z'));
        assert!(parsed.timestamp_subsec_millis() <= 999);
    }
}
