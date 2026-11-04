use status_core::diff::{diff, diff_outcome, Event, ServiceState};
use status_core::model::{ComponentStatus, IncidentStatus, Snapshot};
use status_core::provider::statuspage::parse;

fn load(name: &str) -> Snapshot {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    parse(&std::fs::read(path).unwrap()).unwrap()
}

/// State as it would be after a baseline poll of `name`.
fn baseline(name: &str) -> ServiceState {
    let (state, events) = diff(&ServiceState::default(), &load(name));
    assert!(events.is_empty());
    state
}

#[test]
fn baseline_produces_no_events_even_with_open_incident() {
    let (state, events) = diff(&ServiceState::default(), &load("incident_open"));
    assert!(events.is_empty());
    assert!(state.initialized);
    assert!(state.incidents.contains_key("inc1"));
    assert_eq!(
        state.incidents["inc1"].last_update_id.as_deref(),
        Some("upd1")
    );
}

#[test]
fn unchanged_poll_is_silent() {
    let prev = baseline("incident_open");
    let (state, events) = diff(&prev, &load("incident_open"));
    assert!(events.is_empty());
    assert_eq!(state, prev);
}

#[test]
fn incident_opened() {
    let prev = baseline("all_green");
    let (state, events) = diff(&prev, &load("incident_open"));
    // The degraded Git Operations component is covered by the incident: no ComponentChanged.
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::IncidentOpened(i) if i.id == "inc1"));
    assert!(state.incidents.contains_key("inc1"));
}

#[test]
fn incident_updated_on_new_update_id() {
    let prev = baseline("incident_open");
    let (state, events) = diff(&prev, &load("incident_updated"));
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::IncidentUpdated(i) if i.status == IncidentStatus::Identified)
    );
    assert_eq!(
        state.incidents["inc1"].last_update_id.as_deref(),
        Some("upd2")
    );

    // same snapshot again: nothing new
    let (_, again) = diff(&state, &load("incident_updated"));
    assert!(again.is_empty());
}

#[test]
fn incident_resolved_when_gone() {
    let prev = baseline("incident_updated");
    let (state, events) = diff(&prev, &load("all_green"));
    // Component recovery is covered by the resolved incident.
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::IncidentResolved { id, name } => {
            assert_eq!(id, "inc1");
            assert_eq!(name, "Disruption with some GitHub services");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(state.incidents.is_empty());
}

#[test]
fn component_change_without_incident_notifies() {
    let prev = baseline("all_green");
    let (state, events) = diff(&prev, &load("maintenance"));
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ComponentChanged { id, from, to, .. } => {
            assert_eq!(id, "c-actions");
            assert_eq!(*from, ComponentStatus::Operational);
            assert_eq!(*to, ComponentStatus::UnderMaintenance);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        state.components["c-actions"],
        ComponentStatus::UnderMaintenance
    );
}

#[test]
fn component_change_not_covered_by_incident_still_notifies() {
    // Incident covers c-git; Actions changes at the same time -> both events.
    let prev = baseline("all_green");
    let mut snap = load("incident_open");
    snap.components
        .iter_mut()
        .find(|c| c.id == "c-actions")
        .unwrap()
        .status = ComponentStatus::PartialOutage;
    let (_, events) = diff(&prev, &snap);
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], Event::IncidentOpened(_)));
    assert!(matches!(&events[1], Event::ComponentChanged { id, .. } if id == "c-actions"));
}

#[test]
fn fetch_error_keeps_state_and_emits_nothing() {
    let prev = baseline("incident_open");
    let (state, events) = diff_outcome::<&str>(&prev, Err("timeout"));
    assert_eq!(state, prev);
    assert!(events.is_empty());

    // An error before the baseline must not initialize the state.
    let fresh = ServiceState::default();
    let (state, events) = diff_outcome::<&str>(&fresh, Err("timeout"));
    assert!(!state.initialized);
    assert!(events.is_empty());
}

#[test]
fn state_survives_a_restart() {
    let prev = baseline("incident_open");
    let json = serde_json::to_string(&prev).unwrap();
    let restored: ServiceState = serde_json::from_str(&json).unwrap();
    let (_, events) = diff(&restored, &load("incident_open"));
    assert!(events.is_empty());
}

#[test]
fn error_then_recovery_reports_what_changed_meanwhile() {
    let prev = baseline("all_green");
    let (after_err, _) = diff_outcome::<&str>(&prev, Err("down"));
    let (_, events) = diff(&after_err, &load("incident_open"));
    assert!(matches!(events[0], Event::IncidentOpened(_)));
}
