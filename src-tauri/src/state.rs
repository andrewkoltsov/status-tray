//! Shared runtime state (one `Arc<Shared>`, managed by Tauri).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Local};
use status_core::catalog::Service;
use status_core::diff::ServiceState;
use status_core::model::Snapshot;
use status_core::notify::RateLimiter;
use status_core::provider::{self, Registry};
use status_core::update::UpdateInfo;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::Settings;

/// Messages for the scheduler loop.
pub enum Cmd {
    /// Poll every enabled service now ("Check now", system wake).
    CheckNow,
    /// Poll one service now (it was just enabled; this is its baseline poll).
    PollService(String),
    /// Interval changed: restart the wait with the new value.
    IntervalChanged,
}

#[derive(Default)]
pub struct ServiceRuntime {
    pub snapshot: Option<Snapshot>,
    pub failures: u32,
    /// Ticks to skip before polling again (exponential backoff after failures).
    pub skip_ticks: u32,
}

pub struct Shared {
    /// Catalog entries with a type this build supports.
    pub services: Vec<Service>,
    pub providers: Registry,
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    pub runtime: Mutex<HashMap<String, ServiceRuntime>>,
    pub diff_state: Mutex<HashMap<String, ServiceState>>,
    pub limiters: Mutex<HashMap<String, RateLimiter>>,
    pub update: Mutex<Option<UpdateInfo>>,
    pub last_check: Mutex<Option<DateTime<Local>>>,
    pub tx: UnboundedSender<Cmd>,
}

impl Shared {
    pub fn new(
        services: Vec<Service>,
        settings: Settings,
        diff_state: HashMap<String, ServiceState>,
        tx: UnboundedSender<Cmd>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(format!("status-tray/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()
            .expect("http client");
        Shared {
            services,
            providers: provider::registry(),
            http,
            settings: Mutex::new(settings),
            runtime: Mutex::default(),
            diff_state: Mutex::new(diff_state),
            limiters: Mutex::default(),
            update: Mutex::default(),
            last_check: Mutex::default(),
            tx,
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.settings.lock().unwrap().interval_min) * 60)
    }

    /// Enabled services, in catalog order.
    pub fn enabled_services(&self) -> Vec<Service> {
        let settings = self.settings.lock().unwrap();
        self.services
            .iter()
            .filter(|s| settings.enabled.contains(&s.id))
            .cloned()
            .collect()
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.settings
            .lock()
            .unwrap()
            .enabled
            .iter()
            .any(|e| e == id)
    }
}
