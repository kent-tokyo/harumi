use std::fmt;

/// Errors returned by harumi-ai operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error from the underlying harumi PDF layer.
    Harumi(harumi::Error),
    /// The AI translator returned an error or invalid JSON.
    Translator(String),
    /// The translator violated its contract: returned Vec length must equal input length.
    LengthMismatch { expected: usize, got: usize },
    /// The provided font bytes could not be parsed.
    FontParse(String),
    /// I/O error.
    Io(std::io::Error),
    /// The translated PDF failed the quality gate set by [`crate::QualityProfile::Strict`].
    ///
    /// Only returned when [`crate::QualityProfile::Strict`] is active and the final
    /// layout has violations.  The [`crate::quality::QualityViolation`] list describes
    /// what exceeded the thresholds.
    QualityGateFailed(Vec<crate::quality::QualityViolation>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Harumi(e) => write!(f, "PDF error: {e}"),
            Error::Translator(msg) => write!(f, "translator error: {msg}"),
            Error::LengthMismatch { expected, got } => {
                write!(f, "translator returned {got} items for {expected} inputs")
            }
            Error::FontParse(msg) => write!(f, "font parse error: {msg}"),
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::QualityGateFailed(violations) => {
                write!(f, "quality gate failed: ")?;
                for (i, v) in violations.iter().enumerate() {
                    if i > 0 { write!(f, "; ")?; }
                    write!(f, "{v}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<harumi::Error> for Error {
    fn from(e: harumi::Error) -> Self {
        Error::Harumi(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Alias for `std::result::Result<T, harumi_ai::Error>`.
pub type Result<T> = std::result::Result<T, Error>;
