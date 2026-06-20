//! # harumi
//!
//! Pure-Rust PDF library — CJK font embedding (Chinese/Japanese/Korean),
//! OCR text overlay, text extraction, HTML→PDF, page merge/split.
//! Zero C/C++ dependencies. WASM-compatible.
//!
//! ## Use cases
//!
//! | Scenario | Key API |
//! |---|---|
//! | OCR invisible text layer | `add_invisible_text` · `add_invisible_text_runs` |
//! | AI / RAG text extraction | `extract_text_runs` · `extract_text_chunks` · `extract_as_markdown` |
//! | PDF watermark / stamp | `add_text` · `add_text_with_rotation` |
//! | Scanned PDF → searchable | `add_invisible_text` + hOCR helpers (`ocr` feature) |
//! | HTML → PDF | `render_html_to_pdf` (`html` feature) |
//! | PDF text replacement | `replace_text` · `replace_text_resubset` |
//! | Page merge / split | `merge_from` · `extract_pages` |
//! | Digital signature creation | `sign_document` · `add_signature_field` (`digital-signature` feature) |
//! | WASM / Edge / Lambda | All APIs — zero C/C++ dependencies |
//!
//! ## Motivation
//!
//! Rust lacks a high-level, zero-C-dependency library for injecting text into
//! existing PDFs. Low-level crates like `lopdf` expose the raw PDF object graph
//! and require manual CID font assembly. `harumi` wraps that complexity behind
//! a simple, ergonomic API.
//!
//! ## Quick start
//!
//! ```no_run
//! use harumi::{Document, TextRun};
//!
//! # fn main() -> harumi::Result<()> {
//! let mut doc = Document::from_file("scanned.pdf")?;
//! let font = doc.embed_font(include_bytes!("../tests/fixtures/NotoSansJP-Regular.ttf"))?;
//!
//! // Invisible OCR text layer
//! doc.page(1)?.add_invisible_text("日本語テキスト", font, [72.0, 700.0], 12.0)?;
//!
//! // Visible red label
//! doc.page(1)?.add_text("CONFIDENTIAL", font, [72.0, 750.0], 18.0, [0.8, 0.0, 0.0])?;
//!
//! doc.save("output.pdf")?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Coordinate system
//!
//! All coordinates are in **PDF points** (1 pt = 1/72 inch). The origin is at
//! the **bottom-left** of the page. Use [`page.size()`](PageHandle::size) to
//! query the page dimensions and position text relative to them.
//!
//! ## Font subsetting
//!
//! [`embed_font`](Document::embed_font) stores the raw TTF bytes without
//! processing. At [`save`](Document::save) time, harumi collects every
//! character used across all pages, runs a single subset per font, and embeds
//! the result. This means subsetting overhead is paid once regardless of how
//! many pages or text runs reference the same font.
//!
//! ## Feature flags
//!
//! | Flag                | Enables | Extra deps |
//! |---------------------|---------|------------|
//! | `ocr`               | hOCR pixel→PDF coordinate helpers | none |
//! | `draw`              | Shapes: rect, line, ellipse, polygon, path | none |
//! | `image`             | JPEG/PNG embed + extraction; enables `draw` | `png` crate |
//! | `flow`              | `FlowDocument` auto-pagination builder + headers/footers | none |
//! | `html`              | HTML→PDF renderer; enables `flow` | none (internal tokenizer) |
//! | `digital-signature` | Create and verify PKCS#7/CMS signatures | RustCrypto crates |

mod chunk;
mod content;
mod document;
mod error;
mod extract;
mod font;
mod replace;
mod resubset;

#[cfg(feature = "image")]
mod extract_image;

#[cfg(feature = "draw")]
pub(crate) mod draw;

#[cfg(feature = "ocr")]
pub mod ocr;

#[cfg(feature = "flow")]
pub mod flow;

pub mod signature;

#[cfg(feature = "digital-signature")]
pub mod signature_create;

#[cfg(feature = "digital-signature")]
pub(crate) mod cms_builder;

#[cfg(feature = "digital-signature")]
pub(crate) mod pdf_incremental;

pub use chunk::{ChunkType, TextChunk};
pub use document::{
    AttachmentInfo, BatchEntry, BoxFitOptions, Color, Document, FieldType, FitOptions, FitResult,
    FormField, FragmentReplaceFailureReason, FragmentReplaceOpts, OverflowPolicy, PageHandle,
    PdfMetadata, ReplaceOptions, TextFieldOptions, TextRun, VerticalAlign, calculate_text_width,
    font_covers_char, glyph_advance_pt, wrap_paragraph,
};
pub use error::{Error, Result};
pub use extract::{
    Collision, ColumnZone, ExtractionWarning, GroupingStrategy, LayoutRegion, LayoutRegionKind,
    LayoutRegionOptions, PlacedBox, RegionFitPlan, TableCell, TextFragment, TextGroup, WarningKind,
    detect_collisions, detect_text_columns, extract_layout_regions, extract_table_cells,
    group_text_fragments, merge_short_cjk_tails, sort_by_reading_order, text_fragment_bounds,
};
pub use font::FontHandle;

#[cfg(feature = "image")]
pub use extract_image::{PageImage, PageImageFormat};

#[cfg(feature = "flow")]
pub use flow::{FlowDocument, FlowOptions, HeaderFooter, InlineSpan, Margins};

#[cfg(feature = "html")]
pub use flow::html::{HtmlRenderOptions, render_html_to_pdf};

pub use signature::SignatureInfo;

#[cfg(feature = "digital-signature")]
pub use signature_create::{
    CertificateInput, PrivateKeyInput, SignatureFieldOptions, SigningContext,
};

// Re-export lopdf for integration test access.
#[doc(hidden)]
pub use lopdf;
