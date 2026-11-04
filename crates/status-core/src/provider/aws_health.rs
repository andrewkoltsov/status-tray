//! AWS Health provider (`{url}/public/currentevents`). The body is UTF-16 JSON.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::{get_bytes, string_list, Params, Provider};
use crate::catalog::Service;
use crate::model::{Incident, IncidentStatus, IncidentUpdate, Severity, Snapshot};
use crate::Result;

const STATUS_PAGE: &str = "https://health.aws.amazon.com/health/status";

pub struct AwsHealth;

pub fn events_url(base: &str) -> String {
    format!("{}/public/currentevents", base.trim_end_matches('/'))
}

#[async_trait]
impl Provider for AwsHealth {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot> {
        let bytes = get_bytes(http, &events_url(&service.url)).await?;
        parse(&bytes, &service.params)
    }
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    arn: String,
    #[serde(default)]
    region_name: String,
    /// "0" resolved, "1" informational, "2" degradation, "3" disruption (string or number).
    #[serde(default)]
    status: Value,
    #[serde(default)]
    service_name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    event_log: Vec<RawLog>,
}

#[derive(Deserialize)]
struct RawLog {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    status: Value,
    #[serde(default)]
    timestamp: Value,
}

fn code(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Decode the response body: UTF-16 (BOM or BOM-less heuristic), falling back to UTF-8.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        // JSON starts with an ASCII char, so a NUL in the first or second byte means UTF-16.
        [a, 0, ..] if *a != 0 => utf16(bytes, true),
        [0, b, ..] if *b != 0 => utf16(bytes, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn impact(status: &str) -> Severity {
    match status {
        "1" => Severity::Minor,
        "2" => Severity::Major,
        "3" => Severity::Critical,
        _ => Severity::Unknown,
    }
}

/// Region code from `arn:aws:health:<region>::event/...`; `None` if absent.
fn arn_region(arn: &str) -> Option<&str> {
    arn.split(':').nth(3).filter(|r| !r.is_empty())
}

fn rfc3339(ts: &Value) -> String {
    ts.as_i64()
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| code(ts))
}

/// Parse a `currentevents` body (raw bytes, any supported encoding). No I/O.
/// `params.regions`: region-code prefixes; events with no parsable region are kept.
pub fn parse(bytes: &[u8], params: &Params) -> Result<Snapshot> {
    let raw: Vec<RawEvent> = serde_json::from_str(decode(bytes).trim_start_matches('\u{feff}'))?;
    let prefixes = string_list(params, "regions");

    let incidents: Vec<Incident> = raw
        .into_iter()
        .filter(|e| code(&e.status) != "0")
        .filter(|e| match arn_region(&e.arn) {
            Some(r) => prefixes.is_empty() || prefixes.iter().any(|p| r.starts_with(p.as_str())),
            None => true,
        })
        .map(|e| {
            let mut log = e.event_log;
            // Newest first, as the model expects.
            log.sort_by_key(|l| std::cmp::Reverse(l.timestamp.as_i64().unwrap_or(0)));
            let updates = log
                .into_iter()
                .map(|l| IncidentUpdate {
                    id: code(&l.timestamp),
                    status: if code(&l.status) == "0" {
                        IncidentStatus::Resolved
                    } else {
                        IncidentStatus::Unknown
                    },
                    body: if l.message.is_empty() {
                        l.summary
                    } else {
                        l.message
                    },
                    created_at: rfc3339(&l.timestamp),
                })
                .collect();
            Incident {
                id: e.arn,
                name: format!("{} ({}): {}", e.service_name, e.region_name, e.summary),
                status: IncidentStatus::Unknown,
                impact: impact(&code(&e.status)),
                url: STATUS_PAGE.to_owned(),
                component_ids: Vec::new(),
                updates,
            }
        })
        .collect();

    Ok(Snapshot {
        overall: incidents
            .iter()
            .map(|i| i.impact)
            .max()
            .unwrap_or(Severity::None),
        components: Vec::new(),
        incidents,
        fetched_at: Utc::now(),
    })
}
