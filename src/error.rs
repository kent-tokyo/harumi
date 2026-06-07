use std::fmt;

/// Errors returned by harumi operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A file system or stream I/O error.
    Io(std::io::Error),

    /// A structural problem in the PDF (malformed object graph, missing key, etc.).
    Pdf(lopdf::Error),

    /// The font format is not supported. Only TrueType (`.ttf`) and OpenType (`.otf`) are supported.
    UnsupportedFontKind,

    /// The font binary could not be parsed.
    FontParse(String),

    /// The requested page number does not exist in the document.
    PageNotFound(u32),

    /// A [`FontHandle`](crate::FontHandle) obtained from a different `Document` was used.
    InvalidFont(u32),

    /// An image could not be decoded (requires the `image` feature).
    #[cfg(feature = "image")]
    ImageDecode(String),

    #[cfg(not(feature = "image"))]
    #[doc(hidden)]
    _ImageDecode,

    /// A caller-supplied parameter is invalid (e.g. NaN coordinate, zero-size box).
    InvalidInput(String),

    /// A character in `new_text` is not present in the embedded font's ToUnicode mapping.
    /// The font may be subsetted and no longer contain the required glyph.
    FontCharNotMapped { ch: char, font_name: String },

    /// The password provided for an encrypted PDF was incorrect.
    WrongPassword,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Pdf(e) => write!(f, "PDF error: {}", e),
            Error::UnsupportedFontKind => write!(
                f,
                "unsupported font kind: only TrueType (.ttf) and OpenType (.otf) are supported"
            ),
            Error::FontParse(msg) => write!(f, "font parse error: {}", msg),
            Error::PageNotFound(page) => write!(f, "page {} not found", page),
            Error::InvalidFont(handle) => write!(f, "font handle {} is invalid", handle),
            #[cfg(feature = "image")]
            Error::ImageDecode(msg) => write!(f, "image decode error: {}", msg),
            #[cfg(not(feature = "image"))]
            Error::_ImageDecode => unreachable!(),
            Error::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            Error::FontCharNotMapped { ch, font_name } => {
                write!(
                    f,
                    "char '{}' not found in font '{}' ToUnicode mapping; font may be subsetted",
                    ch, font_name
                )
            }
            Error::WrongPassword => write!(f, "wrong password for encrypted PDF"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<lopdf::Error> for Error {
    fn from(e: lopdf::Error) -> Self {
        Error::Pdf(e)
    }
}

/// Alias for `std::result::Result<T, harumi::Error>`.
pub type Result<T> = std::result::Result<T, Error>;
