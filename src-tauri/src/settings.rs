//! Persistence via tauri-plugin-store: `settings.json` and `state.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use status_core::diff::ServiceState;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const SETTINGS_FILE: &str = "settings.json";
const STATE_FILE: &str = "state.json";

pub const INTERVALS_MIN: [u32; 5] = [1, 2, 5, 10, 15];
pub const DEFAULT_INTERVAL_MIN: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: Vec<String>,
    pub interval_min: u32,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: vec!["github".to_owned()],
            interval_min: DEFAULT_INTERVAL_MIN,
            autostart: false,
        }
    }
}

pub fn load(app: &AppHandle) -> Settings {
    let defaults = Settings::default();
    let Ok(store) = app.store(SETTINGS_FILE) else {
        return defaults;
    };
    let enabled = store
        .get("enabled")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(defaults.enabled);
    let interval_min = store
        .get("interval_min")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .filter(|v| INTERVALS_MIN.contains(v))
        .unwrap_or(defaults.interval_min);
    let autostart = store
        .get("autostart")
        .and_then(|v| v.as_bool())
        .unwrap_or(defaults.autostart);
    Settings {
        enabled,
        interval_min,
        autostart,
    }
}

pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    store.set("enabled", serde_json::json!(s.enabled));
    store.set("interval_min", serde_json::json!(s.interval_min));
    store.set("autostart", serde_json::json!(s.autostart));
    store.save().map_err(|e| e.to_string())
}

pub fn load_states(app: &AppHandle) -> HashMap<String, ServiceState> {
    let Ok(store) = app.store(STATE_FILE) else {
        return HashMap::new();
    };
    store
        .entries()
        .into_iter()
        .filter_map(|(id, v)| Some((id, serde_json::from_value(v).ok()?)))
        .collect()
}

/// `Some(state)` writes it, `None` drops the service's state.
pub fn save_state(app: &AppHandle, id: &str, state: Option<&ServiceState>) {
    let Ok(store) = app.store(STATE_FILE) else {
        return;
    };
    match state {
        Some(s) => store.set(id, serde_json::json!(s)),
        None => {
            store.delete(id);
        }
    }
    if let Err(e) = store.save() {
        log::warn!("could not save {STATE_FILE}: {e}");
    }
}
