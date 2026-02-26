/// Exit codes used across pack subcommands.
///
/// Mapping:
///   0 — success (PACK_CREATED, OK, NO_CHANGES, PUBLISHED, FETCHED)
///   1 — domain failure (INVALID, CHANGES)
///   2 — refusal (REFUSAL)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Invalid = 1,
    Refusal = 2,
}

impl From<ExitCode> for u8 {
    fn from(code: ExitCode) -> u8 {
        code as u8
    }
}
