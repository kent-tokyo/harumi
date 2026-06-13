use std::collections::HashMap;

/// Maps BCP-47 language tags to unsubsetted TTF/OTF font bytes.
///
/// Required for cross-script translation (e.g. JA→ZH, EN→KO) where the target
/// language needs different glyph coverage than the source font. Same-script
/// pairs (e.g. EN→DE) do not require a FontMap entry.
///
/// # Example
/// ```no_run
/// use harumi_ai::FontMap;
/// let mut map = FontMap::new();
/// map.insert("zh", std::fs::read("NotoSansCJKsc-Regular.ttf").unwrap());
/// map.insert("ja", std::fs::read("NotoSansCJKjp-Regular.ttf").unwrap());
/// ```
#[derive(Default, Clone)]
pub struct FontMap {
    inner: HashMap<String, Vec<u8>>,
}

impl FontMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `ttf_bytes` for `lang` (e.g. `"zh"`, `"ja"`, `"ko"`).
    pub fn insert(&mut self, lang: impl Into<String>, ttf_bytes: Vec<u8>) {
        self.inner.insert(lang.into(), ttf_bytes);
    }

    pub(crate) fn get(&self, lang: &str) -> Option<&[u8]> {
        self.inner.get(lang).map(Vec::as_slice)
    }
}
