//! Release update check (spec section 13): on startup and every 24 h.

use std::sync::Arc;
use std::time::Duration;

use status_core::update::{self, UpdateInfo};
use tauri::AppHandle;

use crate::state::Shared;
use crate::tray;

/// Placeholder until the repository has a real owner.
pub const GITHUB_OWNER: &str = "andrewkoltsov";
pub const GITHUB_REPO: &str = "status-tray";

const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

pub fn issue_url() -> String {
    format!(
        "https://github.com/{GITHUB_OWNER}/{GITHUB_REPO}/issues/new?template=service-request.yml"
    )
}

pub fn releases_url() -> String {
    format!("https://github.com/{GITHUB_OWNER}/{GITHUB_REPO}/releases")
}

pub async fn run(app: AppHandle, shared: Arc<Shared>) {
    loop {
        match check(&shared.http).await {
            Ok(found) => {
                *shared.update.lock().unwrap() = found;
                tray::refresh(&app, &shared);
            }
            Err(e) => log::info!("update check failed: {e}"),
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

async fn check(http: &reqwest::Client) -> status_core::Result<Option<UpdateInfo>> {
    let url = update::latest_release_api_url(GITHUB_OWNER, GITHUB_REPO);
    let resp = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(status_core::Error::Status(resp.status().as_u16()));
    }
    let body = resp.bytes().await?;
    update::newer_release(&body, env!("CARGO_PKG_VERSION"))
}
