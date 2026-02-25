//! Pack verification and integrity checking

pub mod checker;

pub use checker::{PackVerifier, VerifyResult, VerifyOutcome, InvalidFinding};