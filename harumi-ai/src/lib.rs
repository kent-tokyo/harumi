//! AI-powered PDF translation orchestration built on [harumi].
//!
//! This crate sits on top of harumi's text extraction and font-embedding APIs.
//! It extracts structured paragraphs from a source PDF, translates them via an
//! LLM, and assembles a new PDF using harumi's direct `add_text` API —
//! producing CIDFontType2 output that is compatible with PSPDFKit.
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
//! - **Layout**: flow-based reconstruction only. Tables, multi-column layouts, images,
//!   and font colours are not reproduced.
//! - **Font**: all output text uses the single font provided in `TranslateOptions::font`.
//! - **Paragraph classification**: `extract_text_chunks` uses font-size heuristics;
//!   complex PDFs may have imperfect heading/paragraph classification.

pub mod cache;
mod error;
mod extractor;
mod builder;
mod inplace;
mod layout;
mod output;
mod overlay;
mod pdf_translator;
mod prompts;
mod quality;
mod repair;
mod translator;
pub mod providers;

pub use cache::TranslationCache;
pub use error::{Error, Result};
pub use layout::LayoutOptions;
pub use output::{CorrectionRound, DebugArtifacts, DebugOptions, TranslateOutput, TranslateQuality,
                 PageQualityReport};
pub use pdf_translator::{OverflowStrategy, TranslateOptions, TranslateOptionsBuilder,
                         TranslationMode, translate_pdf};
pub use quality::{QualityGate, QualityProfile, QualityResult, QualityViolation};
pub use translator::Translator;
