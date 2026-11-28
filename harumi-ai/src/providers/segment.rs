//! Adapter for plain-text translation engines (DeepL, Google Cloud Translation,
//! Azure Translator, an in-house MT service, a dictionary, …).
//!
//! The digital-PDF pipeline talks to [`Translator`] with one JSON batch string
//! per call (the LLM contract described in the crate prompts), and the layout
//! correction loop sends a free-form prompt.  A machine-translation API cannot
//! honour either: it would translate the JSON keys or the prompt itself.
//!
//! [`SegmentAdapter`] bridges that gap.  Implement [`SegmentTranslator`] — "here
//! are N source strings, give back N translations" — and wrap it:
//!
//! ```no_run
//! use async_trait::async_trait;
//! use harumi_ai::providers::{SegmentAdapter, SegmentTranslator};
//!
//! struct MyMt;
//! #[async_trait]
//! impl SegmentTranslator for MyMt {
//!     async fn translate_segments(
//!         &self,
//!         segments: &[String],
//!         _target_lang: &str,
//!         _source_lang: Option<&str>,
//!     ) -> harumi_ai::Result<Vec<String>> {
//!         Ok(segments.to_vec()) // call your MT API here
//!     }
//! }
//!
//! let translator = SegmentAdapter::new(MyMt);
//! ```
//!
//! Layout-correction prompts are answered with an empty correction list, so the
//! layout is fitted purely by geometry (wrapping and shrinking) and the source
//! translation is never rewritten by a non-LLM engine.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{Error, Result, Translator};

/// A translation engine that maps source strings to translated strings 1:1.
#[async_trait]
pub trait SegmentTranslator: Send + Sync {
    /// Translate every segment. Must return exactly `segments.len()` strings,
    /// in the same order.
    async fn translate_segments(
        &self,
        segments: &[String],
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<Vec<String>>;
}

/// Wraps a [`SegmentTranslator`] so it can be used as a [`Translator`].
pub struct SegmentAdapter<T> {
    inner: T,
}

impl<T: SegmentTranslator> SegmentAdapter<T> {
    /// Wrap a plain-text translation engine.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Access the wrapped engine.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

/// Parse a batch request of the shape `{"pages":[{"page":n,"blocks":[{"id":n,"text":"…"}]}]}`.
fn parse_batch(input: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(input.trim()).ok()?;
    v.get("pages")?.as_array()?;
    Some(v)
}

fn is_correction_prompt(input: &str) -> bool {
    input.starts_with("You are a PDF layout editor")
}

#[async_trait]
impl<T: SegmentTranslator> Translator for SegmentAdapter<T> {
    async fn translate(
        &self,
        texts: &[String],
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<Vec<String>> {
        if texts.len() == 1 {
            let input = &texts[0];
            if is_correction_prompt(input) {
                return Ok(vec![json!({ "corrections": [] }).to_string()]);
            }
            if let Some(batch) = parse_batch(input) {
                let pages = batch["pages"].as_array().cloned().unwrap_or_default();
                let mut segments = Vec::new();
                for page in &pages {
                    for block in page["blocks"].as_array().into_iter().flatten() {
                        segments.push(block["text"].as_str().unwrap_or_default().to_owned());
                    }
                }
                let translated = self
                    .inner
                    .translate_segments(&segments, target_lang, source_lang)
                    .await?;
                if translated.len() != segments.len() {
                    return Err(Error::LengthMismatch {
                        expected: segments.len(),
                        got: translated.len(),
                    });
                }
                let mut it = translated.into_iter();
                let out_pages: Vec<Value> = pages
                    .iter()
                    .map(|page| {
                        let blocks: Vec<Value> = page["blocks"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(
                                |b| json!({ "id": b["id"], "text": it.next().unwrap_or_default() }),
                            )
                            .collect();
                        json!({ "page": page["page"], "blocks": blocks })
                    })
                    .collect();
                return Ok(vec![json!({ "pages": out_pages }).to_string()]);
            }
        }
        self.inner
            .translate_segments(texts, target_lang, source_lang)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Upper;
    #[async_trait]
    impl SegmentTranslator for Upper {
        async fn translate_segments(
            &self,
            segments: &[String],
            _target_lang: &str,
            _source_lang: Option<&str>,
        ) -> Result<Vec<String>> {
            Ok(segments.iter().map(|s| s.to_uppercase()).collect())
        }
    }

    #[tokio::test]
    async fn batch_json_is_translated_per_block() {
        let input = r#"{"pages":[{"page":1,"blocks":[{"id":0,"type":"paragraph","text":"a"},{"id":3,"type":"h1","text":"b"}]}]}"#;
        let out = SegmentAdapter::new(Upper)
            .translate(&[input.to_owned()], "en", None)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["pages"][0]["blocks"][0]["text"], "A");
        assert_eq!(v["pages"][0]["blocks"][1]["id"], 3);
        assert_eq!(v["pages"][0]["blocks"][1]["text"], "B");
    }

    #[tokio::test]
    async fn correction_prompt_returns_no_corrections() {
        let prompt = crate::prompts::layout_correction_prompt("en", None, "[]");
        let out = SegmentAdapter::new(Upper)
            .translate(&[prompt], "en", None)
            .await
            .unwrap();
        assert_eq!(out[0], r#"{"corrections":[]}"#);
    }

    #[tokio::test]
    async fn plain_segments_pass_through() {
        let out = SegmentAdapter::new(Upper)
            .translate(&["x".into(), "y".into()], "en", None)
            .await
            .unwrap();
        assert_eq!(out, vec!["X", "Y"]);
    }
}
