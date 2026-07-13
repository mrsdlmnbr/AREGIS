//! Time is an input, never an ambient (axiom A7).
//!
//! Domain logic receives a `&dyn Clock` or an explicit `Timestamp`. This file
//! is the ONLY place in the Rust tree permitted to touch the wall clock
//! (scripts/archlint.sh allowlists exactly this path), and even here the
//! system clock is a thin edge adapter — nothing in `crates/` calls it.

use std::cell::Cell;

pub type Timestamp = chrono::DateTime<chrono::Utc>;

pub trait Clock {
    fn now(&self) -> Timestamp;
}

/// Deterministic clock for simulation and replay. Interior mutability so the
/// harness can hold shared references while advancing time.
#[derive(Debug, Clone)]
pub struct SimClock {
    now: Cell<Timestamp>,
}

impl SimClock {
    pub fn new(start: Timestamp) -> Self {
        Self { now: Cell::new(start) }
    }

    pub fn set(&self, t: Timestamp) {
        self.now.set(t);
    }

    pub fn advance_secs(&self, secs: f64) {
        let delta = chrono::Duration::microseconds((secs * 1_000_000.0).round() as i64);
        self.now.set(self.now.get() + delta);
    }
}

impl Clock for SimClock {
    fn now(&self) -> Timestamp {
        self.now.get()
    }
}

/// Wall-clock adapter for binary entrypoints only. Domain crates must never
/// construct one; they take time as an argument.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now()
    }
}

pub fn parse_ts(s: &str) -> Result<Timestamp, chrono::ParseError> {
    Ok(chrono::DateTime::parse_from_rfc3339(s)?.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_clock_is_settable_and_advances() {
        let c = SimClock::new(parse_ts("2026-03-14T03:11:40Z").unwrap());
        c.advance_secs(5.4);
        assert_eq!(c.now(), parse_ts("2026-03-14T03:11:45.400Z").unwrap());
    }
}
