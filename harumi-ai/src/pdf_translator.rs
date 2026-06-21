use std::sync::Arc;

use futures::stream::{self, StreamExt};
use harumi::Document;

use crate::{
    Error, LayoutOptions, Result, Translator,
    builder::{self, OutputBlock, TranslatedPage},
    cache::TranslationCache,
    extractor,
    output::{DebugArtifacts, DebugOptions, TranslateOutput, TranslateQuality},
    quality::{QualityGate, QualityProfile, QualityResult},
};
use tokio::sync::Mutex;

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
#[derive(Default, Debug, Clone, PartialEq, Eq)]
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
    /// Automatically choose the best mode based on the PDF's layout structure.
    ///
    /// The heuristic inspects the fraction of [`harumi::LayoutRegionKind::TableCell`]
    /// regions on the first page.  If the PDF is dense table/form-like (> 60 % table
    /// cells) or has per-character `Tj` streams, `Overlay` is chosen; otherwise
    /// `InPlace` is tried first.  The mode that was actually used is reported in
    /// [`TranslateQuality::mode_used`].
    Auto,
    /// Produce a bilingual PDF where each original page is immediately followed by
    /// its translated version.
    ///
    /// Output page order: `[orig_1, trans_1, orig_2, trans_2, …, orig_n, trans_n]`.
    /// Useful for side-by-side review and quality-checking workflows.
    Bilingual,
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
    /// Quality profile used to gate the final PDF and guide the correction loop
    /// (default: [`QualityProfile::BestEffort`]).
    pub profile: QualityProfile,
    /// Maximum number of AI layout correction rounds (default: `2`).
    ///
    /// Each round identifies overflowing or colliding lines and asks the AI to
    /// shorten them.  Setting `0` disables the correction loop entirely.
    pub max_correction_rounds: usize,
    /// Controls which debug artifacts are included in [`TranslateOutput::debug`]
    /// (default: all `false`).
    pub debug: DebugOptions,
    /// Skip headers and footers — they will not be covered or translated
    /// (default: `false`).
    ///
    /// When `true`, lines whose layout region role is
    /// [`harumi::LayoutRegionRole::HeaderFooter`] are left completely untouched.
    pub skip_header_footer: bool,
    /// Automatically skip text that is primarily math/formula characters
    /// (Greek letters, math operators, Mathematical Alphanumeric Symbols)
    /// (default: `false`).
    ///
    /// Lines classified as math are passed through verbatim — not covered
    /// and not sent to the AI provider.  See `with_math_patterns()` to add
    /// explicit regex patterns instead.
    pub auto_skip_math: bool,
    /// Optional shared in-memory translation cache.
    ///
    /// When set, repeated phrases are resolved from the cache instead of being
    /// sent to the AI provider.  Pass the same `Arc` across multiple
    /// `translate_pdf` calls to share the cache between documents.
    /// Use [`TranslateOptions::with_cache`] to set this in a builder chain.
    pub cache: Option<Arc<Mutex<TranslationCache>>>,
    /// Regex patterns for text that must NOT be translated (passed through as-is).
    ///
    /// Each string is compiled as a full-match [`regex::Regex`].  Strings that
    /// fail to compile are silently skipped.  Use
    /// [`TranslateOptions::with_sds_patterns`] to add built-in SDS defaults.
    pub skip_patterns: Vec<String>,
}

impl Clone for TranslateOptions {
    fn clone(&self) -> Self {
        Self {
            target_lang: self.target_lang.clone(),
            source_lang: self.source_lang.clone(),
            translator: Arc::clone(&self.translator),
            font: self.font.clone(),
            layout: self.layout.clone(),
            concurrency: self.concurrency,
            pages_per_batch: self.pages_per_batch,
            mode: self.mode.clone(),
            cover_color: self.cover_color,
            font_fallbacks: self.font_fallbacks.clone(),
            overflow: match &self.overflow {
                OverflowStrategy::Shrink { min_font_size } => {
                    OverflowStrategy::Shrink { min_font_size: *min_font_size }
                }
                OverflowStrategy::Truncate { min_font_size } => {
                    OverflowStrategy::Truncate { min_font_size: *min_font_size }
                }
            },
            progress_fn: self.progress_fn.clone(),
            profile: match self.profile {
                QualityProfile::PreserveLayout => QualityProfile::PreserveLayout,
                QualityProfile::Readable => QualityProfile::Readable,
                QualityProfile::Strict => QualityProfile::Strict,
                QualityProfile::BestEffort => QualityProfile::BestEffort,
            },
            max_correction_rounds: self.max_correction_rounds,
            debug: self.debug.clone(),
            skip_header_footer: self.skip_header_footer,
            auto_skip_math: self.auto_skip_math,
            cache: self.cache.as_ref().map(Arc::clone),
            skip_patterns: self.skip_patterns.clone(),
        }
    }
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
            profile: QualityProfile::default(),
            max_correction_rounds: 2,
            debug: DebugOptions::default(),
            skip_header_footer: false,
            auto_skip_math: false,
            cache: None,
            skip_patterns: vec![],
        }
    }

    /// Attach a shared translation cache.
    pub fn with_cache(mut self, cache: Arc<Mutex<TranslationCache>>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Add built-in skip patterns for SDS (Safety Data Sheet) documents.
    ///
    /// Protects: chemical formulas (H₂SO₄), CAS numbers (7664-93-9), UN
    /// numbers (UN1830), numeric value+unit strings, and comparison expressions.
    pub fn with_sds_patterns(mut self) -> Self {
        self.skip_patterns.extend([
            r"^[A-Z][a-z]?\d*(\([A-Z][a-z]?\d*\)[\d]?)+$",    // Ca(OH)2, Fe2(SO4)3
            r"^[A-Z][a-z]?\d+([A-Z][a-z]?\d*)*$",              // H2SO4, NaOH, CO2
            r"^\d{1,7}-\d{2}-\d$",                              // CAS: 7664-93-9
            r"^UN\s?\d{4}$",                                     // UN1830
            r"^\d+(\.\d+)?\s*(mg|kg|mL|L|ppm|ppb|%|°C|K|Pa|MPa|bar|mol|g|t|μg|ng)(/\w+)?$",
            r"^[<>≤≥±]\s*[\d.,]+(\s*[\w%/°]+)?$",               // < 5, ≥ 10, ± 0.5 %
        ].map(String::from));
        self
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
    profile: Option<QualityProfile>,
    max_correction_rounds: Option<usize>,
    debug: Option<DebugOptions>,
    cache: Option<Arc<Mutex<TranslationCache>>>,
    skip_patterns: Vec<String>,
    skip_header_footer: bool,
    auto_skip_math: bool,
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

    /// Set the quality profile (default: [`QualityProfile::BestEffort`]).
    pub fn profile(mut self, p: QualityProfile) -> Self {
        self.profile = Some(p);
        self
    }

    /// Maximum AI correction rounds (default: `2`). Set `0` to disable.
    pub fn max_correction_rounds(mut self, n: usize) -> Self {
        self.max_correction_rounds = Some(n);
        self
    }

    /// Debug artifact options (default: all disabled).
    pub fn debug(mut self, opts: DebugOptions) -> Self {
        self.debug = Some(opts);
        self
    }

    /// Attach a shared translation cache.
    pub fn with_cache(mut self, cache: Arc<Mutex<TranslationCache>>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Add a regex pattern for text that must not be translated.
    pub fn add_skip_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.skip_patterns.push(pattern.into());
        self
    }

    /// Skip headers and footers (default `false`).
    pub fn skip_header_footer(mut self, v: bool) -> Self {
        self.skip_header_footer = v;
        self
    }

    /// Auto-skip math/formula lines via Unicode detection (default `false`).
    pub fn auto_skip_math(mut self, v: bool) -> Self {
        self.auto_skip_math = v;
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
            profile: self.profile.unwrap_or_default(),
            max_correction_rounds: self.max_correction_rounds.unwrap_or(2),
            debug: self.debug.unwrap_or_default(),
            skip_header_footer: self.skip_header_footer,
            auto_skip_math: self.auto_skip_math,
            cache: self.cache,
            skip_patterns: self.skip_patterns,
        }
    }
}

// ── translate_pdf ─────────────────────────────────────────────────────────────

/// Translate all text in `pdf_bytes` to `options.target_lang`.
///
/// Returns a [`TranslateOutput`] containing the PDF bytes, per-page quality
/// diagnostics, and optional debug artifacts.
///
/// # How it works
///
/// 1. **Mode selection** — if [`TranslationMode::Auto`] is set, the PDF structure
///    is inspected to choose between `InPlace` and `Overlay`.
/// 2. **Extract** — text is extracted and structured per page.
/// 3. **Translate** — pages are batched and sent to the AI provider concurrently.
/// 4. **Place** — translated text is placed using the selected mode.
/// 5. **Correction loop** — overflowing or colliding lines are sent back to the AI
///    for shortening (up to [`TranslateOptions::max_correction_rounds`] rounds).
/// 6. **Quality gate** — the final layout is evaluated against
///    [`TranslateOptions::profile`].  Only [`QualityProfile::Strict`] causes an
///    error on failure; all other profiles return the PDF regardless.
pub async fn translate_pdf(pdf_bytes: &[u8], options: TranslateOptions) -> Result<TranslateOutput> {
    // Auto mode with quality-aware profiles uses a multi-stage cascade.
    if matches!(options.mode, TranslationMode::Auto)
        && !matches!(options.profile, QualityProfile::BestEffort)
    {
        return translate_pdf_auto_cascade(pdf_bytes, options).await;
    }

    // All other cases: resolve to one concrete mode and dispatch once.
    let mut opt = options;
    let mode_for_report = if matches!(opt.mode, TranslationMode::Auto) {
        let m = detect_best_mode(pdf_bytes);
        opt.mode = m;
        match opt.mode {
            TranslationMode::Overlay => TranslationMode::Overlay,
            TranslationMode::InPlace => TranslationMode::InPlace,
            _ => TranslationMode::Overlay,
        }
    } else {
        match opt.mode {
            TranslationMode::Overlay => TranslationMode::Overlay,
            TranslationMode::NewDocument => TranslationMode::NewDocument,
            TranslationMode::InPlace => TranslationMode::InPlace,
            TranslationMode::Auto => TranslationMode::Overlay,
            TranslationMode::Bilingual => TranslationMode::Bilingual,
        }
    };

    let profile = clone_profile(&opt.profile);
    let debug_opts = opt.debug.clone();

    let raw = match opt.mode {
        TranslationMode::NewDocument => translate_pdf_new_document_full(pdf_bytes, opt).await?,
        TranslationMode::Overlay | TranslationMode::Auto => {
            crate::overlay::translate_pdf_overlay_full(pdf_bytes, opt).await?
        }
        TranslationMode::InPlace => crate::inplace::translate_pdf_inplace_full(pdf_bytes, opt).await?,
        TranslationMode::Bilingual => crate::overlay::translate_pdf_bilingual_full(pdf_bytes, opt).await?,
    };

    finalize_output(raw, profile, debug_opts, mode_for_report, None)
}

/// Quality-aware Auto cascade: InPlace → Overlay → NewDocument (Readable only).
async fn translate_pdf_auto_cascade(pdf_bytes: &[u8], options: TranslateOptions) -> Result<TranslateOutput> {
    let profile = clone_profile(&options.profile);
    let gate = QualityGate::from_profile(&profile);
    let debug_opts = options.debug.clone();

    // ── Stage 1: InPlace ─────────────────────────────────────────────────────
    let (inplace_bytes, inplace_stats) =
        crate::inplace::translate_pdf_inplace_inner(pdf_bytes, &options).await?;

    // Decide whether to cascade: high fallback rate OR quality gate would fail
    // (for Strict/PreserveLayout/Readable the gate is not permissive, so always
    // try a better mode).
    let inplace_quality_ok = inplace_stats.fallback_rate() <= 0.30 && gate.is_permissive();

    if inplace_quality_ok {
        // InPlace looks good — no cascade needed.
        let raw = TranslateOutput {
            pdf_bytes: inplace_bytes,
            quality: TranslateQuality {
                pages: vec![],
                overall: QualityResult::Pass,
                correction_rounds: 0,
                mode_used: TranslationMode::InPlace,
                fallback_reason: None,
            },
            debug: None,
        };
        return finalize_output(raw, profile, debug_opts, TranslationMode::InPlace, None);
    }

    let reason1 = format!(
        "InPlace overlay-fallback rate {:.0}% exceeded threshold; retried as Overlay",
        inplace_stats.fallback_rate() * 100.0
    );
    eprintln!("[harumi-ai] Auto cascade: {reason1}");

    // ── Stage 2: Overlay ─────────────────────────────────────────────────────
    // For Readable profile we may still cascade to NewDocument, so keep a clone.
    let nd_opts = if matches!(profile, QualityProfile::Readable) {
        let mut o = options.clone();
        o.mode = TranslationMode::NewDocument;
        Some(o)
    } else {
        None
    };

    let mut overlay_opts = options;
    overlay_opts.mode = TranslationMode::Overlay;
    let overlay_raw = crate::overlay::translate_pdf_overlay_full(pdf_bytes, overlay_opts).await?;

    // For Strict: gate must pass after Overlay or we error.
    if matches!(profile, QualityProfile::Strict) {
        return finalize_output(overlay_raw, profile, debug_opts, TranslationMode::Overlay, Some(reason1));
    }

    // For Readable: check if a NewDocument fallback is warranted.
    if matches!(profile, QualityProfile::Readable) {
        let overlay_passes = overlay_raw.quality.pages.iter()
            .all(|r| gate.evaluate(&r.summary).is_pass());
        if !overlay_passes
            && let Some(nd_opts) = nd_opts {
            let reason2 = format!(
                "{reason1}; Overlay quality gate still failed; retried as NewDocument"
            );
            eprintln!("[harumi-ai] Auto cascade: cascading to NewDocument");
            let nd_raw = translate_pdf_new_document_full(pdf_bytes, nd_opts).await?;
            return finalize_output(nd_raw, profile, debug_opts, TranslationMode::NewDocument, Some(reason2));
        }
    }

    finalize_output(overlay_raw, profile, debug_opts, TranslationMode::Overlay, Some(reason1))
}

/// Snapshot a `QualityProfile` value (QualityProfile doesn't derive Clone/Copy).
fn clone_profile(p: &QualityProfile) -> QualityProfile {
    match p {
        QualityProfile::PreserveLayout => QualityProfile::PreserveLayout,
        QualityProfile::Readable => QualityProfile::Readable,
        QualityProfile::Strict => QualityProfile::Strict,
        QualityProfile::BestEffort => QualityProfile::BestEffort,
    }
}

/// Evaluate gate, build debug artifacts, and assemble the final [`TranslateOutput`].
fn finalize_output(
    raw: TranslateOutput,
    profile: QualityProfile,
    debug_opts: DebugOptions,
    mode_used: TranslationMode,
    fallback_reason: Option<String>,
) -> Result<TranslateOutput> {
    let gate = QualityGate::from_profile(&profile);

    // Strict profile: fail on any gate violation.
    if matches!(profile, QualityProfile::Strict) && !gate.is_permissive() {
        let mut violations = Vec::new();
        for report in &raw.quality.pages {
            if let QualityResult::Fail(mut v) = gate.evaluate(&report.summary) {
                violations.append(&mut v);
            }
        }
        if !violations.is_empty() {
            return Err(Error::QualityGateFailed(violations));
        }
    }

    // Aggregate overall quality result.
    let overall = {
        let all_violations: Vec<_> = raw.quality.pages.iter()
            .flat_map(|r| gate.evaluate(&r.summary).violations().to_vec())
            .collect();
        if all_violations.is_empty() { QualityResult::Pass } else { QualityResult::Fail(all_violations) }
    };

    let needs_debug = debug_opts.layout_report
        || debug_opts.collision_report
        || debug_opts.overlay_pdf
        || debug_opts.correction_history;

    let debug = if needs_debug {
        let layout_report_json = if debug_opts.layout_report {
            let entries: Vec<serde_json::Value> = raw.quality.pages.iter()
                .map(|r| serde_json::json!({
                    "page_num": r.page_num,
                    "overflow_count": r.summary.overflow_count,
                    "collision_count": r.summary.collision_count,
                    "shrunk_count": r.summary.shrunk_count,
                    "worst_overlap_area": r.summary.worst_overlap_area,
                }))
                .collect();
            Some(serde_json::to_string_pretty(&entries).unwrap_or_default())
        } else { None };

        let correction_history = if debug_opts.correction_history {
            raw.debug.as_ref().map_or_else(Vec::new, |d| d.correction_history.clone())
        } else { Vec::new() };

        Some(DebugArtifacts {
            layout_report_json,
            collision_report_json: None,
            debug_overlay_pdf: raw.debug.as_ref().and_then(|d| {
                if debug_opts.overlay_pdf { d.debug_overlay_pdf.clone() } else { None }
            }),
            correction_history,
        })
    } else { None };

    Ok(TranslateOutput {
        pdf_bytes: raw.pdf_bytes,
        quality: TranslateQuality {
            pages: raw.quality.pages,
            overall,
            correction_rounds: raw.quality.correction_rounds,
            mode_used,
            fallback_reason,
        },
        debug,
    })
}

/// Detect the best [`TranslationMode`] for `pdf_bytes` based on layout structure.
///
/// Uses a quick heuristic on the first page: if more than 60 % of layout regions
/// are table cells (typical of dense SDS/form PDFs), `Overlay` is chosen; otherwise
/// `InPlace` is tried first.
fn detect_best_mode(pdf_bytes: &[u8]) -> TranslationMode {
    let Ok(mut doc) = harumi::Document::from_bytes(pdf_bytes) else {
        return TranslationMode::Overlay;
    };
    // Sample the first page only for speed.
    let Ok(frags) = doc.extract_text_runs(1) else {
        return TranslationMode::Overlay;
    };
    if frags.is_empty() {
        return TranslationMode::Overlay;
    }
    let page_size = doc.page(1).ok().and_then(|p| p.size().ok()).unwrap_or((595.0, 842.0));
    let regions = harumi::extract_layout_regions(
        &frags,
        page_size.0,
        page_size.1,
        harumi::LayoutRegionOptions::default(),
    );
    if regions.is_empty() {
        return TranslationMode::Overlay;
    }
    let table_count = regions
        .iter()
        .filter(|r| r.kind == harumi::LayoutRegionKind::TableCell)
        .count();
    let table_fraction = table_count as f32 / regions.len() as f32;

    // Dense form/SDS PDF → Overlay; otherwise try InPlace.
    if table_fraction > 0.60 {
        TranslationMode::Overlay
    } else {
        TranslationMode::InPlace
    }
}

async fn translate_pdf_new_document_full(pdf_bytes: &[u8], options: TranslateOptions) -> Result<TranslateOutput> {
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Phase 1: Extract (sync) ───────────────────────────────────────────────
    let mut doc = Document::from_bytes(pdf_bytes)?;
    let pages = extractor::extract_pages(&mut doc)?;
    drop(doc);

    if pages.is_empty() {
        let mut blank = Document::new((595.0, 842.0))?;
        let pdf_bytes = blank.save_to_bytes()?;
        return Ok(TranslateOutput {
            pdf_bytes,
            quality: TranslateQuality {
                pages: vec![],
                overall: crate::quality::QualityResult::Pass,
                correction_rounds: 0,
                mode_used: TranslationMode::NewDocument,
                fallback_reason: None,
            },
            debug: None,
        });
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
    let pdf_bytes = builder::build_pdf(&translated_pages, &options.font, &options.layout)?;
    Ok(TranslateOutput {
        pdf_bytes,
        quality: TranslateQuality {
            pages: vec![],
            overall: crate::quality::QualityResult::Pass,
            correction_rounds: 0,
            mode_used: TranslationMode::NewDocument,
            fallback_reason: None,
        },
        debug: None,
    })
}
