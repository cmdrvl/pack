//! Refusal system for pack errors

pub mod codes;
pub mod envelope;

pub use codes::{RefusalCode, RefusalDetail};
pub use envelope::{RefusalEnvelope, RefusalOutcome, output_refusal};