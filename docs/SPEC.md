# Status Tray — Specification (MVP)

Working name: **Status Tray** (`status-tray`). Rename is cheap until the first release.

## 1. Goal

A small cross-platform tray app that watches public status pages (GitHub, Cloudflare, OpenAI, …)
and raises a desktop notification when something changes — so you learn GitHub is down
*before* your push hangs.

Non-goals (MVP): monitoring your own services, uptime pings, history/graphs, accounts, sync,
mobile.

## 2. Platforms & stack

- macOS (arm64 + x64), Windows (x64), Linux (AppImage/deb, best effort).
- **Tauri 2** (Rust). No Node toolchain: the UI is plain HTML/CSS/JS served from `ui/`,
  using `window.__TAURI__` (`app.withGlobalTauri = true`).
- Plugins: `tauri-plugin-notification`, `tauri-plugin-autostart`, `tauri-plugin-store`,
  `tauri-plugin-opener`, `tauri-plugin-single-instance`.
- HTTP: `reqwest` (rustls). Async: `tokio` (Tauri's runtime).

## 3. Repository layout

```
status-tray/
├── Cargo.toml                 # workspace: crates/status-core, src-tauri
├── services.json              # service catalog (data, not code)
├── crates/status-core/        # pure logic, no Tauri deps, fully unit-tested
│   ├── src/model.rs           # Snapshot, Component, Incident, Severity
│   ├── src/catalog.rs         # parse/validate services.json
│   ├── src/provider/mod.rs    # Provider trait + registry by `type`
│   ├── src/provider/statuspage.rs
│   ├── src/diff.rs            # old state + new snapshot -> Vec<Event>
│   └── tests/fixtures/*.json  # real summary.json samples
├── src-tauri/                 # app shell: tray, scheduler, notifications, settings, IPC
├── ui/                        # index.html, app.js, style.css (settings window)
├── docs/SPEC.md
└── .github/
    ├── ISSUE_TEMPLATE/service-request.yml
    └── workflows/ci.yml, release.yml
```

## 4. Service catalog

`services.json` is bundled into the binary (`include_str!`). Adding a service built on a
supported provider = one JSON entry + PR, no code.

```json
{
  "version": 1,
  "services": [
    { "id": "github", "name": "GitHub", "type": "statuspage",
      "url": "https://www.githubstatus.com" }
  ]
}
```

- `id`: unique, `[a-z0-9-]+`, stable (used as settings/state key).
- `type`: provider id; unknown types are skipped with a log warning (forward compatibility).
- Catalog validation runs in a unit test (unique ids, known types, https URLs).
- Initial catalog: GitHub, Cloudflare, OpenAI, Anthropic, Atlassian/Bitbucket, Discord, npm,
  Vercel, DigitalOcean, Linear, Sentry, CircleCI — each entry verified to serve
  `/api/v2/summary.json` before it lands.
- Later (not MVP): optional remote catalog refresh from the repo's raw `services.json`.

## 5. Providers

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn fetch(&self, http: &reqwest::Client, service: &Service) -> Result<Snapshot>;
}
```

Registry: `HashMap<&'static str, Arc<dyn Provider>>`, key = catalog `type`.
Parsing is separated from I/O (`fn parse(bytes) -> Result<Snapshot>`) so tests use fixtures.

### 5.1 `statuspage` (Atlassian Statuspage) — MVP

- `GET {url}/api/v2/summary.json`
- Maps: `status.indicator` (none|minor|major|critical) → page severity;
  `components[]` (skip group containers; keep `operational|degraded_performance|partial_outage|major_outage|under_maintenance`);
  `incidents[]` (unresolved) with `id, name, status, impact, shortlink, incident_updates[]`.
- Unknown enum values → `Unknown`, never a parse failure.

### 5.2 Future providers (not MVP)

`rss`/`atom` generic fallback, `incidentio`, `google-cloud`, `aws-health`.

## 6. Normalized model

```
Snapshot   { overall: Severity, components: Vec<Component>, incidents: Vec<Incident>, fetched_at }
Component  { id, name, status: ComponentStatus }
Incident   { id, name, status: IncidentStatus, impact: Severity, url, updates: Vec<IncidentUpdate> }
IncidentUpdate { id, status, body, created_at }
Severity   = None | Minor | Major | Critical | Maintenance | Unknown   (Ord for "worst")
```

## 7. Change detection (diff)

Persisted per service: `{ incidents: {id -> {status, last_update_id}}, components: {id -> status}, initialized: bool }`.

Events produced by `diff(prev_state, snapshot) -> (new_state, Vec<Event>)`:

| Event | Notify |
|---|---|
| `IncidentOpened` (new id) | yes |
| `IncidentUpdated` (new latest update id) | yes |
| `IncidentResolved` (id gone from unresolved list) | yes |
| `ComponentChanged` (status changed) | only if no incident covers it in the same poll (avoid double notifications) |

Rules:
- **Baseline**: first successful poll of a service (`initialized == false`) produces no events.
  Same when the user enables a service in settings.
- Disabling a service drops its state.
- A fetch error never produces incident/resolve events and does not touch state.

## 8. Scheduler

- One global interval (setting): 1, 2, 5, 10, 15 min; default **2 min**.
- Tick → poll all enabled services concurrently (bounded, e.g. 4 at a time), 15 s timeout each,
  ±10 % jitter.
- "Check now" from the tray triggers an immediate poll.
- On system wake / network back (best effort: detect a tick gap > 2× interval) → poll immediately.
- Errors: exponential backoff per service (cap = 4× interval). After 3 consecutive failures the
  service shows "unreachable" in the tray menu. No notification for that (your own network is the
  usual culprit).
- HTTP: `User-Agent: status-tray/<version>`, honour `ETag`/`If-None-Match` when the server sends one.

## 9. Notifications

- Title: `"{Service}: {incident name}"`; body: latest update status + first ~140 chars of body.
- Resolved: `"{Service}: resolved — {incident name}"`.
- Component-only change: `"{Service}: {component} — {status}"`.
- Rate limit: max 1 notification per service per 30 s; extra events are merged
  (`"+N more updates"`).
- Known limitation: click-to-open is not reliable across desktops with the Tauri notification
  plugin, so incident links live in the tray menu (§10). Revisit after MVP.

## 10. Tray

- Icon reflects the **worst** severity among enabled services:
  OK (green/neutral), Minor (yellow), Major/Critical (red), Unknown/unreachable (grey).
  macOS: template-style monochrome + badge variant; ship PNGs for each state.
- Tooltip: `"Status Tray — all good"` / `"GitHub: Major outage"`.
- Menu (rebuilt after every poll):
  ```
  ● GitHub — Degraded: Git Operations      → opens latest incident or status page
  ● Cloudflare — Operational               → opens status page
  ○ Linear — unreachable
  ─────────
  Last check: 11:52
  Check now
  Settings…
  ─────────
  Update available: v0.3.0                 (only if newer release exists)
  Quit
  ```
- macOS: no Dock icon (`ActivationPolicy::Accessory`).
- Single instance: second launch focuses the settings window.

## 11. Settings window

Hidden at start; opened from tray. Closing hides it (app keeps running).

- Search box + list of catalog services with checkboxes (name + domain).
- Interval select.
- "Launch at login" toggle (autostart plugin).
- Footer: app version, "Missing a service? Request it" → opens
  `https://github.com/OWNER/status-tray/issues/new?template=service-request.yml`,
  "Releases" link, update notice if any.
- Changes apply immediately (no Save button).

IPC commands: `get_catalog`, `get_settings`, `set_enabled(service_id, bool)`,
`set_interval(minutes)`, `set_autostart(bool)`, `get_app_info` (version, update info),
`open_url(url)`.

## 12. Persistence

`tauri-plugin-store`, two files in the app data dir:
- `settings.json`: `{ enabled: [ids], interval_min, autostart }`. Default enabled: `["github"]`.
- `state.json`: diff state per service (§7).

## 13. Updates

On startup and every 24 h: `GET https://api.github.com/repos/OWNER/status-tray/releases/latest`,
compare semver with the app version, show notice in tray + settings. No auto-install in MVP.

## 14. CI / release

- `ci.yml` (push/PR): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test -p status-core`,
  `cargo check -p status-tray` on ubuntu (with webkit2gtk deps).
- `release.yml` (tag `v*`): `tauri-apps/tauri-action` matrix macOS (arm64, x64), Windows, Ubuntu →
  draft GitHub Release with dmg / msi / AppImage+deb.
- Unsigned builds for now (Gatekeeper / SmartScreen warnings documented in README).

## 15. Testing

- `status-core`: parser tests on real fixtures (all-green, incident open, incident updated,
  resolved, maintenance, unknown enum values); diff tests for every row in §7 incl. baseline
  and error paths; catalog validation test.
- App shell: manual checklist in README (tray states, notification, settings persistence,
  autostart).

## 16. MVP acceptance

1. Enable GitHub, set 1 min. When githubstatus.com opens an incident, a notification appears
   within ~1 interval, and the tray turns yellow/red.
2. Each new incident update → one notification; resolution → one notification; tray returns to OK.
3. Restarting the app does not re-notify about already-seen incidents.
4. Settings persist across restarts; autostart works on macOS and Windows.

## 17. Open questions

- App name / bundle id / GitHub owner (placeholders: `status-tray`, `OWNER`).
- Per-service component filter (e.g. only GitHub "Git Operations", "Actions") — likely v0.2.
- Git pre-push hook companion (reads the app's state file or hits the API directly) — v0.2+.
