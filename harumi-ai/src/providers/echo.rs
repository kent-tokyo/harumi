use crate::{Result, Translator};
use async_trait::async_trait;

/// A no-op translator that returns every input string unchanged.
///
/// Useful for testing the PDF manipulation pipeline without an LLM API key.
pub struct EchoTranslator;

#[async_trait]
impl Translator for EchoTranslator {
    async fn translate(
        &self,
        texts: &[String],
        _target_lang: &str,
        _source_lang: Option<&str>,
    ) -> Result<Vec<String>> {
        Ok(texts.to_vec())
    }
}
