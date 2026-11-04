use status_core::catalog::{Catalog, Service};
use status_core::provider::known_types;

fn svc(id: &str, ty: &str, url: &str) -> Service {
    Service {
        id: id.into(),
        name: id.into(),
        service_type: ty.into(),
        url: url.into(),
        params: Default::default(),
    }
}

fn catalog(services: Vec<Service>) -> Catalog {
    Catalog {
        version: 1,
        services,
    }
}

#[test]
fn bundled_catalog_is_valid() {
    let c = Catalog::bundled().expect("services.json parses");
    c.validate(&known_types()).expect("services.json is valid");
}

#[test]
fn valid_catalog_passes() {
    let c = catalog(vec![svc(
        "github",
        "statuspage",
        "https://www.githubstatus.com",
    )]);
    assert!(c.validate(&known_types()).is_ok());
}

#[test]
fn rejects_duplicate_ids() {
    let c = catalog(vec![
        svc("a", "statuspage", "https://a.example.com"),
        svc("a", "statuspage", "https://b.example.com"),
    ]);
    assert!(c
        .validate(&known_types())
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
}

#[test]
fn rejects_bad_ids() {
    for id in ["", "GitHub", "git hub", "git_hub", "gîthub"] {
        let c = catalog(vec![svc(id, "statuspage", "https://a.example.com")]);
        assert!(
            c.validate(&known_types()).is_err(),
            "id {id:?} should be rejected"
        );
    }
}

#[test]
fn rejects_unknown_type_in_validation_but_skips_it_at_runtime() {
    let c = catalog(vec![
        svc("a", "statuspage", "https://a.example.com"),
        svc("b", "future-provider", "https://b.example.com"),
    ]);
    assert!(c.validate(&known_types()).is_err());
    let supported = c.supported(&known_types());
    assert_eq!(supported.len(), 1);
    assert_eq!(supported[0].id, "a");
}

#[test]
fn rejects_non_https_urls() {
    for url in [
        "http://a.example.com",
        "ftp://a.example.com",
        "a.example.com",
        "https://",
    ] {
        let c = catalog(vec![svc("a", "statuspage", url)]);
        assert!(
            c.validate(&known_types()).is_err(),
            "url {url:?} should be rejected"
        );
    }
}

#[test]
fn rejects_unsupported_version() {
    let mut c = catalog(vec![]);
    c.version = 2;
    assert!(c.validate(&known_types()).is_err());
}

fn with_params(ty: &str, params: serde_json::Value) -> Service {
    let mut s = svc("a", ty, "https://a.example.com");
    s.params = params.as_object().unwrap().clone();
    s
}

#[test]
fn params_default_to_empty_and_are_optional_in_json() {
    let c = Catalog::parse(
        r#"{"version":1,"services":[{"id":"a","name":"A","type":"statuspage","url":"https://a.example.com"}]}"#,
    )
    .unwrap();
    assert!(c.services[0].params.is_empty());
    assert!(!serde_json::to_string(&c).unwrap().contains("params"));
}

#[test]
fn known_types_include_cloud_providers() {
    let t = known_types();
    for ty in ["statuspage", "google-cloud", "aws-health", "rss-active"] {
        assert!(t.contains(&ty), "{ty} missing");
    }
}

#[test]
fn accepts_valid_params() {
    let c = catalog(vec![with_params(
        "google-cloud",
        serde_json::json!({"locations": ["europe-"]}),
    )]);
    assert!(c.validate(&known_types()).is_ok());
    let c = catalog(vec![with_params(
        "aws-health",
        serde_json::json!({"regions": ["eu-", "us-east-1"]}),
    )]);
    assert!(c.validate(&known_types()).is_ok());
}

#[test]
fn rejects_bad_params() {
    for s in [
        with_params("statuspage", serde_json::json!({"locations": ["x"]})),
        with_params("google-cloud", serde_json::json!({"regions": ["x"]})),
        with_params("google-cloud", serde_json::json!({"locations": "europe-"})),
        with_params("aws-health", serde_json::json!({"regions": [1]})),
        with_params("rss-active", serde_json::json!({"x": []})),
    ] {
        assert!(catalog(vec![s]).validate(&known_types()).is_err());
    }
}
