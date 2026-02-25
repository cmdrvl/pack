//! Pack manifest model and canonical serialization

pub mod model;
pub mod canonical;

pub use model::{Manifest, Member, MemberType};
pub use canonical::{CanonicalSerializer, to_canonical_json};