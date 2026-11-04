//! Generic "active incidents" feed provider (RSS 2.0 / Atom). Every entry is an active
//! incident; an entry disappearing from the feed means it was resolved.

use async_trait::async_trait;
use chrono::Utc;

use super::{get_bytes, Params, Provider};
use crate::catalog::Service;
use crate::model::{Incident, IncidentStatus, IncidentUpdate, Severity, Snapshot};
use crate::{Error, Result};

pub struct RssActive;

#[async_trait]
impl Provider for RssActive {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot> {
        // `url` is the full feed URL.
        let bytes = get_bytes(http, &service.url).await?;
        parse(&bytes, &service.params)
    }
}

/// Remove HTML tags, decode a few common entities, collapse whitespace.
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// FNV-1a: stable across Rust versions (the value is persisted).
fn fnv1a(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Parse an RSS/Atom feed. No I/O; `params` is unused.
pub fn parse(bytes: &[u8], _params: &Params) -> Result<Snapshot> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| Error::Feed(e.to_string()))?;
    let feed_link = feed
        .links
        .first()
        .map(|l| l.href.clone())
        .unwrap_or_default();

    let incidents: Vec<Incident> = feed
        .entries
        .into_iter()
        .map(|e| {
            let title = e
                .title
                .map(|t| t.content.trim().to_owned())
                .unwrap_or_default();
            let link = e.links.first().map(|l| l.href.clone());
            let id = if e.id.is_empty() {
                format!("{}{}", link.as_deref().unwrap_or(""), title)
            } else {
                e.id
            };
            let summary = e
                .summary
                .map(|s| s.content)
                .or_else(|| e.content.and_then(|c| c.body))
                .unwrap_or_default();
            let body = strip_html(&summary);
            let stamp = e.updated.or(e.published);
            let update_id = match stamp {
                Some(t) => t.to_rfc3339(),
                None => fnv1a(&summary),
            };
            Incident {
                id,
                name: if title.is_empty() {
                    "Active incident".to_owned()
                } else {
                    title
                },
                status: IncidentStatus::Unknown,
                impact: Severity::Major,
                url: link.unwrap_or_else(|| feed_link.clone()),
                component_ids: Vec::new(),
                updates: vec![IncidentUpdate {
                    id: update_id,
                    status: IncidentStatus::Unknown,
                    body,
                    created_at: stamp.map(|t| t.to_rfc3339()).unwrap_or_default(),
                }],
            }
        })
        .collect();

    Ok(Snapshot {
        overall: if incidents.is_empty() {
            Severity::None
        } else {
            Severity::Major
        },
        components: Vec::new(),
        incidents,
        fetched_at: Utc::now(),
    })
}
