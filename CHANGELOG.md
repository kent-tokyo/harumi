# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased] — v0.2.0

### Added

- **`Document::extract_text_runs(page)`** — extracts positioned text runs from a page, returning `Vec<TextFragment>`.
  Each `TextFragment` carries `text` (Unicode string), `x`/`y` (PDF-point coordinates, bottom-left origin),
  `width` (estimated from advance widths), and `font_size`.
  Supports **Identity-H CID fonts** (Type0, as written by harumi) and **standard simple fonts**
  (Type1, MMType1, TrueType) with WinAnsiEncoding, MacRomanEncoding, StandardEncoding,
  or `/Encoding` dictionaries with `/Differences` arrays.
  Both `/ToUnicode` CMaps (`beginbfchar` and `beginbfrange`) and encoding-table fallback are handled.
  Literal PDF strings `(...)` as well as hex strings `<...>` are decoded in both `Tj` and `TJ` operators.
  Pending (not-yet-saved) operations are not included — call `save_to_bytes()` and reload first if needed.
  Returns `PageNotFound` for out-of-range page numbers.

- **`TextFragment`** — public struct returned by `extract_text_runs`: fields `text: String`, `x: f32`, `y: f32`, `width: f32`, `font_size: f32`.

- **`Document::new(size)`** — creates a blank single-page PDF from scratch (`size` is `(width, height)` in PDF points).
  Returns `InvalidInput` if size is zero, negative, or non-finite.
  Add more pages with `insert_blank_page`; add text or shapes with `page(1)?`.

- **`Document::extract_pages(page_numbers)`** — returns a new `Document` containing only the specified pages
  (1-indexed, caller-controlled order). Page content, fonts, and images are preserved;
  Outlines/Bookmarks, AcroForm, `/Names`, `/PageLabels`, `/OpenAction`, and `/StructTreeRoot` are stripped.
  Returns `InvalidInput` for empty or duplicate page numbers; `PageNotFound` for out-of-range numbers.
  The source document (`self`) is not modified.

- **`Document::merge_from(other)`** — appends all pages from `other` to the end of this document.
  All page content, fonts, and images from `other` are preserved.
  Outlines/Bookmarks, AcroForm, and `/Info` metadata are not carried over.
  `other` must have no unflushed pending operations (load with `from_file`/`from_bytes`
  or reload after `save_to_bytes()`).

- **Page manipulation API** — operate on the page tree without touching content streams
  - `Document::rotate_page(number, degrees)` — adds `degrees` (multiple of 90) to the page's `/Rotate` entry; accumulates on repeated calls; negative values rotate counter-clockwise
  - `Document::remove_page(number)` — removes a page and renumbers the rest; returns `PageNotFound` for invalid numbers, `InvalidInput` when removing the last page
  - `Document::insert_blank_page(after, (width, height))` — inserts a blank page at position `after` (0 = prepend, `page_count()` = append); handles nested `/Pages` trees by flattening
  - `Document::reorder_pages(new_order)` — reorders pages using 1-indexed old page numbers; validates length, range, and uniqueness
  - All four methods return `InvalidInput` if called after `save()`

- **`draw` feature** (zero extra dependencies)
  - `PageHandle::add_rect(rect, color, opacity)` — filled rectangle with per-channel RGB color and fill opacity
  - `PageHandle::add_line(from, to, color, line_width, opacity)` — stroked line segment
  - `ExtGStateRegistry` — deduplicates `/ExtGState` entries across draw ops on the same page
- **`image` feature** (adds `image` crate; enables `draw`)
  - `PageHandle::add_image(bytes, rect)` — embed JPEG or PNG at full opacity
  - `PageHandle::add_image_with_opacity(bytes, rect, opacity)` — embed with alpha
  - JPEG files are embedded without re-encoding (DCTDecode pass-through)
  - PNG and other formats are decoded to raw RGB and compressed with FlateDecode; alpha channel is composited against a white background
- `MediaBox` parent-chain traversal in `page.size()` (up to 32 hops, cycle-safe) — pages that inherit their MediaBox from a parent `/Pages` node are now handled correctly
- CFF2 variable font early detection: `save()` now returns a clear `FontParse` error instead of silently producing a broken PDF
- TTC collection magic-byte detection (`ttcf`) — `embed_font` now accepts `.ttc` files (index 0 is used; allsorts and ttf-parser handle TTC natively)

- **`Document::metadata()`** — reads the document's `/Info` dictionary, returning a `PdfMetadata` with `title`, `author`, `subject`, `keywords`, and `creator` fields (all `Option<String>`).
  Returns `PdfMetadata::default()` (all `None`) when no `/Info` dictionary is present.
  Handles UTF-16BE strings (BOM `\xFE\xFF`) as well as raw UTF-8/Latin-1 byte strings.

- **`Document::set_metadata(&PdfMetadata)`** — writes (or replaces) the `/Info` dictionary.
  Only `Some` fields are written to the dictionary; `None` fields are omitted.
  Can be called at any point before or after adding text/shapes — independent of font subsetting.

- **`PdfMetadata`** — public struct with `title`, `author`, `subject`, `keywords`, `creator` fields, all `Option<String>`. Derives `Debug`, `Clone`, `Default`, `PartialEq`.

### Fixed

- **`remove_page` correctness** — pending text/draw operations queued for a removed page
  are now discarded before `save()`, preventing a write to a deleted page object.
  The deleted page's dictionary object is also removed from the PDF object graph,
  reducing orphaned-object bloat. (Stream objects referenced by the page are not
  removed as they may be shared by other pages.)
- `build_widths_array`: replaced `unwrap()` with `unwrap_or(units_per_em)` — missing advance-width entries no longer panic
- `finalize()`: replaced `embedded.get().unwrap()` with `.ok_or(Error::InvalidFont(...))` — invalid font handles return an error instead of panicking
- `FontFile3` stream for CFF/OTF fonts now includes the `Length1` entry required by some validators

---

## [0.1.0] — 2026-04-xx

### Added

- `Document::from_file` / `from_bytes` — load existing PDFs
- `Document::save` / `save_to_bytes` — write without corrupting the original object graph
- `Document::embed_font(ttf_bytes)` — register a TrueType or OpenType font; subsetting is deferred to `save()` time
- `PageHandle::add_invisible_text` — OCR invisible text layer (render mode 3)
- `PageHandle::add_text` — visible text overlay with RGB color
- `PageHandle::add_invisible_text_runs` — batch API; one subset pass regardless of run count
- `page.size()` — query page dimensions from MediaBox (PDF points, width × height)
- Full CJK support: CID font object graph (`Type0 → CIDFontType2 → FontDescriptor → FontFile2`), ToUnicode CMap, GID remapping after allsorts subsetting
- `ocr` feature: `hocr_y_to_pdf`, `hocr_x_to_pdf`, `pixel_size_to_pt` coordinate helpers
- Verified end-to-end with `NotoSansJP-Regular.ttf` (Japanese), `NotoSansCJKsc-Regular.ttf` (Simplified Chinese), `NotoSansCJKkr-Regular.ttf` (Korean)
