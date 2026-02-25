//! Artifact collection and path normalization

pub mod collector;
pub mod path;

pub use collector::{ArtifactCollector, CollectedFile};
pub use path::{normalize_member_path, is_safe_relative_path};