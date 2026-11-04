//! IPC commands used by the settings window (spec section 11).

use std::sync::Arc;

use serde::Serialize;
use status_core::catalog::Service;
use status_core::update::UpdateInfo;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::settings::{self, Settings, INTERVALS_MIN};
use crate::state::{Cmd, Shared};
use crate::{tray, updates};

type Shared_<'a> = State<'a, Arc<Shared>>;

#[tauri::command]
pub fn get_catalog(state: Shared_) -> Vec<Service> {
    state.services.clone()
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: Shared_) -> Settings {
    let mut s = state.settings.lock().unwrap().clone();
    // The OS is the source of truth for autostart.
    if let Ok(on) = app.autolaunch().is_enabled() {
        s.autostart = on;
    }
    s
}

#[tauri::command]
pub fn set_enabled(
    app: AppHandle,
    state: Shared_,
    service_id: String,
    enabled: bool,
) -> Result<(), String> {
    if !state.services.iter().any(|s| s.id == service_id) {
        return Err(format!("unknown service '{service_id}'"));
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.enabled.retain(|id| id != &service_id);
        if enabled {
            s.enabled.push(service_id.clone());
        }
        settings::save(&app, &s)?;
    }

    if enabled {
        // Baseline poll right away: it records current state without notifying.
        let _ = state.tx.send(Cmd::PollService(service_id));
    } else {
        // Disabling drops the service's state.
        state.runtime.lock().unwrap().remove(&service_id);
        state.diff_state.lock().unwrap().remove(&service_id);
        state.limiters.lock().unwrap().remove(&service_id);
        settings::save_state(&app, &service_id, None);
        tray::refresh(&app, state.inner());
    }
    Ok(())
}

#[tauri::command]
pub fn set_interval(app: AppHandle, state: Shared_, minutes: u32) -> Result<(), String> {
    if !INTERVALS_MIN.contains(&minutes) {
        return Err(format!("interval must be one of {INTERVALS_MIN:?}"));
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.interval_min = minutes;
        settings::save(&app, &s)?;
    }
    let _ = state.tx.send(Cmd::IntervalChanged);
    Ok(())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, state: Shared_, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())?;
    let mut s = state.settings.lock().unwrap();
    s.autostart = enabled;
    settings::save(&app, &s)
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    update: Option<UpdateInfo>,
    issue_url: String,
    releases_url: String,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle, state: Shared_) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        update: state.update.lock().unwrap().clone(),
        issue_url: updates::issue_url(),
        releases_url: updates::releases_url(),
    }
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    open_https(&app, &url)
}

/// Only https links are ever opened.
pub fn open_https(app: &AppHandle, url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https URLs can be opened".to_owned());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
