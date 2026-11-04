//! Polling loop: one global interval, bounded concurrency, jitter, per-service backoff.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::stream::{self, StreamExt};
use status_core::catalog::Service;
use status_core::diff;
use status_core::model::Snapshot;
use status_core::notify::{self, RateLimiter};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::settings;
use crate::state::{Cmd, ServiceRuntime, Shared};
use crate::tray;

const MAX_CONCURRENT: usize = 4;
const NOTIFY_WINDOW: Duration = Duration::from_secs(30);
/// Consecutive failures after which the tray shows "unreachable".
pub const UNREACHABLE_AFTER: u32 = 3;
/// Backoff cap, in ticks (4x the interval).
const MAX_BACKOFF_TICKS: u32 = 4;

pub async fn run(app: AppHandle, shared: Arc<Shared>, mut rx: UnboundedReceiver<Cmd>) {
    poll(&app, &shared, None, true).await;
    // Wall clock on purpose: monotonic clocks may not advance while the machine sleeps.
    let mut last_tick = SystemTime::now();

    loop {
        let interval = shared.interval();
        let mut force = false;

        tokio::select! {
            _ = tokio::time::sleep(jitter(interval)) => {}
            cmd = rx.recv() => match cmd {
                None => return,
                Some(Cmd::CheckNow) => force = true,
                Some(Cmd::PollService(id)) => {
                    poll(&app, &shared, Some(vec![id]), true).await;
                    continue;
                }
                Some(Cmd::IntervalChanged) => continue,
            },
        }

        // Best-effort wake detection: a tick that arrives far too late means we slept.
        let gap = last_tick.elapsed().unwrap_or_default();
        if gap > interval * 2 {
            log::info!("tick gap {gap:?} > 2x interval; polling immediately");
            force = true;
        }

        poll(&app, &shared, None, force).await;
        last_tick = SystemTime::now();
    }
}

/// +-10 % jitter without pulling in a RNG crate.
fn jitter(d: Duration) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.subsec_nanos())
        .unwrap_or(0);
    d.mul_f64(0.9 + 0.2 * f64::from(nanos % 1000) / 1000.0)
}

/// Poll `only` (or every enabled service). Without `force`, services in backoff are skipped.
async fn poll(app: &AppHandle, shared: &Arc<Shared>, only: Option<Vec<String>>, force: bool) {
    let targets: Vec<Service> = shared
        .enabled_services()
        .into_iter()
        .filter(|s| only.as_ref().is_none_or(|ids| ids.contains(&s.id)))
        .filter(|s| force || due(shared, &s.id))
        .collect();

    let results: Vec<(Service, status_core::Result<Snapshot>)> = stream::iter(targets)
        .map(|svc| {
            let shared = shared.clone();
            async move {
                let res = match shared.providers.get(svc.service_type.as_str()) {
                    Some(p) => p.fetch(&shared.http, &svc).await,
                    None => Err(status_core::Error::Catalog(format!(
                        "no provider for type '{}'",
                        svc.service_type
                    ))),
                };
                (svc, res)
            }
        })
        .buffer_unordered(MAX_CONCURRENT)
        .collect()
        .await;

    for (svc, res) in results {
        apply(app, shared, &svc, res);
    }
    *shared.last_check.lock().unwrap() = Some(chrono::Local::now());
    tray::refresh(app, shared);
}

/// Backoff bookkeeping: true if this tick should poll the service.
fn due(shared: &Shared, id: &str) -> bool {
    let mut rt = shared.runtime.lock().unwrap();
    match rt.get_mut(id) {
        Some(r) if r.skip_ticks > 0 => {
            r.skip_ticks -= 1;
            false
        }
        _ => true,
    }
}

/// Record one fetch outcome: runtime status, diff state, notifications.
fn apply(app: &AppHandle, shared: &Arc<Shared>, svc: &Service, res: status_core::Result<Snapshot>) {
    // The user may have disabled the service while the request was in flight.
    if !shared.is_enabled(&svc.id) {
        return;
    }

    let prev = shared
        .diff_state
        .lock()
        .unwrap()
        .get(&svc.id)
        .cloned()
        .unwrap_or_default();
    let (new_state, events) = diff::diff_outcome(&prev, res.as_ref());

    if res.is_ok() {
        settings::save_state(app, &svc.id, Some(&new_state));
        shared
            .diff_state
            .lock()
            .unwrap()
            .insert(svc.id.clone(), new_state);
    }

    {
        let mut rt = shared.runtime.lock().unwrap();
        let r: &mut ServiceRuntime = rt.entry(svc.id.clone()).or_default();
        match res {
            Ok(snapshot) => {
                r.snapshot = Some(snapshot);
                r.failures = 0;
                r.skip_ticks = 0;
            }
            Err(e) => {
                r.failures += 1;
                // 1 failure -> skip 1 tick, then 3 (cap = 4x interval).
                r.skip_ticks = 2u32.saturating_pow(r.failures).min(MAX_BACKOFF_TICKS) - 1;
                log::warn!("{}: fetch failed ({} in a row): {e}", svc.id, r.failures);
            }
        }
    }

    // Always submit (even with no events) so held notifications get flushed.
    let items = events
        .iter()
        .map(|e| notify::format(&svc.name, e))
        .collect();
    let to_show = shared
        .limiters
        .lock()
        .unwrap()
        .entry(svc.id.clone())
        .or_insert_with(|| RateLimiter::new(NOTIFY_WINDOW))
        .submit(items, Instant::now());
    if let Some(n) = to_show {
        if let Err(e) = app
            .notification()
            .builder()
            .title(n.title)
            .body(n.body)
            .show()
        {
            log::warn!("notification failed: {e}");
        }
    }
}
