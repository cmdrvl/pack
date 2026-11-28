mod memory;
mod report;
mod schema;

#[cfg(feature = "cli")]
mod checks;
#[cfg(feature = "cli")]
mod command;

#[cfg(feature = "cli")]
pub(crate) use checks::run_checks;
#[cfg(feature = "cli")]
pub use command::{execute_verify, execute_verify_expecting};
pub(crate) use memory::verify_manifest_members;
pub use memory::verify_tar_bytes;
pub use report::{VerifyChecks, VerifyOutcome, VerifyReport};
#[cfg(feature = "cli")]
pub(crate) use schema::has_schema_for_version;
