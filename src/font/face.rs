//! Private font metadata adapter.

use std::{collections::HashMap, fmt, sync::Mutex};

use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
    raw::FontRef,
};

pub struct Face<'a> {
    inner: FontRef<'a>,
    glyph_cache: Mutex<HashMap<char, Option<GlyphId>>>,
    advance_cache: Mutex<HashMap<u16, Option<u16>>>,
}

impl Clone for Face<'_> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            glyph_cache: Mutex::new(self.glyph_cache.lock().unwrap().clone()),
            advance_cache: Mutex::new(self.advance_cache.lock().unwrap().clone()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphId(pub(crate) u16);

#[derive(Debug, Clone, Copy)]
pub struct ParseError;

impl std::error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid or unsupported OpenType font")
    }
}

impl Face<'_> {
    pub fn parse<'a>(data: &'a [u8], face_index: u32) -> Result<Face<'a>, ParseError> {
        FontRef::from_index(data, face_index)
            .map(|inner| Face {
                inner,
                glyph_cache: Mutex::new(HashMap::new()),
                advance_cache: Mutex::new(HashMap::new()),
            })
            .map_err(|_| ParseError)
    }

    pub fn glyph_index(&self, character: char) -> Option<GlyphId> {
        if let Some(glyph) = self.glyph_cache.lock().unwrap().get(&character).copied() {
            return glyph;
        }
        let glyph = self
            .inner
            .charmap()
            .map(character)
            .and_then(|gid| u16::try_from(gid.to_u32()).ok())
            .map(GlyphId);
        self.glyph_cache.lock().unwrap().insert(character, glyph);
        glyph
    }

    pub fn units_per_em(&self) -> u16 {
        self.inner
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em
    }

    pub fn glyph_hor_advance(&self, glyph: GlyphId) -> Option<u16> {
        if let Some(advance) = self.advance_cache.lock().unwrap().get(&glyph.0).copied() {
            return advance;
        }
        let advance = self
            .inner
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(skrifa::GlyphId::new(glyph.0 as u32))
            .and_then(|width| u16::try_from(width.round() as i32).ok());
        self.advance_cache.lock().unwrap().insert(glyph.0, advance);
        advance
    }

    pub fn ascender(&self) -> i16 {
        self.inner
            .metrics(Size::unscaled(), LocationRef::default())
            .ascent
            .round() as i16
    }

    pub fn descender(&self) -> i16 {
        self.inner
            .metrics(Size::unscaled(), LocationRef::default())
            .descent
            .round() as i16
    }

    pub fn capital_height(&self) -> Option<i16> {
        self.inner
            .metrics(Size::unscaled(), LocationRef::default())
            .cap_height
            .map(|height| height.round() as i16)
    }

    #[allow(dead_code)]
    pub(crate) fn global_bounding_box(&self) -> BoundingBox {
        let bounds = self
            .inner
            .metrics(Size::unscaled(), LocationRef::default())
            .bounds
            .unwrap_or_default();
        BoundingBox {
            x_min: bounds.x_min.round() as i16,
            y_min: bounds.y_min.round() as i16,
            x_max: bounds.x_max.round() as i16,
            y_max: bounds.y_max.round() as i16,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BoundingBox {
    pub(crate) x_min: i16,
    pub(crate) y_min: i16,
    pub(crate) x_max: i16,
    pub(crate) y_max: i16,
}
