use std::sync::Arc;

use futures::stream::{self, StreamExt};
use harumi::Document;

use crate::{
    Error, LayoutOptions, Result, Translator,
    builder::{self, OutputBlock, TranslatedPage},
    extractor,
};

/// Controls what happens when translated text is wider than the original bounding box.
pub enum OverflowStrategy {
    /// Scale the font down until the text fits.  `min_font_size` sets the
    /// floor (default `6.0` pt); below that the text overflows silently.
    Shrink {
        /// Smallest font size (pt) allowed before text is allowed to overflow.
        min_font_size: f32,
    },
    /// If scaling to `min_font_size` still overflows, clip the text and append `"…"`.
    Truncate {
        /// Smallest font size (pt) before the text is truncated.
        min_font_size: f32,
    },
}

impl Default for OverflowStrategy {
    fn default() -> Self { Self::Shrink { min_font_size: 6.0 } }
}

impl OverflowStrategy {
    pub(crate) fn min_font_size(&self) -> f32 {
        match self {
            Self::Shrink { min_font_size } | Self::Truncate { min_font_size } => *min_font_size,
        }
    }
}

/// Controls how the translated PDF is produced.
#[derive(Default)]
pub enum TranslationMode {
    /// Keep the original PDF intact and overlay translated text on top,
    /// covering original text with white rectangles.
    #[default]
    Overlay,
    /// Build a brand-new PDF from scratch. All original layout, images, and
    /// graphics are discarded; only text is preserved.
    NewDocument,
    /// Rewrite the PDF content streams in-place: `Tj`/`TJ` operators are
    /// replaced directly, so the original text is eliminated from the stream.
    /// Lines where no match is found (e.g. per-character Japanese PDFs with
    /// `Td` between each `Tj`) fall back automatically to the `Overlay`
    /// approach for that line only.
    InPlace,
}

/// Options for [`translate_pdf`].
///
/// # Quick construction
/// ```
/// use harumi_ai::{TranslateOptions, providers::EchoTranslator};
/// // via new()
/// let opts = TranslateOptions::new("en", EchoTranslator, vec![]);
/// // via builder
/// let opts = TranslateOptions::builder()
///     .target_lang("en")
///     .translator(EchoTranslator)
///     .font(vec![])
///     .build();
/// ```
pub struct TranslateOptions {
    /// BCP-47 target language tag, e.g. `"en"`, `"zh"`, `"ja"`.
    pub target_lang: String,
    /// Source language hint (BCP-47). `None` → provider auto-detects.
    pub source_lang: Option<String>,
    /// The AI translation provider.
    pub translator: Arc<dyn Translator>,
    /// Unsubsetted TTF/OTF font bytes embedded in the output PDF.
    pub font: Vec<u8>,
    /// Output PDF layout options.
    pub layout: LayoutOptions,
    /// Maximum number of batches translated concurrently (default: `4`).
    pub concurrency: usize,
    /// Number of consecutive source pages grouped into a single LLM request
    /// (default: `1`). Larger values give the model cross-page context at the
    /// cost of larger prompts.
    pub pages_per_batch: usize,
    /// Translation output mode (default: `Overlay`).
    pub mode: TranslationMode,
    /// Background cover color for overlay mode (default: `None` = white `[1.0, 1.0, 1.0]`).
    ///
    /// In overlay mode, a filled rectangle is drawn over the original text before
    /// placing the translation. Set this when the source PDF has a non-white background
    /// (e.g. safety signs, coloured headers) so the cover matches the background.
    pub cover_color: Option<[f32; 3]>,
    /// Additional TTF/OTF fonts tried in order when the primary `font` does not
    /// contain a glyph for a character (default: empty — no fallback).
    ///
    /// harumi-ai partitions each translated text run into sub-runs by font,
    /// embeds only the fonts that are actually used, and renders each sub-run
    /// with the appropriate font.  The primary font is always tried first.
    pub font_fallbacks: Vec<Vec<u8>>,
    /// What to do when translated text overflows its bounding box (default: `Shrink { 6.0 }`).
    pub overflow: OverflowStrategy,
    /// Optional callback invoked after each page's translation completes.
    ///
    /// The first argument is the number of pages translated so far; the second is
    /// the total page count.  Useful for streaming progress to a client.
    pub progress_fn: Option<Arc<dyn Fn(u32, u32) + Send + Sync>>,
}

impl TranslateOptions {
    /// Construct with the three required fields; all other fields use defaults.
    pub fn new(
        target_lang: impl Into<String>,
        translator: impl Translator + 'static,
        font: Vec<u8>,
    ) -> Self {
        Self {
            target_lang: target_lang.into(),
            source_lang: None,
            translator: Arc::new(translator),
            font,
            layout: LayoutOptions::default(),
            concurrency: 4,
            pages_per_batch: 1,
            mode: TranslationMode::default(),
            cover_color: None,
            font_fallbacks: vec![],
            overflow: OverflowStrategy::default(),
            progress_fn: None,
        }
    }

    pub fn builder() -> TranslateOptionsBuilder {
        TranslateOptionsBuilder::default()
    }
}

// ── Builder ──────────────────────────────────────────────────────────────────

/// Builder for [`TranslateOptions`].
#[derive(Default)]
pub struct TranslateOptionsBuilder {
    target_lang: Option<String>,
    source_lang: Option<String>,
    translator: Option<Arc<dyn Translator>>,
    font: Option<Vec<u8>>,
    font_fallbacks: Vec<Vec<u8>>,
    layout: Option<LayoutOptions>,
    concurrency: Option<usize>,
    pages_per_batch: Option<usize>,
    mode: Option<TranslationMode>,
    cover_color: Option<[f32; 3]>,
    overflow: Option<OverflowStrategy>,
    progress_fn: Option<Arc<dyn Fn(u32, u32) + Send + Sync>>,
}

impl TranslateOptionsBuilder {
    /// BCP-47 target language tag (required).
    pub fn target_lang(mut self, lang: impl Into<String>) -> Self {
        self.target_lang = Some(lang.into());
        self
    }

    /// BCP-47 source language hint (optional; omit for auto-detect).
    pub fn source_lang(mut self, lang: impl Into<String>) -> Self {
        self.source_lang = Some(lang.into());
        self
    }

    /// AI translation provider (required).
    pub fn translator(mut self, t: impl Translator + 'static) -> Self {
        self.translator = Some(Arc::new(t));
        self
    }

    /// Unsubsetted TTF/OTF font bytes for the output PDF (required).
    pub fn font(mut self, bytes: Vec<u8>) -> Self {
        self.font = Some(bytes);
        self
    }

    /// Override output layout options.
    pub fn layout(mut self, layout: LayoutOptions) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Maximum concurrent LLM batch requests (default: `4`).
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = Some(n);
        self
    }

    /// Pages per LLM request (default: `1`). Increase for cross-page context.
    pub fn pages_per_batch(mut self, n: usize) -> Self {
        self.pages_per_batch = Some(n.max(1));
        self
    }

    /// Translation output mode (default: `Overlay`).
    pub fn mode(mut self, m: TranslationMode) -> Self {
        self.mode = Some(m);
        self
    }

    /// Background cover color for overlay mode (default: `None` = white).
    ///
    /// Provide an RGB triple where each component is in `0.0..=1.0`.
    pub fn cover_color(mut self, color: [f32; 3]) -> Self {
        self.cover_color = Some(color);
        self
    }

    /// Append a fallback font (unsubsetted TTF/OTF bytes) tried when the primary font
    /// does not contain a glyph.  Call multiple times to add multiple fallbacks.
    pub fn add_font_fallback(mut self, bytes: Vec<u8>) -> Self {
        self.font_fallbacks.push(bytes);
        self
    }

    /// Overflow handling strategy when translated text is wider than the original (default: `Shrink { 6.0 }`).
    pub fn overflow(mut self, strategy: OverflowStrategy) -> Self {
        self.overflow = Some(strategy);
        self
    }

    /// Register a progress callback invoked after each page's translation completes.
    ///
    /// `f(pages_done, total_pages)` — both counts are 1-based.
    pub fn on_progress<F: Fn(u32, u32) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.progress_fn = Some(Arc::new(f));
        self
    }

    /// Build the options. Panics if `target_lang`, `translator`, or `font` are missing.
    pub fn build(self) -> TranslateOptions {
        TranslateOptions {
            target_lang: self
                .target_lang
                .expect("TranslateOptionsBuilder: target_lang() is required"),
            source_lang: self.source_lang,
            translator: self
                .translator
                .expect("TranslateOptionsBuilder: translator() is required"),
            font: self.font.expect("TranslateOptionsBuilder: font() is required"),
            font_fallbacks: self.font_fallbacks,
            layout: self.layout.unwrap_or_default(),
            concurrency: self.concurrency.unwrap_or(4),
            pages_per_batch: self.pages_per_batch.unwrap_or(1),
            mode: self.mode.unwrap_or_default(),
            cover_color: self.cover_color,
            overflow: self.overflow.unwrap_or_default(),
            progress_fn: self.progress_fn,
        }
    }
}

// ── translate_pdf ─────────────────────────────────────────────────────────────

/// Translate all text in `pdf_bytes` to `options.target_lang` and return a new PDF.
///
/// # How it works
///
/// 1. **Extract** — [`harumi::Document::extract_text_chunks`] groups text fragments into
///    paragraphs and headings per page.
/// 2. **Translate** — pages are grouped into batches of [`TranslateOptions::pages_per_batch`]
///    and sent to the `Translator` as `{"pages": [...]}` JSON. Batches run concurrently
///    up to [`TranslateOptions::concurrency`].
/// 3. **Build** — a new PDF is assembled using harumi's direct `add_text` API (CIDFontType2
///    + ToUnicode CMap — PSPDFKit-compatible), or overlaid on the original if
///      [`TranslationMode::Overlay`] is selected.
pub async fn translate_pdf(pdf_bytes: &[u8], options: TranslateOptions) -> Result<Vec<u8>> {
    match options.mode {
        TranslationMode::NewDocument => translate_pdf_new_document(pdf_bytes, options).await,
        TranslationMode::Overlay => crate::overlay::translate_pdf_overlay(pdf_bytes, options).await,
        TranslationMode::InPlace => crate::inplace::translate_pdf_inplace(pdf_bytes, options).await,
    }
}

async fn translate_pdf_new_document(pdf_bytes: &[u8], options: TranslateOptions) -> Result<Vec<u8>> {
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Phase 1: Extract (sync) ───────────────────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    let pages = extractor::extract_pages(&mut doc)?;
    drop(doc);

    if pages.is_empty() {
        let mut blank = Document::new((595.0, 842.0))?;
        return blank.save_to_bytes().map_err(Into::into);
    }

    // ── Phase 2: Translate (async, no Document access) ───────────────────────
    let translator = Arc::clone(&options.translator);
    let target_lang = options.target_lang.clone();
    let source_lang = options.source_lang.clone();
    let batch_size = options.pages_per_batch;
    let total_pages = pages.len() as u32;
    let done_pages = Arc::new(AtomicU32::new(0));

    // Group consecutive pages into batches for cross-page context.
    let batches: Vec<Vec<extractor::PageContent>> = pages
        .chunks(batch_size)
        .map(<[_]>::to_vec)
        .collect();

    let translated_pages: Vec<TranslatedPage> = stream::iter(batches)
        .map(|batch| {
            let translator = Arc::clone(&translator);
            let target = target_lang.clone();
            let src = source_lang.clone();
            let batch_len = batch.len() as u32;
            let done_pages = Arc::clone(&done_pages);
            let progress = options.progress_fn.clone();
            async move {
                let batch_json = extractor::pages_to_json(&batch)?;
                let results = translator
                    .translate(&[batch_json], &target, src.as_deref())
                    .await?;
                let completed = done_pages.fetch_add(batch_len, Ordering::Relaxed) + batch_len;
                if let Some(f) = &progress { f(completed.min(total_pages), total_pages); }

                let json = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::Translator("translator returned empty result".into()))?;

                let page_block_lists = extractor::json_to_translated_pages(&json)?;

                // Match each page in the batch to its translated blocks.
                let translated: Vec<TranslatedPage> = batch
                    .iter()
                    .zip(
                        page_block_lists
                            .iter()
                            .chain(std::iter::repeat(&vec![])), // pad if LLM returned fewer pages
                    )
                    .map(|(orig_page, t_blocks)| {
                        let output_blocks: Vec<OutputBlock> = t_blocks
                            .iter()
                            .filter_map(|tb| {
                                orig_page.blocks.iter().find(|b| b.id == tb.id).map(|orig| {
                                    OutputBlock {
                                        block_type: orig.block_type.clone(),
                                        text: tb.text.clone(),
                                    }
                                })
                            })
                            .collect();
                        TranslatedPage { size: orig_page.size, blocks: output_blocks }
                    })
                    .collect();

                Ok::<Vec<TranslatedPage>, Error>(translated)
            }
        })
        .buffered(options.concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    // ── Phase 3: Build new PDF (sync) ─────────────────────────────────────────
    builder::build_pdf(&translated_pages, &options.font, &options.layout)
}
