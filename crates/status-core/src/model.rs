//! Normalized model shared by all providers.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How bad things are. `Ord` answers "which is worse?":
/// `None < Maintenance < Unknown < Minor < Major < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    None,
    Minor,
    Major,
    Critical,
    Maintenance,
    #[serde(other)]
    Unknown,
}

impl Severity {
    fn rank(self) -> u8 {
        match self {
            Severity::None => 0,
            Severity::Maintenance => 1,
            Severity::Unknown => 2,
            Severity::Minor => 3,
            Severity::Major => 4,
            Severity::Critical => 5,
        }
    }

    /// Parse Statuspage `status.indicator` / `incident.impact` values.
    pub fn from_api(s: &str) -> Self {
        match s {
            "none" => Severity::None,
            "minor" => Severity::Minor,
            "major" => Severity::Major,
            "critical" => Severity::Critical,
            "maintenance" => Severity::Maintenance,
            _ => Severity::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Severity::None => "Operational",
            Severity::Minor => "Degraded",
            Severity::Major => "Major outage",
            Severity::Critical => "Critical outage",
            Severity::Maintenance => "Maintenance",
            Severity::Unknown => "Unknown",
        }
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    Operational,
    DegradedPerformance,
    PartialOutage,
    MajorOutage,
    UnderMaintenance,
    #[serde(other)]
    Unknown,
}

impl ComponentStatus {
    pub fn from_api(s: &str) -> Self {
        match s {
            "operational" => ComponentStatus::Operational,
            "degraded_performance" => ComponentStatus::DegradedPerformance,
            "partial_outage" => ComponentStatus::PartialOutage,
            "major_outage" => ComponentStatus::MajorOutage,
            "under_maintenance" => ComponentStatus::UnderMaintenance,
            _ => ComponentStatus::Unknown,
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            ComponentStatus::Operational => Severity::None,
            ComponentStatus::DegradedPerformance => Severity::Minor,
            ComponentStatus::PartialOutage => Severity::Major,
            ComponentStatus::MajorOutage => Severity::Critical,
            ComponentStatus::UnderMaintenance => Severity::Maintenance,
            ComponentStatus::Unknown => Severity::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ComponentStatus::Operational => "Operational",
            ComponentStatus::DegradedPerformance => "Degraded performance",
            ComponentStatus::PartialOutage => "Partial outage",
            ComponentStatus::MajorOutage => "Major outage",
            ComponentStatus::UnderMaintenance => "Under maintenance",
            ComponentStatus::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    Postmortem,
    Scheduled,
    InProgress,
    Verifying,
    Completed,
    #[serde(other)]
    Unknown,
}

impl IncidentStatus {
    pub fn from_api(s: &str) -> Self {
        match s {
            "investigating" => IncidentStatus::Investigating,
            "identified" => IncidentStatus::Identified,
            "monitoring" => IncidentStatus::Monitoring,
            "resolved" => IncidentStatus::Resolved,
            "postmortem" => IncidentStatus::Postmortem,
            "scheduled" => IncidentStatus::Scheduled,
            "in_progress" => IncidentStatus::InProgress,
            "verifying" => IncidentStatus::Verifying,
            "completed" => IncidentStatus::Completed,
            _ => IncidentStatus::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            IncidentStatus::Investigating => "Investigating",
            IncidentStatus::Identified => "Identified",
            IncidentStatus::Monitoring => "Monitoring",
            IncidentStatus::Resolved => "Resolved",
            IncidentStatus::Postmortem => "Postmortem",
            IncidentStatus::Scheduled => "Scheduled",
            IncidentStatus::InProgress => "In progress",
            IncidentStatus::Verifying => "Verifying",
            IncidentStatus::Completed => "Completed",
            IncidentStatus::Unknown => "Update",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub status: ComponentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentUpdate {
    pub id: String,
    pub status: IncidentStatus,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub name: String,
    pub status: IncidentStatus,
    pub impact: Severity,
    pub url: String,
    /// Ids of components the incident affects (used to avoid double notifications).
    pub component_ids: Vec<String>,
    /// Newest first, as the API returns them.
    pub updates: Vec<IncidentUpdate>,
}

impl Incident {
    pub fn latest_update(&self) -> Option<&IncidentUpdate> {
        self.updates.first()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub overall: Severity,
    pub components: Vec<Component>,
    /// Unresolved incidents only.
    pub incidents: Vec<Incident>,
    pub fetched_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering_is_worst_last() {
        let mut v = vec![
            Severity::Critical,
            Severity::None,
            Severity::Unknown,
            Severity::Major,
            Severity::Maintenance,
            Severity::Minor,
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                Severity::None,
                Severity::Maintenance,
                Severity::Unknown,
                Severity::Minor,
                Severity::Major,
                Severity::Critical
            ]
        );
    }

    #[test]
    fn unknown_values_never_fail() {
        assert_eq!(Severity::from_api("wat"), Severity::Unknown);
        assert_eq!(ComponentStatus::from_api("wat"), ComponentStatus::Unknown);
        assert_eq!(IncidentStatus::from_api("wat"), IncidentStatus::Unknown);
        let s: ComponentStatus = serde_json::from_str("\"wat\"").unwrap();
        assert_eq!(s, ComponentStatus::Unknown);
    }
}
