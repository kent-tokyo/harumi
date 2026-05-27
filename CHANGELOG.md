# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.5.0] — 2026-05-27

### Added

- **`Document::add_bookmark`** — appends a named PDF document outline entry (flat bookmarks list).
  Bookmarks are visible in the PDF viewer's navigation/outline panel.
  Title strings containing non-ASCII characters (CJK, accented Latin, etc.) are automatically
  encoded as UTF-16BE with BOM for full Unicode compatibility.

- **`PageHandle::add_link_url`** — adds an invisible URI link annotation to a page.
  Clicking the area in a PDF viewer navigates to the given URL. Uses the standard `/A /URI` action.

- **`PageHandle::add_link_internal`** — adds an invisible internal link annotation that jumps to
  a specific page number within the same document. Uses a `/Dest [pageRef /XYZ]` destination.

- **`HeaderFooter`** (`flow` feature) — header/footer configuration struct for [`FlowDocument`].
  Set `FlowOptions::header` or `FlowOptions::footer` to render left / center / right text on
  every page. Supports `{{page}}` and `{{total}}` placeholder substitution at render time.
  Includes a `HeaderFooter::page_number()` convenience constructor.

- **`FlowOptions::header` / `FlowOptions::footer`** (`flow` feature) — `Option<HeaderFooter>`
  fields on `FlowOptions` (both default to `None`).

- **`FlowOptions::auto_bookmarks`** (`flow` feature) — when `true` (default), every
  `push_heading` call automatically records a PDF bookmark pointing to the top of that heading.
  Set to `false` to suppress outline generation.

### Fixed

- **`build_outlines_from_bookmarks`** now **merges** new bookmarks into an existing `/Outlines`
  tree instead of overwriting it. Loading a PDF that already has bookmarks and calling
  `add_bookmark()` no longer silently discards the original outline entries.
- **`set_metadata()`** now returns `Error::InvalidInput` when called after `save()` / `save_to_bytes()`,
  matching the `finalized` guard on all other mutating methods.
- **`hf_measure` fallback** in `FlowDocument` now uses `text.chars().count()` instead of
  `text.len()` (byte length). This fixes right-aligned and centered header/footer text being
  mis-positioned when the font face is unavailable and the text contains CJK or other multi-byte
  characters.
- **`parse_bfrange_line`** now uses `checked_add` to guard against `u32` overflow in adversarially
  crafted ToUnicode CMap streams. Overflow silently broke Unicode extraction; now the range is
  truncated instead.

### Changed

- **`add_link_url`** doc comment adds an explicit security note: callers are responsible for
  ensuring `url` does not contain `javascript:`, `data:`, or other potentially unsafe URI schemes.

### Internal

- `pdf_text_string` helper: ASCII strings use literal encoding; non-ASCII strings use UTF-16BE
  with BOM for /Title and similar text-string fields.
- `build_link_annot_base` + `append_annotation_to_page` helpers handle direct/indirect /Annots
  arrays and missing /Annots entries on existing pages.
- `find_cross_op_matches` and `find_cross_op_matches_preserve` merged into a shared
  `find_cross_op_matches_inner` function, eliminating ~150 lines of duplicated logic.
  `CrossOpMatchPreserve` struct removed; `CrossOpMatch` now used by both code paths.
- All Clippy warnings resolved (`is_multiple_of`, `excessive_precision`, `needless_late_init`,
  `too_many_arguments`).

---

## [0.4.2] — 2026-05-23

### Fixed

- Corrected broken intra-doc link: `Error::InvalidInput` → `crate::Error::InvalidInput` in `flow/mod.rs`.

---

## [0.4.1] — 2026-05-23

### Added

- **`Document::extract_page_image`** (`image` feature) — extracts the embedded raster image from a
  single-image scanned PDF page. Returns a `PageImage` with `format` (`Jpeg` or `Png`) and `data` bytes.
  Useful for round-tripping scanned PDFs: load with `from_file`, call `extract_page_image`, process the
  raw image, then re-embed with `add_image`.

---

## [0.4.0] — 2026-05-23

### Added

- **`flow` feature** (`draw` implied) — `FlowDocument`, a push-style document builder with automatic pagination.
  Push block elements in order; page breaks are inserted automatically when content overflows a page.
  - `FlowDocument::new(font_bytes, options)` — create a document with an embedded font
  - `push_heading(text, level)` — heading at level 1–6; font size scaled by `FlowOptions::heading_size_scale`
  - `push_paragraph(text)` — body-text paragraph with automatic word wrapping (Latin word boundaries / CJK break-anywhere)
  - `push_key_value_table(rows)` — two-column key/value table with light-gray horizontal separator lines
  - `push_list(items, ordered)` — bulleted (`•`) or numbered list
  - `push_page_break()` — explicit page break
  - `render()` → `Vec<u8>` — finalize and return the PDF bytes
  - `FlowOptions` — `page_size`, `margins`, `body_font_size`, `heading_size_scale`, `line_height_factor`, `paragraph_spacing`, `table_key_ratio`, `max_pages`
  - `Margins` — `uniform(pt)` and `a4_standard()` (≈ 20 mm, 56.7 pt, on all sides)

- **`html` feature** (implies `flow`) — `render_html_to_pdf(html, options) -> Result<Vec<u8>>` converts an HTML string to PDF bytes.
  Backed by `FlowDocument`; HTML is parsed with `scraper` (html5ever-based).
  - Supported elements: `<h1>`–`<h6>`, `<p>`, `<table>/<tr>/<th>/<td>`, `<ul>/<ol>/<li>`, `<div>/<section>/<article>/<body>` (block containers)
  - Page breaks: `style="page-break-after: always"` or `class="page-break"`
  - Skipped entirely: `<head>`, `<script>`, `<style>`, `<meta>`, `<link>`, `<noscript>`
  - Deeply nested HTML (5 000+ div levels) handled without stack overflow (iterative DFS walker)
  - `HtmlRenderOptions` — `font_bytes` (required), `page_size`, `margins`, `body_font_size`, `line_height_factor`, `max_pages`
  - `HtmlRenderOptions::font_bytes` is required; returns `Error::InvalidInput` when empty

### Security

- **`max_pages` limit** (`flow` and `html` features) — `FlowOptions::max_pages` and `HtmlRenderOptions::max_pages` (both default 2000) cap the number of pages that may be generated. `ensure_space` returns `Error::InvalidInput` if the limit would be exceeded, preventing unbounded memory growth when rendering untrusted HTML.
- **Iterative HTML tree walker** — `walk_iterative` uses an explicit `Vec` stack instead of recursion, preventing stack overflows from deeply nested HTML (tested with 5 000 `<div>` levels).

---

## [0.3.0] — 2026-05-21

### Added

- **`PageHandle::add_text_with_rotation`** — overlays text at an arbitrary counter-clockwise rotation (degrees).
  Uses the PDF `Tm` (text matrix) operator: `cos(θ) sin(θ) -sin(θ) cos(θ) x y Tm`.
  Zero degrees falls back to the standard `Td` operator for backward compatibility.
  Accepts the same font/size/color/opacity parameters as `add_text_with_opacity`.

- **Simultaneous fill + stroke for shapes** (`draw` feature) — `add_ellipse` and `add_polygon` now accept
  a `stroke_width: f32` parameter (appended as the last argument). When both `filled = true` and
  `stroke_width > 0.0`, the PDF `B` (fill-then-stroke) operator is used. Setting `stroke_width = 0.0`
  preserves the previous behavior (`f` / `S` depending on `filled`).
  **Breaking change**: callers must append `0.0` (or a positive stroke width) to existing calls.

- **`PageHandle::add_path`** (`draw` feature) — unified path API that subsumes `add_polygon` and `add_polyline`.
  Parameters: `points`, `closed: bool`, `color`, `filled: bool`, `stroke_width: f32`, `opacity`.
  `closed = true` appends the PDF `h` (closepath) operator; `closed = false` leaves the path open.
  Supports the same fill/stroke/both modes as the updated `add_ellipse`/`add_polygon`.
  The existing `add_polygon` and `add_polyline` methods are unchanged (additive, not replaced).

- **Cross-operator text replace** — `replace_text` and `replace_text_preserve_font` now match
  `old_text` that is split across consecutive `Tj`/`TJ` operators within the same font context
  (same `Tf` operator, same `BT`/`ET` block). Previously only single-operator exact matches were supported.
  Matches that span a positional operator (`Td`, `Tm`, etc.) between the operators are intentionally
  skipped to avoid reordering text. `can_replace_text` likewise counts cross-operator matches.

- **`TextFragment::font_name`** — PDF resource name of the font at the extracted position (e.g. `"HR0"`, `"F1"`). Useful for identifying which font family a run belongs to, especially for CJK glyph diagnostics.
- **`TextFragment::color`** — RGB fill color `[f32; 3]` at the extracted position, tracking the most recent `rg` or `g` content-stream operator. Defaults to black `[0.0, 0.0, 0.0]`.
- **`TextFragment::invisible`** — `true` when the text render mode is 3 (OCR search layer, `Tr 3`). Lets callers distinguish invisible OCR text from visible content.
- **`TextFragment` is now `#[non_exhaustive]`** — future field additions will not require semver breaking changes.
- **`PageHandle::replace_text` now returns `Result<usize>`** (was `Result<()>`). The return value is the number of occurrences of `old_text` found on the page. A return value of `0` means no match was found and no operation is queued. The match count is computed eagerly at call time (read-only scan).
- **`PageHandle::replace_text_preserve_font` now returns `Result<usize>`** (was `Result<()>`). Glyph validation is now performed eagerly at call time — `Err(FontCharNotMapped)` is returned immediately if any character in `new_text` is absent from the existing font's ToUnicode mapping, enabling 1-pass fallback patterns without waiting until `save()`.
- **`PageHandle::can_replace_text(old_text, new_text) -> Result<usize>`** — pure read-only scan that returns the number of occurrences of `old_text` and validates that all characters in `new_text` are present in the existing font's subset. No document modification. Useful for preflight checks before deciding which replace method to call.
- **`PageHandle::add_ellipse(rect, color, opacity, filled)`** (`draw` feature) — draws an ellipse or circle approximated with 4 cubic Bézier curves (`c` operator). `filled = true` uses `rg`/`f`; `filled = false` uses `RG`/`S`.

### Changed

- **`replace_text_preserve_font` validation timing**: `FontCharNotMapped` is now returned at call time instead of `save()` time. This is a behavioral change for callers that previously relied on the error being deferred.

---

## [0.2.0] — 2026-05-16

### Added

- **`PageHandle::replace_text_preserve_font(old_text, new_text)`** — in-place text replacement that reuses the font already embedded in the PDF at the matched position. No `FontHandle` is required: harumi reads the font reference from the preceding `Tf` operator.
  If any character in `new_text` is absent from the font's ToUnicode mapping (e.g. the font was subsetted and the glyph was not included), `save()` returns `Error::FontCharNotMapped` so the caller can fall back to `replace_text` with an explicit font.
  Width compensation (`Td`) is applied automatically.

- **`PageHandle::replace_text(old_text, new_text, font)`** — true in-place text replacement in existing PDF content streams.
  Decodes existing `Tj` and `TJ` operators, locates the first occurrence where the decoded Unicode string matches `old_text`,
  and rewrites the stream with the new text encoded in `font` (a newly embedded font).
  Font-switching (`Tf`) is injected automatically; a `Td` operator is appended after each replacement to compensate for
  the width difference between the old and new glyphs, preventing subsequent text from drifting.
  For `TJ` arrays, the matching element is split out as a standalone `Tj` so the `Tf` can appear outside the array.
  Returns `Ok(())` without modifying the PDF if `old_text` is not found on the page.
  **Limitation**: `old_text` must match the complete decoded content of one `Tj` operator or one string element within a
  `TJ` array. Text that spans multiple operators is not matched.

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
