//! AI-powered PDF translation for digital and scanned PDFs, built on [harumi].
//!
//! ## Digital PDFs
//! Extracts positioned text, translates via an LLM, and writes back using
//! layout-aware overlay or in-place replacement — producing CIDFontType2 output
//! compatible with PSPDFKit.
//!
//! ## Scanned PDFs
//! Accepts OCR JSON in HierText format ([`InputTextSource::OcrJson`]) from
//! ocrs-cjk, PaddleOCR, or any compatible tool. Translates recognized regions,
//! covers original image text with a white mask, and overlays translated text —
//! without rasterizing the page.
//!
//! # Quick start
//! ```no_run
//! use harumi_ai::{translate_pdf, TranslateOptions, providers::EchoTranslator};
//!
//! #[tokio::main]
//! async fn main() {
//!     let font = std::fs::read("NotoSansJP-Regular.ttf").unwrap();
//!     let pdf = std::fs::read("source.pdf").unwrap();
//!
//!     let opts = TranslateOptions::new("en", EchoTranslator, font);
//!     let output = translate_pdf(&pdf, opts).await.unwrap();
//!     std::fs::write("translated.pdf", output.pdf_bytes).unwrap();
//! }
//! ```
//!
//! # Known limitations
//! - **Layout**: write-back uses extracted coordinates and inferred regions; it does
//!   not guarantee pixel-identical output. Complex tables, rotated or vertical text,
//!   and text over non-uniform image backgrounds require visual review.
//! - **Font and style**: translated text uses the configured primary/fallback fonts;
//!   source font styling is not preserved run-for-run in every translation mode.
//! - **Paragraph classification**: `extract_text_chunks` uses font-size heuristics;
//!   complex PDFs may have imperfect heading/paragraph classification.

mod builder;
pub mod cache;
mod error;
mod extractor;
pub(crate) mod font_sizing;
mod inplace;
mod layout;
mod layout_repair;
pub mod ocr_input;
mod output;
mod overlay;
mod pdf_translator;
mod prompts;
pub mod providers;
mod quality;
mod repair;
mod translator;

pub use cache::TranslationCache;
pub use error::{Error, Result};
pub use font_sizing::FontSizePolicy;
pub use layout::LayoutOptions;
pub use layout_repair::{
    LayoutCorrection, LayoutRepairMode, RasterizeOptions, VisionProvider, VisionRepairRequest,
};
pub use ocr_input::OcrRegion;
pub use output::{
    CorrectionRound, DebugArtifacts, DebugOptions, PageQualityReport, TranslateOutput,
    TranslateQuality,
};
pub use pdf_translator::{
    InputTextSource, OverflowStrategy, TranslateOptions, TranslateOptionsBuilder, TranslationMode,
    translate_pdf,
};
pub use quality::{QualityGate, QualityProfile, QualityResult, QualityViolation};
pub use translator::Translator;
