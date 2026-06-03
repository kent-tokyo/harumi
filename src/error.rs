use thiserror::Error;

/// Errors returned by harumi operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A file system or stream I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A structural problem in the PDF (malformed object graph, missing key, etc.).
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// The font format is not supported. Only TrueType (`.ttf`) and OpenType (`.otf`) are supported.
    #[error("unsupported font kind: only TrueType (.ttf) and OpenType (.otf) are supported")]
    UnsupportedFontKind,

    /// The font binary could not be parsed.
    #[error("font parse error: {0}")]
    FontParse(String),

    /// The requested page number does not exist in the document.
    #[error("page {0} not found")]
    PageNotFound(u32),

    /// A [`FontHandle`](crate::FontHandle) obtained from a different `Document` was used.
    #[error("font handle {0} is invalid")]
    InvalidFont(u32),

    /// An image could not be decoded (requires the `image` feature).
    #[cfg(feature = "image")]
    #[error("image decode error: {0}")]
    ImageDecode(String),

    /// A caller-supplied parameter is invalid (e.g. NaN coordinate, zero-size box).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A character in `new_text` is not present in the embedded font's ToUnicode mapping.
    /// The font may be subsetted and no longer contain the required glyph.
    #[error("char '{ch}' not found in font '{font_name}' ToUnicode mapping; font may be subsetted")]
    FontCharNotMapped { ch: char, font_name: String },

    /// The password provided for an encrypted PDF was incorrect.
    #[error("wrong password for encrypted PDF")]
    WrongPassword,
}

/// Alias for `std::result::Result<T, harumi::Error>`.
pub type Result<T> = std::result::Result<T, Error>;
