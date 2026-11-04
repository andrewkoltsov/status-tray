//! Providers turn a catalog entry into a normalized [`Snapshot`].

pub mod aws_health;
pub mod google_cloud;
pub mod rss_active;
pub mod statuspage;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::catalog::Service;
use crate::model::Snapshot;
use crate::Result;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot>;
}

/// Provider options from the catalog (`params`).
pub type Params = serde_json::Map<String, serde_json::Value>;

/// A param that should be a list of strings; anything else yields an empty list.
pub(crate) fn string_list(params: &Params, key: &str) -> Vec<String> {
    params
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Plain GET returning the body of a 2xx response.
pub(crate) async fn get_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = http.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(crate::Error::Status(resp.status().as_u16()));
    }
    Ok(resp.bytes().await?.to_vec())
}

pub type Registry = HashMap<&'static str, Arc<dyn Provider>>;

/// Providers by catalog `type`.
pub fn registry() -> Registry {
    let mut m: Registry = HashMap::new();
    m.insert("statuspage", Arc::new(statuspage::StatusPage::default()));
    m.insert("google-cloud", Arc::new(google_cloud::GoogleCloud));
    m.insert("aws-health", Arc::new(aws_health::AwsHealth));
    m.insert("rss-active", Arc::new(rss_active::RssActive));
    m
}

/// The catalog `type` values this build understands.
pub fn known_types() -> Vec<&'static str> {
    let mut v: Vec<_> = registry().keys().copied().collect();
    v.sort_unstable();
    v
}
