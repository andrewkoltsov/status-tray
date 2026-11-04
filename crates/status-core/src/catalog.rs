//! Service catalog (`services.json`).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The catalog bundled into the binary.
pub const BUNDLED: &str = include_str!("../../../services.json");

pub const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub service_type: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub services: Vec<Service>,
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn bundled() -> Result<Self> {
        Self::parse(BUNDLED)
    }

    /// Services whose `type` is in `known_types`. Others are skipped with a warning
    /// (forward compatibility with newer catalogs).
    pub fn supported(&self, known_types: &[&str]) -> Vec<Service> {
        self.services
            .iter()
            .filter(|s| {
                let ok = known_types.contains(&s.service_type.as_str());
                if !ok {
                    log::warn!(
                        "skipping service '{}': unknown type '{}'",
                        s.id,
                        s.service_type
                    );
                }
                ok
            })
            .cloned()
            .collect()
    }

    /// Strict validation (used by the unit test and CI). Returns every problem found.
    pub fn validate(&self, known_types: &[&str]) -> std::result::Result<(), Error> {
        let mut problems = Vec::new();
        if self.version != SUPPORTED_VERSION {
            problems.push(format!("unsupported version {}", self.version));
        }
        let mut seen = HashSet::new();
        for s in &self.services {
            if !is_valid_id(&s.id) {
                problems.push(format!("invalid id '{}' (want [a-z0-9-]+)", s.id));
            }
            if !seen.insert(s.id.as_str()) {
                problems.push(format!("duplicate id '{}'", s.id));
            }
            if s.name.trim().is_empty() {
                problems.push(format!("'{}': empty name", s.id));
            }
            if !known_types.contains(&s.service_type.as_str()) {
                problems.push(format!("'{}': unknown type '{}'", s.id, s.service_type));
            }
            match reqwest::Url::parse(&s.url) {
                Ok(u) if u.scheme() == "https" && u.host_str().is_some() => {}
                _ => problems.push(format!("'{}': url must be a valid https URL", s.id)),
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(Error::Catalog(problems.join("; ")))
        }
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
