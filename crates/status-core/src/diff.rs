//! Change detection: previous persisted state + new snapshot -> new state + events.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{ComponentStatus, Incident, IncidentStatus, Snapshot};

/// What we remember about an open incident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentState {
    /// Kept so a "resolved" notification can name the incident after it left the API.
    #[serde(default)]
    pub name: String,
    pub status: IncidentStatus,
    pub last_update_id: Option<String>,
    #[serde(default)]
    pub component_ids: Vec<String>,
}

/// Persisted per service (`state.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceState {
    pub incidents: BTreeMap<String, IncidentState>,
    pub components: BTreeMap<String, ComponentStatus>,
    pub initialized: bool,
}

impl ServiceState {
    fn from_snapshot(snap: &Snapshot) -> Self {
        ServiceState {
            incidents: snap
                .incidents
                .iter()
                .map(|i| {
                    (
                        i.id.clone(),
                        IncidentState {
                            name: i.name.clone(),
                            status: i.status,
                            last_update_id: i.latest_update().map(|u| u.id.clone()),
                            component_ids: i.component_ids.clone(),
                        },
                    )
                })
                .collect(),
            components: snap
                .components
                .iter()
                .map(|c| (c.id.clone(), c.status))
                .collect(),
            initialized: true,
        }
    }
}

/// Every event here is notification-worthy.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    IncidentOpened(Incident),
    IncidentUpdated(Incident),
    IncidentResolved {
        id: String,
        name: String,
    },
    /// Only emitted when no incident event in the same poll covers the component.
    ComponentChanged {
        id: String,
        name: String,
        from: ComponentStatus,
        to: ComponentStatus,
    },
}

/// Compare the previous state with a fresh snapshot.
///
/// The first poll (`!prev.initialized`) is a baseline: state is recorded, no events.
pub fn diff(prev: &ServiceState, snap: &Snapshot) -> (ServiceState, Vec<Event>) {
    let new_state = ServiceState::from_snapshot(snap);
    if !prev.initialized {
        return (new_state, Vec::new());
    }

    let mut events = Vec::new();
    // Components mentioned by an incident event in this poll.
    let mut covered: HashSet<String> = HashSet::new();

    for inc in &snap.incidents {
        match prev.incidents.get(&inc.id) {
            None => {
                covered.extend(inc.component_ids.iter().cloned());
                events.push(Event::IncidentOpened(inc.clone()));
            }
            Some(old) => {
                let latest = inc.latest_update().map(|u| u.id.as_str());
                if latest != old.last_update_id.as_deref() {
                    covered.extend(inc.component_ids.iter().cloned());
                    events.push(Event::IncidentUpdated(inc.clone()));
                }
            }
        }
    }

    for (id, old) in &prev.incidents {
        if !snap.incidents.iter().any(|i| &i.id == id) {
            covered.extend(old.component_ids.iter().cloned());
            events.push(Event::IncidentResolved {
                id: id.clone(),
                name: old.name.clone(),
            });
        }
    }

    for c in &snap.components {
        if let Some(&old) = prev.components.get(&c.id) {
            if old != c.status && !covered.contains(&c.id) {
                events.push(Event::ComponentChanged {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    from: old,
                    to: c.status,
                });
            }
        }
    }

    (new_state, events)
}

/// Like [`diff`], but takes the outcome of a fetch. A fetch error produces no events
/// and leaves the state untouched.
pub fn diff_outcome<E>(
    prev: &ServiceState,
    outcome: Result<&Snapshot, E>,
) -> (ServiceState, Vec<Event>) {
    match outcome {
        Ok(snap) => diff(prev, snap),
        Err(_) => (prev.clone(), Vec::new()),
    }
}
