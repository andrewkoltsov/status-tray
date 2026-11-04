//! Notification text and per-service rate limiting (no OS calls here).

use std::time::{Duration, Instant};

use crate::diff::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

const BODY_CHARS: usize = 140;

pub fn format(service: &str, event: &Event) -> Notification {
    match event {
        Event::IncidentOpened(inc) | Event::IncidentUpdated(inc) => {
            let (status, text) = match inc.latest_update() {
                Some(u) => (u.status, u.body.as_str()),
                None => (inc.status, ""),
            };
            let text = truncate(text, BODY_CHARS);
            let body = if text.is_empty() {
                status.label().to_owned()
            } else {
                format!("{}: {}", status.label(), text)
            };
            Notification {
                title: format!("{}: {}", service, inc.name),
                body,
            }
        }
        Event::IncidentResolved { name, .. } => Notification {
            title: format!("{}: resolved \u{2014} {}", service, name),
            body: "The incident has been resolved.".to_owned(),
        },
        Event::ComponentChanged { name, to, .. } => Notification {
            title: format!("{}: {} \u{2014} {}", service, name, to.label()),
            body: String::new(),
        },
    }
}

/// Collapse whitespace and cut at `max` characters.
fn truncate(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{}\u{2026}", cut.trim_end())
    }
}

/// At most one notification per window; anything arriving meanwhile is merged
/// into the next one as "(+N more updates)".
pub struct RateLimiter {
    window: Duration,
    last_sent: Option<Instant>,
    pending: Vec<Notification>,
}

impl RateLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last_sent: None,
            pending: Vec::new(),
        }
    }

    /// Queue `items` and return the notification to show now, if the window allows.
    /// Call it after every poll (also with no items) so held events get flushed.
    pub fn submit(&mut self, items: Vec<Notification>, now: Instant) -> Option<Notification> {
        self.pending.extend(items);
        if self.pending.is_empty() {
            return None;
        }
        if let Some(last) = self.last_sent {
            if now.duration_since(last) < self.window {
                return None;
            }
        }
        let mut all = std::mem::take(&mut self.pending).into_iter();
        let mut first = all.next()?;
        let more = all.count();
        if more > 0 {
            if first.body.is_empty() {
                first.body = format!("+{more} more updates");
            } else {
                first.body = format!("{} (+{more} more updates)", first.body);
            }
        }
        self.last_sent = Some(now);
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ComponentStatus;

    fn n(t: &str) -> Notification {
        Notification {
            title: t.into(),
            body: "b".into(),
        }
    }

    #[test]
    fn limiter_sends_first_then_holds_then_merges() {
        let t0 = Instant::now();
        let mut rl = RateLimiter::new(Duration::from_secs(30));
        assert_eq!(rl.submit(vec![n("a")], t0).unwrap().title, "a");
        // inside the window: held
        assert!(rl
            .submit(vec![n("b"), n("c")], t0 + Duration::from_secs(5))
            .is_none());
        // window over: one merged notification
        let out = rl.submit(vec![], t0 + Duration::from_secs(31)).unwrap();
        assert_eq!(out.title, "b");
        assert_eq!(out.body, "b (+1 more updates)");
        assert!(rl.submit(vec![], t0 + Duration::from_secs(100)).is_none());
    }

    #[test]
    fn component_text() {
        let ev = Event::ComponentChanged {
            id: "1".into(),
            name: "Actions".into(),
            from: ComponentStatus::Operational,
            to: ComponentStatus::PartialOutage,
        };
        assert_eq!(
            format("GitHub", &ev).title,
            "GitHub: Actions \u{2014} Partial outage"
        );
    }

    #[test]
    fn truncates_long_bodies() {
        let long = "word ".repeat(100);
        let t = truncate(&long, 140);
        assert!(t.chars().count() <= 141);
        assert!(t.ends_with('\u{2026}'));
    }
}
