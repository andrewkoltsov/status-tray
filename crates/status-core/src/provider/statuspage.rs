//! Atlassian Statuspage provider (`/api/v2/summary.json`).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use serde::Deserialize;

use super::Provider;
use crate::catalog::Service;
use crate::model::{
    Component, ComponentStatus, Incident, IncidentStatus, IncidentUpdate, Severity, Snapshot,
};
use crate::{Error, Result};

#[derive(Debug, Clone)]
struct CacheEntry {
    etag: String,
    snapshot: Snapshot,
}

/// Fetches `{url}/api/v2/summary.json`, honouring `ETag` / `If-None-Match`.
#[derive(Default)]
pub struct StatusPage {
    cache: Mutex<HashMap<String, CacheEntry>>,
}

pub fn summary_url(base: &str) -> String {
    format!("{}/api/v2/summary.json", base.trim_end_matches('/'))
}

#[async_trait]
impl Provider for StatusPage {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot> {
        let cached = self.cache.lock().unwrap().get(&service.id).cloned();

        let mut req = http.get(summary_url(&service.url));
        if let Some(entry) = &cached {
            req = req.header(IF_NONE_MATCH, &entry.etag);
        }
        let resp = req.send().await?;

        if resp.status() == StatusCode::NOT_MODIFIED {
            return match cached {
                Some(mut entry) => {
                    entry.snapshot.fetched_at = Utc::now();
                    Ok(entry.snapshot)
                }
                None => Err(Error::Status(304)),
            };
        }
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }

        let etag = resp
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = resp.bytes().await?;
        let snapshot = parse(&bytes)?;

        let mut cache = self.cache.lock().unwrap();
        match etag {
            Some(etag) => {
                cache.insert(
                    service.id.clone(),
                    CacheEntry {
                        etag,
                        snapshot: snapshot.clone(),
                    },
                );
            }
            None => {
                cache.remove(&service.id);
            }
        }
        Ok(snapshot)
    }
}

// --- raw wire format (only the fields we use) ---

#[derive(Deserialize)]
struct RawSummary {
    #[serde(default)]
    page: RawPage,
    status: RawStatus,
    #[serde(default)]
    components: Vec<RawComponent>,
    #[serde(default)]
    incidents: Vec<RawIncident>,
}

#[derive(Deserialize, Default)]
struct RawPage {
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct RawStatus {
    indicator: String,
}

#[derive(Deserialize)]
struct RawComponent {
    id: String,
    name: String,
    status: String,
    /// Group containers have `group: true`; they are not real components.
    #[serde(default)]
    group: bool,
}

#[derive(Deserialize)]
struct RawIncident {
    id: String,
    name: String,
    status: String,
    #[serde(default)]
    impact: String,
    #[serde(default)]
    shortlink: Option<String>,
    #[serde(default)]
    components: Vec<RawIncidentComponent>,
    #[serde(default)]
    incident_updates: Vec<RawUpdate>,
}

#[derive(Deserialize)]
struct RawIncidentComponent {
    id: String,
}

#[derive(Deserialize)]
struct RawUpdate {
    id: String,
    status: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: String,
}

/// Parse a `summary.json` body. No I/O. Unknown enum values become `Unknown`.
pub fn parse(bytes: &[u8]) -> Result<Snapshot> {
    let raw: RawSummary = serde_json::from_slice(bytes)?;
    let page_url = raw.page.url.trim_end_matches('/').to_owned();

    let components = raw
        .components
        .into_iter()
        .filter(|c| !c.group)
        .map(|c| Component {
            id: c.id,
            name: c.name,
            status: ComponentStatus::from_api(&c.status),
        })
        .collect();

    let incidents = raw
        .incidents
        .into_iter()
        .map(|i| {
            let url = match i.shortlink.filter(|s| !s.is_empty()) {
                Some(link) => link,
                None => format!("{}/incidents/{}", page_url, i.id),
            };
            Incident {
                id: i.id,
                name: i.name,
                status: IncidentStatus::from_api(&i.status),
                impact: Severity::from_api(&i.impact),
                url,
                component_ids: i.components.into_iter().map(|c| c.id).collect(),
                updates: i
                    .incident_updates
                    .into_iter()
                    .map(|u| IncidentUpdate {
                        id: u.id,
                        status: IncidentStatus::from_api(&u.status),
                        body: u.body,
                        created_at: u.created_at,
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Snapshot {
        overall: Severity::from_api(&raw.status.indicator),
        components,
        incidents,
        fetched_at: Utc::now(),
    })
}
