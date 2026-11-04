//! Google Cloud provider (`{url}/incidents.json`). Open incidents only.

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;

use super::{get_bytes, string_list, Params, Provider};
use crate::catalog::Service;
use crate::model::{Incident, IncidentStatus, IncidentUpdate, Severity, Snapshot};
use crate::Result;

const SITE: &str = "https://status.cloud.google.com/";

pub struct GoogleCloud;

pub fn incidents_url(base: &str) -> String {
    format!("{}/incidents.json", base.trim_end_matches('/'))
}

#[async_trait]
impl Provider for GoogleCloud {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot> {
        let bytes = get_bytes(http, &incidents_url(&service.url)).await?;
        parse(&bytes, &service.params)
    }
}

#[derive(Deserialize)]
struct RawIncident {
    id: String,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    external_desc: String,
    #[serde(default)]
    uri: String,
    #[serde(default)]
    status_impact: String,
    #[serde(default)]
    updates: Vec<RawUpdate>,
    #[serde(default)]
    most_recent_update: Option<RawUpdate>,
    #[serde(default)]
    currently_affected_locations: Vec<RawLocation>,
}

#[derive(Deserialize, Clone)]
struct RawUpdate {
    #[serde(default)]
    created: String,
    #[serde(default)]
    modified: String,
    #[serde(default)]
    when: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct RawLocation {
    #[serde(default)]
    id: String,
}

fn impact(s: &str) -> Severity {
    match s {
        "SERVICE_OUTAGE" => Severity::Major,
        "SERVICE_DISRUPTION" => Severity::Minor,
        "SERVICE_INFORMATION" => Severity::None,
        _ => Severity::Unknown,
    }
}

fn to_update(u: RawUpdate) -> IncidentUpdate {
    let id = if u.modified.is_empty() {
        u.created.clone()
    } else {
        u.modified
    };
    let created_at = if u.when.is_empty() { u.created } else { u.when };
    IncidentUpdate {
        id,
        status: IncidentStatus::Unknown,
        body: u.text,
        created_at,
    }
}

/// Parse `incidents.json`. No I/O. Open incidents only (`end` missing or null).
/// `params.locations`: id prefixes; incidents with no current locations are global and kept.
pub fn parse(bytes: &[u8], params: &Params) -> Result<Snapshot> {
    let raw: Vec<RawIncident> = serde_json::from_slice(bytes)?;
    let prefixes = string_list(params, "locations");

    let incidents: Vec<Incident> = raw
        .into_iter()
        .filter(|i| i.end.as_deref().unwrap_or("").is_empty())
        .filter(|i| {
            prefixes.is_empty()
                || i.currently_affected_locations.is_empty()
                || i.currently_affected_locations
                    .iter()
                    .any(|l| prefixes.iter().any(|p| l.id.starts_with(p.as_str())))
        })
        .map(|i| {
            // Newest first: most_recent_update, then the rest of the history.
            let mut updates = Vec::new();
            let recent_created = i.most_recent_update.as_ref().map(|u| u.created.clone());
            if let Some(u) = i.most_recent_update {
                updates.push(to_update(u));
            }
            let mut older = i.updates;
            older.sort_by(|a, b| b.created.cmp(&a.created));
            updates.extend(
                older
                    .into_iter()
                    .filter(|u| Some(&u.created) != recent_created.as_ref())
                    .map(to_update),
            );
            Incident {
                url: format!("{SITE}{}", i.uri.trim_start_matches('/')),
                id: i.id,
                name: i.external_desc,
                status: IncidentStatus::Unknown,
                impact: impact(&i.status_impact),
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
