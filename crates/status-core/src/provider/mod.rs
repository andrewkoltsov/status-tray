//! Providers turn a catalog entry into a normalized [`Snapshot`].

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

pub type Registry = HashMap<&'static str, Arc<dyn Provider>>;

/// Providers by catalog `type`.
pub fn registry() -> Registry {
    let mut m: Registry = HashMap::new();
    m.insert("statuspage", Arc::new(statuspage::StatusPage::default()));
    m
}

/// The catalog `type` values this build understands.
pub fn known_types() -> Vec<&'static str> {
    let mut v: Vec<_> = registry().keys().copied().collect();
    v.sort_unstable();
    v
}
