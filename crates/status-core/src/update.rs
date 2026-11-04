//! Release update check (pure part): URL + parse/compare.

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// Version string without a leading `v`.
    pub version: String,
    pub url: String,
}

pub fn latest_release_api_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// `Some` only if the latest release is a newer semver than `current`.
/// Tags that are not valid semver are ignored.
pub fn newer_release(body: &[u8], current: &str) -> Result<Option<UpdateInfo>> {
    let rel: Release = serde_json::from_slice(body)?;
    let tag = rel.tag_name.trim_start_matches('v');
    let (Ok(latest), Ok(current)) = (semver::Version::parse(tag), semver::Version::parse(current))
    else {
        return Ok(None);
    };
    Ok((latest > current).then(|| UpdateInfo {
        version: latest.to_string(),
        url: rel.html_url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str =
        r#"{"tag_name":"v0.3.0","html_url":"https://github.com/o/r/releases/tag/v0.3.0"}"#;

    #[test]
    fn newer_is_reported() {
        let u = newer_release(BODY.as_bytes(), "0.2.9").unwrap().unwrap();
        assert_eq!(u.version, "0.3.0");
    }

    #[test]
    fn same_or_older_is_not() {
        assert!(newer_release(BODY.as_bytes(), "0.3.0").unwrap().is_none());
        assert!(newer_release(BODY.as_bytes(), "1.0.0").unwrap().is_none());
    }

    #[test]
    fn garbage_tag_is_ignored() {
        let b = br#"{"tag_name":"nightly","html_url":"x"}"#;
        assert!(newer_release(b, "0.1.0").unwrap().is_none());
    }
}
