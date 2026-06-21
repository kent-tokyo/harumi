// cache.rs — in-memory translation cache

use std::collections::HashMap;

/// In-memory cache mapping source text to its translation.
///
/// Pass a shared `Arc<tokio::sync::Mutex<TranslationCache>>` via
/// [`TranslateOptions::cache`] to deduplicate repeated phrases within
/// a document (or across multiple `translate_pdf` calls when the same
/// `Arc` is reused).
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use harumi_ai::{TranslationCache, TranslateOptions, providers::EchoTranslator};
///
/// let cache = Arc::new(tokio::sync::Mutex::new(TranslationCache::default()));
///
/// let opts1 = TranslateOptions::new("en", EchoTranslator, vec![])
///     .with_cache(Arc::clone(&cache));
/// // opts2 with the same Arc will reuse cached translations from opts1's run.
/// let opts2 = TranslateOptions::new("en", EchoTranslator, vec![])
///     .with_cache(Arc::clone(&cache));
/// ```
#[derive(Default)]
pub struct TranslationCache {
    map: HashMap<String, String>,
    hits: usize,
    misses: usize,
}

impl TranslationCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a cached translation for `text`.
    pub fn get(&mut self, text: &str) -> Option<&str> {
        if let Some(v) = self.map.get(text) {
            self.hits += 1;
            Some(v.as_str())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store a translation without updating hit/miss counters.
    pub fn insert(&mut self, source: String, translation: String) {
        self.map.insert(source, translation);
    }

    /// Number of unique source texts stored.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` when no translations are stored.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Total cache hits since creation.
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// Total cache misses since creation.
    pub fn misses(&self) -> usize {
        self.misses
    }

    /// Hit rate as a fraction in `0.0..=1.0`.  Returns `0.0` when no lookups have occurred.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}
