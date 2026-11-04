//! Tray icon, tooltip and menu. The menu is rebuilt after every poll.

use std::sync::Arc;

use status_core::model::Severity;
use status_core::update::UpdateInfo;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

use crate::scheduler::UNREACHABLE_AFTER;
use crate::state::{Cmd, Shared};

const TRAY_ID: &str = "main";
/// Menu item ids of the form `url:<https url>` open that URL.
const URL_PREFIX: &str = "url:";

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Ok,
    Minor,
    Major,
    Unknown,
}

impl Level {
    fn icon(self) -> tauri::Result<Image<'static>> {
        let bytes: &[u8] = match self {
            Level::Ok => include_bytes!("../icons/tray/ok-64.png"),
            Level::Minor => include_bytes!("../icons/tray/minor-64.png"),
            Level::Major => include_bytes!("../icons/tray/major-64.png"),
            Level::Unknown => include_bytes!("../icons/tray/unknown-64.png"),
        };
        Image::from_bytes(bytes)
    }
}

struct Row {
    name: String,
    status: String,
    severity: Severity,
    url: String,
}

/// Everything needed to draw the tray, as plain data.
struct View {
    level: Level,
    tooltip: String,
    rows: Vec<Row>,
    last_check: Option<String>,
    update: Option<UpdateInfo>,
}

fn compute(shared: &Shared) -> View {
    let enabled = shared.enabled_services();
    let runtime = shared.runtime.lock().unwrap();

    let rows: Vec<Row> = enabled
        .iter()
        .map(|svc| {
            let unknown = |status: &str| Row {
                name: svc.name.clone(),
                status: status.to_owned(),
                severity: Severity::Unknown,
                url: svc.url.clone(),
            };
            let Some(rt) = runtime.get(&svc.id) else {
                return unknown("checking\u{2026}");
            };
            if rt.failures >= UNREACHABLE_AFTER {
                return unknown("unreachable");
            }
            let Some(snap) = &rt.snapshot else {
                return unknown("checking\u{2026}");
            };

            let affected: Vec<_> = snap
                .components
                .iter()
                .filter(|c| c.status.severity() != Severity::None)
                .collect();
            let worst_component = affected
                .iter()
                .map(|c| c.status.severity())
                .max()
                .unwrap_or(Severity::None);
            let severity = snap.overall.max(worst_component);

            let mut status = if severity == Severity::None {
                "Operational".to_owned()
            } else {
                severity.label().to_owned()
            };
            if severity != Severity::None {
                if let Some(first) = affected.first() {
                    status.push_str(&format!(": {}", first.name));
                    if affected.len() > 1 {
                        status.push_str(&format!(" +{}", affected.len() - 1));
                    }
                } else if let Some(inc) = snap.incidents.first() {
                    status.push_str(&format!(": {}", inc.name));
                }
            }
            let url = snap
                .incidents
                .first()
                .map(|i| i.url.clone())
                .unwrap_or_else(|| svc.url.clone());
            Row {
                name: svc.name.clone(),
                status,
                severity,
                url,
            }
        })
        .collect();

    let worst = rows.iter().max_by_key(|r| r.severity);
    let level = match worst.map(|r| r.severity) {
        None | Some(Severity::Unknown) => Level::Unknown,
        Some(Severity::None) => Level::Ok,
        // Maintenance is not an outage, but worth a glance.
        Some(Severity::Minor) | Some(Severity::Maintenance) => Level::Minor,
        Some(Severity::Major) | Some(Severity::Critical) => Level::Major,
    };
    let tooltip = match worst {
        None => "Status Tray \u{2014} no services enabled".to_owned(),
        Some(r) if r.severity == Severity::None => "Status Tray \u{2014} all good".to_owned(),
        Some(r) => format!("{}: {}", r.name, r.status),
    };

    View {
        level,
        tooltip,
        rows,
        last_check: shared
            .last_check
            .lock()
            .unwrap()
            .map(|t| t.format("%H:%M").to_string()),
        update: shared.update.lock().unwrap().clone(),
    }
}

fn build_menu(app: &AppHandle, view: &View) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;

    if view.rows.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "none",
            "No services enabled",
            false,
            None::<&str>,
        )?)?;
    }
    for row in &view.rows {
        let bullet = if row.severity == Severity::Unknown {
            "\u{25CB}"
        } else {
            "\u{25CF}"
        };
        let text = format!("{bullet} {} \u{2014} {}", row.name, row.status);
        let id = format!("{URL_PREFIX}{}", row.url);
        menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let last = format!(
        "Last check: {}",
        view.last_check.as_deref().unwrap_or("\u{2014}")
    );
    menu.append(&MenuItem::with_id(app, "last", last, false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "check_now",
        "Check now",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "settings",
        "Settings\u{2026}",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if let Some(u) = &view.update {
        let id = format!("{URL_PREFIX}{}", u.url);
        let text = format!("Update available: v{}", u.version);
        menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
    }
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn build(app: &AppHandle, shared: &Arc<Shared>) -> tauri::Result<()> {
    let view = compute(shared);
    let menu = build_menu(app, &view)?;
    let shared = shared.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(view.level.icon()?)
        .tooltip(&view.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| on_menu_event(app, &shared, event))
        .build(app)?;
    Ok(())
}

/// Redraw icon, tooltip and menu from the current state (callable from any thread).
pub fn refresh(app: &AppHandle, shared: &Arc<Shared>) {
    let view = compute(shared);
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else {
            return;
        };
        match build_menu(&handle, &view) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => log::warn!("tray menu: {e}"),
        }
        if let Ok(icon) = view.level.icon() {
            let _ = tray.set_icon(Some(icon));
        }
        let _ = tray.set_tooltip(Some(&view.tooltip));
    });
    if let Err(e) = result {
        log::warn!("tray refresh: {e}");
    }
}

fn on_menu_event(app: &AppHandle, shared: &Arc<Shared>, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        "check_now" => {
            let _ = shared.tx.send(Cmd::CheckNow);
        }
        "settings" => crate::show_settings(app),
        "quit" => app.exit(0),
        _ => {
            if let Some(url) = id.strip_prefix(URL_PREFIX) {
                if let Err(e) = crate::commands::open_https(app, url) {
                    log::warn!("open {url}: {e}");
                }
            }
        }
    }
}
