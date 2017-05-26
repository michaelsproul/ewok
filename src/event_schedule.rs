//! Tools for specifying events in advance.

use event::Event;
use std::collections::BTreeMap;

/// A schedule for the occurence of events like node additions and removals.
///
/// You specify the event, and the step number at which you'd like it to occur.
pub struct EventSchedule {
    pub schedule: BTreeMap<u64, Vec<Event>>,
}

impl EventSchedule {
    pub fn new(schedule: BTreeMap<u64, Vec<Event>>) -> Self {
        EventSchedule { schedule }
    }

    pub fn empty() -> Self {
        EventSchedule { schedule: BTreeMap::new() }
    }

    /// Fetch events occuring at the given step.
    pub fn get_events(&self, step: u64) -> Vec<Event> {
        self.schedule
            .get(&step)
            .cloned()
            .unwrap_or_else(Vec::new)
    }
}
