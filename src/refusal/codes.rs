use std::fmt;

/// Refusal codes for pack operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    /// `seal` called with no artifacts.
    Empty,
    /// Cannot read input, write output, or read pack directory.
    Io,
    /// Member path collision during seal (including reserved paths).
    Duplicate,
    /// Missing or invalid `manifest.json` for verify/diff/push.
    BadPack,
}

impl RefusalCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Empty => "E_EMPTY",
            Self::Io => "E_IO",
            Self::Duplicate => "E_DUPLICATE",
            Self::BadPack => "E_BAD_PACK",
        }
    }

    pub fn default_message(&self) -> &'static str {
        match self {
            Self::Empty => "No artifacts provided to seal",
            Self::Io => "IO failure reading or writing pack data",
            Self::Duplicate => "Resolved member path collision",
            Self::BadPack => "Missing or invalid manifest.json",
        }
    }
}

impl fmt::Display for RefusalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
