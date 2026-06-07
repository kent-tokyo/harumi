pub mod cmap;
pub mod embed;
pub mod subset;
mod ttf_subset;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    TrueType,
    Cff,
}

impl FontKind {
    pub fn detect(data: &[u8]) -> Option<FontKind> {
        if data.len() < 4 {
            return None;
        }
        match &data[..4] {
            [0x00, 0x01, 0x00, 0x00] | b"true" | b"ttcf" => Some(FontKind::TrueType),
            b"OTTO" => Some(FontKind::Cff),
            _ => None,
        }
    }
}

/// Opaque handle to a font registered with [`crate::Document::embed_font`].
///
/// The handle is cheap to copy and can be passed to any number of text
/// placement calls on any page of the same document. Using a handle from a
/// different document will produce an [`Error::InvalidFont`](crate::Error::InvalidFont).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontHandle(pub(crate) u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_truetype_magic() {
        assert_eq!(FontKind::detect(&[0x00, 0x01, 0x00, 0x00, 0x00]), Some(FontKind::TrueType));
        assert_eq!(FontKind::detect(b"true\x00"), Some(FontKind::TrueType));
    }

    #[test]
    fn detect_ttc_magic() {
        // TTC collections start with 'ttcf'.
        assert_eq!(FontKind::detect(b"ttcf\x00\x01\x00\x00"), Some(FontKind::TrueType));
    }

    #[test]
    fn detect_cff_magic() {
        assert_eq!(FontKind::detect(b"OTTO\x00"), Some(FontKind::Cff));
    }

    #[test]
    fn detect_unknown_returns_none() {
        assert_eq!(FontKind::detect(b"WOFF"), None);
        assert_eq!(FontKind::detect(b"wOFF"), None);
        assert_eq!(FontKind::detect(b"\x00\x00"), None); // too short
    }
}

/// Internal state for a font embedded in the document.
#[derive(Debug)]
#[allow(dead_code)]
pub struct EmbeddedFont {
    /// lopdf object ID of the Type0 font dictionary.
    pub type0_id: lopdf::ObjectId,
    /// Key used in the page /Resources /Font dict, e.g. b"F0".
    pub pdf_name: Vec<u8>,
    /// GID → Unicode char, built during subsetting.
    pub gid_to_char: BTreeMap<u16, char>,
    /// GID → advance width in font design units.
    pub gid_to_advance: BTreeMap<u16, u16>,
    pub units_per_em: u16,
}
