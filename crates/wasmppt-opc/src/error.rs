use core::fmt;

/// Stable machine-readable category for a package failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    Io,
    Truncated,
    InvalidSignature,
    InvalidField,
    InvalidPath,
    DuplicateEntry,
    UnsupportedCompression,
    UnsupportedEncryption,
    UnsupportedMultiDisk,
    UnsupportedZip64,
    LimitExceeded,
    OverlappingEntries,
    ChecksumMismatch,
    SizeMismatch,
}

/// A bounded ZIP/OPC error with a stable code and actionable context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    code: ErrorCode,
    message: String,
}

impl Error {
    /// Construct a capability or package error with a stable machine code.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The stable category consumers should branch on.
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Human-readable context for logs and diagnostics.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
