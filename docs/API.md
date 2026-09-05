# harumi — API Overview

## Core API

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

// Merge documents
doc.merge_from(other)?;             // append other's pages to end

// Save
doc.save("output.pdf")?;
doc.save_to_bytes()?;   // in-memory variant

// Extract text from existing PDFs (CID + standard simple fonts)
let runs: Vec<TextFragment> = doc.extract_text_runs(page_number)?;
// `run.rotation_degrees` preserves common 90°/270° text direction.
// `run.color` and `run.opacity` expose the active fill style when present.

// Include non-fatal diagnostics such as missing ToUnicode CMaps or skipped streams.
let (runs, warnings) = doc.extract_text_runs_verbose(page_number)?;

// PDF metadata (/Info dictionary)
let meta: PdfMetadata = doc.metadata()?;
doc.set_metadata(&PdfMetadata { title: Some("...".into()), ..Default::default() })?;

// Replace text in existing content stream; returns match count
let n: usize = doc.page(1)?.replace_text(old_text, new_text, font)?;
let n: usize = doc.page(1)?.replace_text_preserve_font(old_text, new_text)?;
let n: usize = doc.page(1)?.can_replace_text(old_text, new_text)?;
let n: usize = doc.page(1)?.replace_text_resubset(old, new, font_bytes)?;

// Link annotations (no feature gate)
doc.page(1)?.add_link_url([x, y, w, h], "https://example.com")?;
doc.page(1)?.add_link_internal([x, y, w, h], target_page)?;

// Document outline / bookmarks
doc.add_bookmark("Section Title", page, y)?;

// Markup annotations
doc.page(1)?.add_highlight([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_underline([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_strikeout([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_squiggly([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_sticky_note([x, y], "comment text")?;

// AcroForm
let fields: Vec<FormField> = doc.form_fields()?;
let n: usize = doc.fill_form(&[("field_name", "value")])?;

// Page boxes
let cb: Option<[f32; 4]> = doc.page(1)?.crop_box()?;
doc.page(1)?.set_crop_box([x, y, w, h])?;

// Password protection
Document::from_file_with_password(path, password)?;
doc.set_encryption(user_pw, owner_pw)?;
```

---

## Coordinate System

Coordinates are in **PDF points** (1 pt = 1/72 inch), origin at the **bottom-left** of the
visible page area. Extraction normalizes inherited `CropBox`/`MediaBox` origins, `/UserUnit`,
and right-angle page `/Rotate` into that local coordinate system. Content writing APIs still
accept the source PDF's ordinary page coordinates; use the page box and rotation metadata when
placing new content on rotated or cropped pages.

For OCR tools that output pixel coordinates from the top-left, use the `ocr` feature helper:

```rust
harumi = { version = "1", features = ["ocr"] }
```

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## Feature Flags

| Flag | What it enables |
|---|---|
| *(default)* | Text overlay, font embedding, `add_text_box`, metadata, annotations, AcroForm, page ops |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_ellipse`, `add_path` |
| `image` | `add_image`, `add_image_with_opacity`, `extract_page_image`, `extract_page_images` (enables `draw`) |
| `ocr` | `ocr::hocr_y_to_pdf`, `ocr::hocr_x_to_pdf`, `ocr::pixel_size_to_pt` |
| `flow` | `FlowDocument` builder with auto-pagination, headers/footers, inline styling |
| `html` | `render_html_to_pdf` — HTML → PDF (enables `flow`) |
| `digital-signature` | `verify_signatures`, `add_signature_field`, `sign_document` |

---

## Supported Fonts

| Font format | Status |
|---|---|
| TrueType (`.ttf`, `sfntVersion = 0x00010000`) | Fully supported — pure-Rust subsetting |
| TrueType Collections (`.ttc`) | Fully supported — face index via `embed_font_at(bytes, face_index)` |
| OpenType with CFF outlines (`.otf`, `OTTO`) | Accepted — embedded as-is (no subsetting) |

For CJK, use the TrueType variant of [Noto Sans CJK](https://github.com/notofonts/noto-cjk):

```
NotoSansCJKjp-Regular.ttf  (Japanese)
NotoSansCJKsc-Regular.ttf  (Simplified Chinese)
NotoSansCJKtc-Regular.ttf  (Traditional Chinese)
NotoSansCJKkr-Regular.ttf  (Korean)
```

---

## Internals

```
harumi
├── lopdf v0.42          — parse and modify existing PDF object graph
├── ttf-parser           — font metadata (bbox, units_per_em, ascender)
└── [internal TTF subsetter] — pure-Rust TrueType subsetting (no external crates)
```

Flow/HTML generation is a new-document typesetting path. It does not promise
pixel-identical reproduction of an existing PDF. For translation overlays,
text that intersects an image preserves the image and is reported as a Major
`image_overlap` issue; background restoration and automatic relocation are not
performed.

Subsetting is **deferred**: `embed_font()` stores raw TTF bytes; at `save()` time, harumi
collects all characters used across every page, subsets once per font, and writes everything
in one pass.

Extraction is best-effort for malformed or underspecified PDFs. In particular, a Type0/CIDFont
without a usable `/ToUnicode` CMap may use an Identity-H/V fallback. Use
`extract_text_runs_verbose()` when the distinction between decoded text and inferred text matters.
The verbose API also reports `WarningKind::UnsupportedFontSubtype` when a font resource has a
missing or unsupported `/Subtype`; text using that font is skipped rather than reported as
successfully decoded. It reports `WarningKind::UnsupportedVerticalWriting` for Type0 fonts using
`/Identity-V`; text recovery may succeed, but vertical metrics and reflow remain best-effort.
The `TextFragment` bounding box is axis-aligned; complex vertical writing and mixed styles may
require visual verification.

`TextFragment::opacity` is the effective non-stroking alpha from page-level `/ExtGState /ca`.
Opacity from nested Form XObjects with private resources and mixed per-glyph styles remains
best-effort and should be checked with a renderer.
