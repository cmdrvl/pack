//! Member copy and bytes hashing

pub mod processor;
pub mod hasher;

pub use processor::{MemberProcessor, ProcessedMember};
pub use hasher::{compute_sha256_hex, hash_bytes};