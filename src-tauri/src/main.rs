// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod scheduler;
mod settings;
mod state;
mod tray;
mod updates;

use std::sync::Arc;

use status_core::catalog::Catalog;
use status_core::provider;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;

use crate::state::Shared;

/// Show (and focus) the hidden settings window.
pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        // Must be registered first: a second launch just shows the settings window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_catalog,
            commands::get_settings,
            commands::set_enabled,
            commands::set_interval,
            commands::set_autostart,
            commands::get_app_info,
            commands::open_url,
        ])
        .on_window_event(|window, event| {
            // Closing the settings window only hides it; the app lives in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // No Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();

            let catalog = Catalog::bundled()?;
            let services = catalog.supported(&provider::known_types());
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let shared = Arc::new(Shared::new(
                services,
                settings::load(&handle),
                settings::load_states(&handle),
                tx,
            ));
            app.manage(shared.clone());

            tray::build(&handle, &shared)?;

            // macOS asks for permission on first use; elsewhere this is a no-op.
            if let Ok(tauri_plugin_notification::PermissionState::Prompt) =
                handle.notification().permission_state()
            {
                let _ = handle.notification().request_permission();
            }

            tauri::async_runtime::spawn(scheduler::run(handle.clone(), shared.clone(), rx));
            tauri::async_runtime::spawn(updates::run(handle, shared));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Status Tray");
}
