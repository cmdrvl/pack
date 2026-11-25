mod ledger;
pub mod query;
mod record;

#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

#[cfg(test)]
static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn test_env_guard() -> MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("witness test environment lock should not be poisoned")
}

pub use ledger::{append_witness, witness_ledger_path};
pub use record::{WitnessInput, WitnessRecord};
