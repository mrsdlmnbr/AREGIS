//! aegis-common — shared foundations for the Rust spine.
//!
//! Everything here exists to make three axioms structural:
//! - A1: the log is the truth (hash chain, canonical encoding)
//! - A5: provenance on every byte (Provenance, Envelope)
//! - A7: time is an input, never an ambient (Clock)

pub mod canonical;
pub mod chain;
pub mod clock;
pub mod crypto;
pub mod geometry;
pub mod hash;
pub mod types;

pub use clock::{Clock, SimClock, Timestamp};
pub use types::*;
