use status_core::model::{ComponentStatus, IncidentStatus, Severity};
use status_core::provider::statuspage::{parse, summary_url};

fn load(name: &str) -> status_core::model::Snapshot {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    parse(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn all_green() {
    let s = load("all_green");
    assert_eq!(s.overall, Severity::None);
    assert!(s.incidents.is_empty());
    // the "Core" group container is skipped
    let names: Vec<_> = s.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Git Operations", "API Requests", "Actions"]);
    assert!(s
        .components
        .iter()
        .all(|c| c.status == ComponentStatus::Operational));
}

#[test]
fn incident_open() {
    let s = load("incident_open");
    assert_eq!(s.overall, Severity::Minor);
    assert_eq!(s.components[0].status, ComponentStatus::DegradedPerformance);
    let i = &s.incidents[0];
    assert_eq!(i.id, "inc1");
    assert_eq!(i.status, IncidentStatus::Investigating);
    assert_eq!(i.impact, Severity::Minor);
    assert_eq!(i.url, "https://stspg.io/abc123");
    assert_eq!(i.component_ids, ["c-git"]);
    assert_eq!(i.updates.len(), 1);
    assert_eq!(i.latest_update().unwrap().id, "upd1");
}

#[test]
fn incident_updated_has_newest_update_first() {
    let s = load("incident_updated");
    let i = &s.incidents[0];
    assert_eq!(i.status, IncidentStatus::Identified);
    assert_eq!(i.updates.len(), 2);
    assert_eq!(i.latest_update().unwrap().id, "upd2");
}

#[test]
fn maintenance() {
    let s = load("maintenance");
    assert_eq!(s.overall, Severity::Maintenance);
    let actions = s.components.iter().find(|c| c.id == "c-actions").unwrap();
    assert_eq!(actions.status, ComponentStatus::UnderMaintenance);
}

#[test]
fn unknown_enum_values_do_not_fail() {
    let s = load("unknown_enums");
    assert_eq!(s.overall, Severity::Unknown);
    assert_eq!(s.components[1].status, ComponentStatus::Unknown);
    let i = &s.incidents[0];
    assert_eq!(i.status, IncidentStatus::Unknown);
    assert_eq!(i.impact, Severity::Unknown);
    assert_eq!(i.latest_update().unwrap().status, IncidentStatus::Unknown);
}

#[test]
fn missing_shortlink_falls_back_to_page_url() {
    let body = br#"{"page":{"url":"https://s.example.com/"},"status":{"indicator":"minor"},
        "incidents":[{"id":"x1","name":"n","status":"investigating","impact":"minor"}]}"#;
    let s = parse(body).unwrap();
    assert_eq!(s.incidents[0].url, "https://s.example.com/incidents/x1");
}

#[test]
fn invalid_json_is_an_error() {
    assert!(parse(b"<html>not json</html>").is_err());
    assert!(parse(b"{}").is_err());
}

#[test]
fn summary_url_handles_trailing_slash() {
    assert_eq!(
        summary_url("https://a.b/"),
        "https://a.b/api/v2/summary.json"
    );
    assert_eq!(
        summary_url("https://a.b"),
        "https://a.b/api/v2/summary.json"
    );
}
