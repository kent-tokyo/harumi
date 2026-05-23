//! # harumi
//!
//! A pure-Rust library for overlaying text onto existing PDFs, with full
//! support for CJK (Japanese / Chinese / Korean) fonts.
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
//! | Flag    | What it enables |
//! |---------|-----------------|
//! | `ocr`   | [`ocr`] module: helpers for converting Tesseract/hOCR pixel coordinates to PDF points |
//! | `draw`  | [`PageHandle::add_rect`], [`PageHandle::add_line`] — filled rectangles and stroked lines (no extra dependencies) |
//! | `image` | [`PageHandle::add_image`], [`PageHandle::add_image_with_opacity`], [`Document::extract_page_image`] — JPEG/PNG raster image overlay + extraction (enables `draw`, adds `image` crate) |

mod content;
mod document;
mod error;
mod extract;
mod font;
mod replace;

#[cfg(feature = "image")]
mod extract_image;

#[cfg(feature = "draw")]
pub(crate) mod draw;

#[cfg(feature = "ocr")]
pub mod ocr;

#[cfg(feature = "flow")]
pub mod flow;

pub use document::{Document, PageHandle, PdfMetadata, TextRun, VerticalAlign};
pub use error::{Error, Result};
pub use extract::TextFragment;
pub use font::FontHandle;

#[cfg(feature = "image")]
pub use extract_image::{PageImage, PageImageFormat};

#[cfg(feature = "flow")]
pub use flow::{FlowDocument, FlowOptions, Margins};

#[cfg(feature = "html")]
pub use flow::html::{render_html_to_pdf, HtmlRenderOptions};

// Re-export lopdf for integration test access.
#[doc(hidden)]
pub use lopdf;
