# harumi

**Overlay text, extract content, merge/split pages, draw shapes — all in pure Rust.**  
Full CJK (Japanese / Chinese / Korean) font support. Zero C dependencies. WASM-ready.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

[日本語](README_ja.md) | [中文](README_zh.md) | [한국어](README_kr.md)

---

## What harumi solves

**Before (without harumi):**  
Hand-assemble CID font objects from the PDF spec. Implement CMap generation, GID mapping, and subsetting in hundreds of lines. Still fight character rendering bugs.

**After (with harumi):**

```rust
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_invisible_text("検索対象テキスト", font, [72.0, 700.0], 12.0)?;
doc.save("searchable.pdf")?;
```

Font subsetting, CID encoding, and ToUnicode CMap generation are all automatic. The library handles it.

---

## What you get

| Challenge | harumi's answer |
|---|---|
| CJK font subsetting is complex | One `embed_font()` call — only used glyphs are included, GIDs correctly remapped |
| Don't want to corrupt existing PDF structure | Append-only: harumi never touches the original object graph |
| Need to run in WASM / Lambda / cross-compile | Pure Rust — zero C/C++ dependencies |
| Need OCR text at specific coordinates | `add_invisible_text` / batch `add_invisible_text_runs` |
| Need to stamp a watermark on PDFs | `add_text(color)` overlays visible text in any RGB color |
| Need to position text relative to page size | `page.size()` reads the MediaBox |
| Need in-memory output for Tauri / WASM | `save_to_bytes()` returns a `Vec<u8>` directly |
| Need to draw highlight rectangles or lines | `add_rect` / `add_line` (`draw` feature, no extra deps) |
| Need to draw a box border or polygon (callout) | `add_rect_stroke` / `add_polygon` (`draw` feature) |
| Need multi-line wrapped text in a box | `add_text_box` (no feature gate needed) |
| Need to embed JPEG / PNG images | `add_image` / `add_image_with_opacity` (`image` feature) |
| Need PNG transparency (signatures, watermarks) | Transparent PNGs use PDF SMask automatically — no white background |
| Need to rotate, remove, or reorder pages | `rotate_page` / `remove_page` / `insert_blank_page` / `reorder_pages` (no feature gate) |
| Need to merge two PDFs into one | `merge_from` appends all pages from another document; content and fonts preserved |
| Need to create a PDF from scratch (no existing file) | `Document::new(size)` creates a blank 1-page PDF; add pages with `insert_blank_page` |
| Need to split a PDF into separate files | `extract_pages` returns a new `Document` with the specified pages in any order |
| Need to extract text positions from an existing PDF | `extract_text_runs` decodes CID fonts and standard simple fonts (Type1, TrueType, WinAnsi, etc.) |
| Need to read or write PDF metadata (title, author…) | `doc.metadata()` reads `/Info`; `doc.set_metadata(&meta)` writes it |

---

## Why this gap existed

JS has [`pdf-lib`](https://pdf-lib.js.org/) — it handles font subsetting, CMap generation, and text layer composition transparently. In Rust, the existing options force you to choose between:

- **`lopdf`** — low-level binary surgery; you hand-assemble CID font objects from the PDF spec
- **`printpdf`** — create-only; cannot modify existing PDFs
- **`pdfium-render`** — C++ bindings that break WASM, cross-compilation, and Lambda deploys

`harumi` fills the gap.

---

## Quick Start

```toml
[dependencies]
harumi = "0.1"
```

### Invisible OCR text layer

```rust
use harumi::{Document, TextRun};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::from_file("scanned.pdf")?;

    // Embed a font — subsetting and CMap generation happen automatically at save()
    let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;

    // Overlay invisible OCR text on page 1
    doc.page(1)?.add_invisible_text(
        "ここにOCRで読み取った日本語テキスト",
        font,
        [100.0, 250.0], // x, y in PDF points (origin: bottom-left)
        12.0,
    )?;

    // Save — the original PDF structure is preserved
    doc.save("searchable_japanese.pdf")?;
    Ok(())
}
```

### Visible text overlay

```rust
// Overlay a red stamp centered on the page
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text(
    "CONFIDENTIAL",
    font,
    [w / 2.0 - 60.0, h / 2.0],
    24.0,
    [0.8, 0.0, 0.0], // red (RGB 0.0–1.0)
)?;
```

### In-memory output

```rust
// For Tauri commands, WASM, or any in-memory pipeline
let pdf_bytes: Vec<u8> = doc.save_to_bytes()?;
```

### Multi-line text box (no feature gate)

```rust
// Wraps at word boundaries (Latin) or any character (CJK); clips at box bottom
doc.page(1)?.add_text_box(
    "This is a long sentence that wraps inside a 200pt-wide bounding box.",
    font,
    [72.0, 400.0, 200.0, 120.0], // [x, y, width, height]
    12.0,
    [0.0, 0.0, 0.0],              // black
    0.0,                          // 0.0 = use font_size * 1.2 line height
)?;
```

### Page manipulation

```rust
// Rotate all pages 90° clockwise
for page_num in 1..=doc.page_count() {
    doc.rotate_page(page_num, 90)?;
}

// Remove a blank cover page
doc.remove_page(1)?;

// Insert a blank A4 title page before page 1
doc.insert_blank_page(0, (595.0, 842.0))?;

// Reverse page order in a 3-page document
doc.reorder_pages(&[3, 2, 1])?;

doc.save("output.pdf")?;
```

### Merge PDFs

```rust
let mut base = Document::from_file("a.pdf")?;
let appendix = Document::from_file("b.pdf")?;
base.merge_from(appendix)?;
base.save("merged.pdf")?;
```

Preserved: all page content, embedded fonts, images, resources.  
Not preserved: Outlines/Bookmarks, AcroForm, `/Info` metadata (author, creation date).

> **Precondition**: `other` must have no unflushed pending operations (freshly loaded, or reloaded after `save_to_bytes()`).

### Create a blank PDF

```rust
let mut doc = Document::new((595.0, 842.0))?;   // blank A4
let font = doc.embed_font(include_bytes!("NotoSansCJK-Regular.ttf"))?;
doc.page(1)?.add_text("Hello, world!", font, [72.0, 700.0], 24.0, [0.0, 0.0, 0.0])?;
doc.save("output.pdf")?;
```

### Extract pages

```rust
let doc = Document::from_file("large.pdf")?;
let mut excerpt = doc.extract_pages(&[3, 5, 7])?;  // pages 3, 5, 7 in that order
excerpt.save("excerpt.pdf")?;
```

### Extract text runs from an existing PDF

```rust
let doc = Document::from_file("existing.pdf")?;
let runs = doc.extract_text_runs(1)?;
for fragment in &runs {
    println!("{:?} at ({:.1}, {:.1})", fragment.text, fragment.x, fragment.y);
}
```

Works on arbitrary PDFs — Identity-H CID fonts (harumi output) and standard simple fonts (Type1, TrueType) with WinAnsiEncoding, MacRomanEncoding, StandardEncoding, or `/Differences` encoding dicts.

### Read/write PDF metadata

```rust
use harumi::{Document, PdfMetadata};

let mut doc = Document::from_file("report.pdf")?;

// Read existing metadata
let meta = doc.metadata()?;
println!("Title: {:?}", meta.title);

// Write new metadata (None fields are omitted from /Info)
doc.set_metadata(&PdfMetadata {
    title: Some("Annual Report 2026".into()),
    author: Some("Harumi Team".into()),
    subject: None,
    keywords: None,
    creator: None,
})?;
doc.save("report_with_meta.pdf")?;
```

### Draw shapes (`draw` feature)

```toml
harumi = { version = "0.1", features = ["draw"] }
```

```rust
// Yellow filled highlight rectangle (x, y, width, height in PDF points)
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// Blue border rectangle (stroke only, no fill)
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// Filled triangle (callout arrow tip)
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true,
)?;

// Black underline stroke
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;
```

### Embed images (`image` feature)

```toml
harumi = { version = "0.1", features = ["image"] }
```

```rust
let jpeg = std::fs::read("stamp.jpg")?;
// Place at [x, y, width, height]; supports JPEG (no decode) and PNG
doc.page(1)?.add_image(&jpeg, [72.0, 500.0, 100.0, 100.0])?;

// With opacity (0.0 = transparent, 1.0 = opaque)
doc.page(1)?.add_image_with_opacity(&jpeg, [72.0, 400.0, 100.0, 100.0], 0.75)?;

// PNG with alpha channel — transparent regions use PDF SMask, no white background
let sig_png = std::fs::read("signature.png")?;
doc.page(1)?.add_image(&sig_png, [72.0, 300.0, 200.0, 80.0])?;
```

---

## API Overview

```rust
// Load
let mut doc = Document::from_file("path/to/file.pdf")?;
let mut doc = Document::from_bytes(&bytes)?;

// Font embedding (one per font file; reuse the handle across pages)
let font: FontHandle = doc.embed_font(ttf_bytes)?;

// Page size (PDF points, width × height)
let (width, height) = doc.page(1)?.size()?;

// Invisible text — for OCR text layers
doc.page(1)?.add_invisible_text(text, font, [x, y], size)?;

// Visible text — for watermarks, stamps, annotations
doc.page(1)?.add_text(text, font, [x, y], size, [r, g, b])?;

// Batch placement (one subsetting pass — efficient for OCR output)
doc.page(1)?.add_invisible_text_runs(&[
    TextRun { text: "line one".into(), font, x: 72.0, y: 700.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
    TextRun { text: "line two".into(), font, x: 72.0, y: 685.0, font_size: 11.0, render_mode: 3, color: [0.0; 3] },
])?;

// Page structure (no feature gate)
doc.page_count()                          // u32
doc.rotate_page(n, degrees)?;             // multiple of 90; accumulates
doc.remove_page(n)?;                      // cannot remove the last page
doc.insert_blank_page(after, (w, h))?;    // after=0 prepends
doc.reorder_pages(&[new_order...])?;      // 1-indexed old page numbers
doc.extract_pages(&[n1, n2, ...])?;       // new Document with selected pages

// Create from scratch
Document::new((w, h))?;                   // blank 1-page PDF

// Merge documents (no pending ops in other)
doc.merge_from(other)?;             // append other's pages to end

// Save
doc.save("output.pdf")?;
doc.save_to_bytes()?;   // in-memory variant

// Extract text from existing PDFs (CID + standard simple fonts)
let runs: Vec<TextFragment> = doc.extract_text_runs(page_number)?;

// PDF metadata (/Info dictionary)
let meta: PdfMetadata = doc.metadata()?;
doc.set_metadata(&PdfMetadata { title: Some("...".into()), ..Default::default() })?;
```

### Coordinate system

Coordinates are in **PDF points** (1 pt = 1/72 inch), origin at the **bottom-left** of the page. If your OCR engine (e.g. Tesseract / hOCR) gives pixel coordinates from the top-left, use the `ocr` feature helper:

```toml
harumi = { version = "0.1", features = ["ocr"] }
```

### Feature flags

| Flag | What it enables | Extra dependencies |
|---|---|---|
| *(default)* | Text overlay, font embedding, `add_text_box` | lopdf, allsorts, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon` — shapes | none |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG raster images (enables `draw`) | `image` crate |
| `ocr` | `ocr::hocr_y_to_pdf` and helpers for Tesseract coordinate conversion | none |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
```

---

## Supported Fonts

| Font format | Status |
|---|---|
| TrueType (`.ttf`, `sfntVersion = 0x00010000`) | Supported |
| OpenType with CFF outlines (`.otf`, `OTTO`) | Accepted; subsetting depends on allsorts |
| TTC collections | Supported (index 0) |

For Japanese/Chinese/Korean, use the **TrueType** variant of [Noto Sans CJK](https://github.com/notofonts/noto-cjk) — end-to-end verified:

```
NotoSansCJKjp-Regular.ttf  (Japanese)
NotoSansCJKsc-Regular.ttf  (Simplified Chinese)
NotoSansCJKtc-Regular.ttf  (Traditional Chinese)
NotoSansCJKkr-Regular.ttf  (Korean)
```

> **OTF note**: harumi accepts `.otf` files and routes them through `FontFile3 /OpenType` embedding. However, allsorts v0.17 cannot subset all CFF variants (e.g. CFF2 variable fonts). If subsetting fails you will get a `FontParse` error at `save()` time. Use the TTF variants above for guaranteed compatibility.

---

## Internals

```
harumi
├── lopdf v0.40          — parse and modify existing PDF object graph
├── allsorts v0.17+      — TrueType font subsetting (used in Prince typesetter)
└── ttf-parser           — font metadata (bbox, units_per_em, ascender)
```

The font pipeline:

1. Parse used characters → collect Unicode code points
2. Map code points → original Glyph IDs via the font's `cmap` table (ttf-parser)
3. Subset the TTF to used glyphs only (allsorts); GIDs are **compacted to 0..N**
4. Remap `gid_to_char` and advance widths from original GIDs to the new compact GIDs
5. Build the CID font object graph: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. Generate a `/ToUnicode` CMap stream so viewers can copy/search the text
7. Append a new content stream to the page's `/Contents` array

Subsetting is **deferred**: `embed_font()` stores the raw TTF bytes; at `save()` time, harumi collects all characters used across every page, subsets once per font, and writes everything in one pass.

---

## Why "harumi"

晴海 — *haru* (clear sky) + *umi* (sea). Calm on the surface, a lot going on underneath.

---

## Roadmap

| Version | Scope |
|---|---|
| **v0.1** | TrueType fonts, invisible + visible text, batch placement, `page.size()`, `save_to_bytes()`, GID remapping fix, OTF accepted |
| **v0.2** | `draw` feature (`add_rect`, `add_line`), `image` feature (`add_image`, `add_image_with_opacity`), CFF2 early error, TTC magic detection, MediaBox parent-chain traversal |
| **v0.3** | `add_text_box`, `add_rect_stroke`, `add_polygon`; security hardening (NaN guards, double-save protection, indirect Contents array, JPEG marker parser fix, PNG overflow) |
| **v0.4** | PNG true transparency (SMask) — transparent PNGs rendered without white background |
| **v0.5** | `add_text_with_opacity`, `add_text_box_aligned` (VerticalAlign), `add_polyline`, `add_text_box_with_opacity` — **Done** |
| **v0.6** | Page manipulation — `rotate_page`, `remove_page`, `insert_blank_page`, `reorder_pages` — **Done** |
| **v0.7** | `merge_from` (PDF merging), `remove_page` correctness & orphan-object fix — **Done** |
| **v0.8** | `Document::new` (blank PDF from scratch), `extract_pages` (page splitting) — **Done** |
| **v0.9** | `extract_text_runs` (CID + standard simple fonts), PDF metadata read/write (`metadata()`, `set_metadata()`, `PdfMetadata`) — **Done** |
| **Next (v0.10+)** | `#[non_exhaustive]` on Error, MSRV declaration, WASM CI, publish to crates.io |

---

## Contributing

Issues and PRs welcome at [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi).

The most complex part of this codebase is `src/font/embed.rs` — the CID font object graph construction. When reporting rendering bugs in a specific PDF viewer, include the viewer name and version in your issue.

---

## License

MIT OR Apache-2.0
