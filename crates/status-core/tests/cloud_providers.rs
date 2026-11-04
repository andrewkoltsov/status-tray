use status_core::diff::{diff, Event, ServiceState};
use status_core::model::{IncidentStatus, Severity, Snapshot};
use status_core::provider::{aws_health, google_cloud, rss_active, Params};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).unwrap()
}

fn params(v: serde_json::Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

fn gcp(name: &str, p: serde_json::Value) -> Snapshot {
    google_cloud::parse(&fixture(name), &params(p)).unwrap()
}
fn aws(name: &str, p: serde_json::Value) -> Snapshot {
    aws_health::parse(&fixture(name), &params(p)).unwrap()
}
fn rss(name: &str) -> Snapshot {
    rss_active::parse(&fixture(name), &Params::new()).unwrap()
}

fn ids(s: &Snapshot) -> Vec<&str> {
    s.incidents.iter().map(|i| i.id.as_str()).collect()
}

// --- google-cloud ---

#[test]
fn gcp_only_open_incidents() {
    let s = gcp("gcp_open.json", serde_json::json!({}));
    assert_eq!(ids(&s), ["Xs2XTHzAVC9YnafnPyzj", "SyntheticAsiaOpen0001"]);
    assert!(s.components.is_empty());
    // worst of Minor (disruption) and Major (outage)
    assert_eq!(s.overall, Severity::Major);
}

#[test]
fn gcp_fields_and_update_id() {
    let s = gcp("gcp_open.json", serde_json::json!({}));
    let i = &s.incidents[0];
    assert_eq!(
        i.name,
        "Multiple services were experiencing issues in the europe-west2 region."
    );
    assert_eq!(i.impact, Severity::Minor);
    assert_eq!(
        i.url,
        "https://status.cloud.google.com/incidents/Xs2XTHzAVC9YnafnPyzj"
    );
    assert_eq!(i.latest_update().unwrap().id, "2026-10-28T00:38:39+00:00");
    // newest first, no duplicate of the most recent update
    assert_eq!(i.updates.len(), 2);
    assert_eq!(i.updates[1].id, "2026-10-27T15:13:38+00:00");
    // leading slash in uri handled
    assert_eq!(
        s.incidents[1].url,
        "https://status.cloud.google.com/incidents/SyntheticAsiaOpen0001"
    );
    assert_eq!(s.incidents[1].impact, Severity::Major);
}

#[test]
fn gcp_none_open_is_overall_none() {
    let s = gcp("gcp_none_open.json", serde_json::json!({}));
    assert!(s.incidents.is_empty());
    assert_eq!(s.overall, Severity::None);
}

#[test]
fn gcp_location_filter() {
    let eu = gcp(
        "gcp_open.json",
        serde_json::json!({"locations": ["europe-"]}),
    );
    assert_eq!(ids(&eu), ["Xs2XTHzAVC9YnafnPyzj"]);
    assert_eq!(eu.overall, Severity::Minor);
    let asia = gcp("gcp_open.json", serde_json::json!({"locations": ["asia-"]}));
    assert_eq!(ids(&asia), ["SyntheticAsiaOpen0001"]);
    let none = gcp("gcp_open.json", serde_json::json!({"locations": ["us-"]}));
    assert!(none.incidents.is_empty());
    assert_eq!(none.overall, Severity::None);
}

#[test]
fn gcp_global_incident_kept_by_any_filter() {
    let json = br#"[{"id":"g1","end":null,"external_desc":"x","uri":"incidents/g1",
        "status_impact":"SERVICE_OUTAGE","currently_affected_locations":[]}]"#;
    let p = params(serde_json::json!({"locations": ["europe-"]}));
    let s = google_cloud::parse(json, &p).unwrap();
    assert_eq!(ids(&s), ["g1"]);
    // no most_recent_update: no updates, still fine
    assert!(s.incidents[0].updates.is_empty());
}

#[test]
fn gcp_impact_mapping_and_unknown() {
    let json = br#"[
      {"id":"a","external_desc":"a","uri":"/incidents/a","status_impact":"SERVICE_OUTAGE"},
      {"id":"b","external_desc":"b","uri":"/incidents/b","status_impact":"SERVICE_DISRUPTION"},
      {"id":"c","external_desc":"c","uri":"/incidents/c","status_impact":"SERVICE_INFORMATION"},
      {"id":"d","external_desc":"d","uri":"/incidents/d","status_impact":"SOMETHING_NEW"}]"#;
    let s = google_cloud::parse(json, &Params::new()).unwrap();
    let impacts: Vec<_> = s.incidents.iter().map(|i| i.impact).collect();
    assert_eq!(
        impacts,
        [
            Severity::Major,
            Severity::Minor,
            Severity::None,
            Severity::Unknown
        ]
    );
    assert_eq!(s.incidents.len(), 4); // information-only still listed
    assert_eq!(s.overall, Severity::Major);
}

#[test]
fn gcp_update_id_falls_back_to_created() {
    let json = br#"[{"id":"a","external_desc":"a","uri":"x","status_impact":"X",
        "most_recent_update":{"created":"2026-01-01T00:00:00+00:00","text":"t"}}]"#;
    let s = google_cloud::parse(json, &Params::new()).unwrap();
    assert_eq!(
        s.incidents[0].latest_update().unwrap().id,
        "2026-01-01T00:00:00+00:00"
    );
}

#[test]
fn gcp_open_to_resolved_and_edit_detected() {
    let (st, ev) = diff(
        &ServiceState::default(),
        &gcp("gcp_open.json", serde_json::json!({})),
    );
    assert!(ev.is_empty());
    let (st, ev) = diff(&st, &gcp("gcp_none_open.json", serde_json::json!({})));
    assert_eq!(ev.len(), 2);
    assert!(ev
        .iter()
        .all(|e| matches!(e, Event::IncidentResolved { .. })));
    assert!(st.incidents.is_empty());

    // an edit (new `modified`) is an update
    let before = gcp("gcp_open.json", serde_json::json!({}));
    let mut after = before.clone();
    after.incidents[0].updates[0].id = "2026-11-01T00:00:00+00:00".into();
    let (st, _) = diff(&ServiceState::default(), &before);
    let (_, ev) = diff(&st, &after);
    assert!(matches!(ev.as_slice(), [Event::IncidentUpdated(i)] if i.id == "Xs2XTHzAVC9YnafnPyzj"));
}

// --- aws-health ---

#[test]
fn aws_parses_open_events() {
    let s = aws("aws_currentevents.json", serde_json::json!({}));
    assert_eq!(s.incidents.len(), 2);
    assert_eq!(s.overall, Severity::Critical);
    let i = &s.incidents[0];
    assert!(i.id.starts_with("arn:aws:health:me-central-1::event/"));
    assert_eq!(i.name, "Multiple services (UAE): Region Availability");
    assert_eq!(i.impact, Severity::Critical);
    assert_eq!(i.url, "https://health.aws.amazon.com/health/status");
    assert_eq!(i.status, IncidentStatus::Unknown);
}

#[test]
fn aws_updates_newest_first_with_timestamp_ids() {
    let s = aws("aws_currentevents.json", serde_json::json!({}));
    let ids: Vec<_> = s.incidents[0]
        .updates
        .iter()
        .map(|u| u.id.as_str())
        .collect();
    assert_eq!(ids, ["1789468030", "1777533954", "1772554485"]);
    assert_eq!(s.incidents[0].latest_update().unwrap().id, "1789468030");
    assert!(!s.incidents[0].updates[0].body.is_empty());
}

#[test]
fn aws_region_filter() {
    let me = aws(
        "aws_currentevents.json",
        serde_json::json!({"regions": ["me-south-1"]}),
    );
    assert_eq!(me.incidents.len(), 1);
    assert!(me.incidents[0].id.contains("me-south-1"));
    let prefix = aws(
        "aws_currentevents.json",
        serde_json::json!({"regions": ["me-"]}),
    );
    assert_eq!(prefix.incidents.len(), 2);
    let eu = aws(
        "aws_currentevents.json",
        serde_json::json!({"regions": ["eu-"]}),
    );
    assert!(eu.incidents.is_empty());
    assert_eq!(eu.overall, Severity::None);
}

#[test]
fn aws_status_codes_and_unparsable_region() {
    let json = br#"[
      {"arn":"arn:aws:health:us-east-1::event/A","status":"0","service_name":"S","region_name":"R","summary":"x"},
      {"arn":"arn:aws:health:us-east-1::event/B","status":"1","service_name":"S","region_name":"R","summary":"x"},
      {"arn":"arn:aws:health:us-east-1::event/C","status":"2","service_name":"S","region_name":"R","summary":"x"},
      {"arn":"arn:aws:health:us-east-1::event/D","status":"3","service_name":"S","region_name":"R","summary":"x"},
      {"arn":"arn:aws:health:us-east-1::event/E","status":"9","service_name":"S","region_name":"R","summary":"x"},
      {"arn":"garbage","status":"2","service_name":"S","region_name":"R","summary":"x"}]"#;
    let p = params(serde_json::json!({"regions": ["eu-"]}));
    let s = aws_health::parse(json, &p).unwrap();
    assert_eq!(ids(&s), ["garbage"]); // only the unparsable one survives the eu- filter

    let s = aws_health::parse(json, &Params::new()).unwrap();
    let got: Vec<_> = s
        .incidents
        .iter()
        .map(|i| (i.id.as_str(), i.impact))
        .collect();
    assert_eq!(
        got,
        [
            ("arn:aws:health:us-east-1::event/B", Severity::Minor),
            ("arn:aws:health:us-east-1::event/C", Severity::Major),
            ("arn:aws:health:us-east-1::event/D", Severity::Critical),
            ("arn:aws:health:us-east-1::event/E", Severity::Unknown),
            ("garbage", Severity::Major),
        ]
    ); // status "0" (resolved) is dropped
}

#[test]
fn aws_empty_list_is_overall_none() {
    let s = aws_health::parse(b"[]", &Params::new()).unwrap();
    assert!(s.incidents.is_empty());
    assert_eq!(s.overall, Severity::None);
}

#[test]
fn aws_decodes_utf16_with_bom() {
    let utf16 = fixture("aws_currentevents_utf16.bin");
    assert_eq!(&utf16[..2], [0xFF, 0xFE]);
    let a = aws_health::parse(&utf16, &Params::new()).unwrap();
    let b = aws("aws_currentevents.json", serde_json::json!({}));
    assert_eq!(a.incidents, b.incidents);
}

#[test]
fn aws_decode_variants() {
    let text = r#"[{"arn":"arn:aws:health:eu-west-1::event/X","status":"2","service_name":"S","region_name":"Ireland","summary":"Zażółć"}]"#;
    let le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let with_bom = |bom: [u8; 2], body: &[u8]| [bom.as_slice(), body].concat();
    for bytes in [
        le.clone(),                                          // LE, no BOM
        be.clone(),                                          // BE, no BOM
        with_bom([0xFF, 0xFE], &le),                         // LE BOM
        with_bom([0xFE, 0xFF], &be),                         // BE BOM
        text.as_bytes().to_vec(),                            // plain UTF-8
        [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat(), // UTF-8 BOM
    ] {
        let s = aws_health::parse(&bytes, &Params::new()).unwrap();
        assert_eq!(s.incidents[0].name, "S (Ireland): Zażółć");
    }
}

#[test]
fn aws_open_to_resolved() {
    let (st, ev) = diff(
        &ServiceState::default(),
        &aws("aws_currentevents.json", serde_json::json!({})),
    );
    assert!(ev.is_empty());
    let empty = aws_health::parse(b"[]", &Params::new()).unwrap();
    let (st, ev) = diff(&st, &empty);
    assert_eq!(ev.len(), 2);
    assert!(ev
        .iter()
        .all(|e| matches!(e, Event::IncidentResolved { .. })));
    assert!(st.incidents.is_empty());
}

// --- rss-active ---

#[test]
fn rss_empty_feed_is_overall_none() {
    let s = rss("azure_empty.xml");
    assert!(s.incidents.is_empty());
    assert!(s.components.is_empty());
    assert_eq!(s.overall, Severity::None);
}

#[test]
fn rss_active_items() {
    let s = rss("azure_active.xml");
    assert_eq!(s.overall, Severity::Major);
    assert_eq!(s.incidents.len(), 2);
    let i = &s.incidents[0];
    assert_eq!(i.id, "AZFD-1A2B");
    assert_eq!(i.name, "Azure Front Door - Connectivity issues");
    assert_eq!(i.impact, Severity::Major);
    assert_eq!(i.url, "https://azure.status.microsoft/en-us/status/#azfd-1");
    let u = i.latest_update().unwrap();
    assert!(u.id.starts_with("2026-11-04T12:15:00"), "{}", u.id);
    assert_eq!(
        u.body,
        "Impact Statement: Starting at 11:50 UTC, customers may experience timeouts & errors. We are investigating."
    );
}

#[test]
fn rss_item_without_date_uses_summary_hash() {
    let s = rss("azure_active.xml");
    let a = s.incidents[1].latest_update().unwrap().id.clone();
    assert_eq!(a.len(), 16);
    assert_eq!(
        a,
        rss("azure_active.xml").incidents[1]
            .latest_update()
            .unwrap()
            .id
    );
    assert_eq!(
        s.incidents[1].updates[0].body,
        "Some customers may be unable to start VMs."
    );
}

#[test]
fn rss_atom_and_missing_guid_and_link() {
    let atom = br#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"><title>t</title>
      <id>urn:feed</id><updated>2026-01-01T00:00:00Z</updated><link href="https://x.example/feed-page"/>
      <entry><id>e1</id><title>Outage</title><updated>2026-01-01T01:00:00Z</updated><summary>hi &lt;b&gt;there&lt;/b&gt;</summary></entry></feed>"#;
    let s = rss_active::parse(atom, &Params::new()).unwrap();
    assert_eq!(s.incidents[0].id, "e1");
    assert_eq!(s.incidents[0].url, "https://x.example/feed-page");
    assert_eq!(s.incidents[0].updates[0].body, "hi there");

    let rss_no_guid =
        br#"<rss version="2.0"><channel><title>t</title><link>https://x.example/</link>
      <item><title>T1</title><link>https://x.example/1</link></item></channel></rss>"#;
    let s = rss_active::parse(rss_no_guid, &Params::new()).unwrap();
    assert!(!s.incidents[0].id.is_empty());
    assert_eq!(s.incidents[0].url, "https://x.example/1");
}

#[test]
fn rss_garbage_is_an_error_not_a_panic() {
    assert!(rss_active::parse(b"not xml", &Params::new()).is_err());
}

#[test]
fn rss_open_to_resolved() {
    let (st, ev) = diff(&ServiceState::default(), &rss("azure_active.xml"));
    assert!(ev.is_empty());
    let (st, ev) = diff(&st, &rss("azure_empty.xml"));
    assert_eq!(ev.len(), 2);
    assert!(ev
        .iter()
        .all(|e| matches!(e, Event::IncidentResolved { .. })));
    assert!(st.incidents.is_empty());
}
