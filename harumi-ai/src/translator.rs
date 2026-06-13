use async_trait::async_trait;

/// Provider-agnostic translation interface.
///
/// Implementations must return a `Vec<String>` of exactly `texts.len()` elements
/// in the same order as the input. Violating this contract causes
/// [`Error::LengthMismatch`](crate::Error::LengthMismatch).
#[async_trait]
pub trait Translator: Send + Sync {
    /// Translate `texts` to `target_lang`.
    ///
    /// `source_lang` is an optional BCP-47 hint (e.g. `"ja"`, `"zh"`).
    /// Pass `None` to let the provider auto-detect the source language.
    async fn translate(
        &self,
        texts: &[String],
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> crate::Result<Vec<String>>;
}
