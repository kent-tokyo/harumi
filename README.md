# harumi

**Layout-preserving CJK PDF editing in pure Rust.**

harumi extracts positioned text from existing PDFs, lets you translate or replace it,
and writes the result back while preserving the original page layout.
CID fonts, CMaps, Unicode mapping, font subsetting, text fitting, and layout
collision checks are all handled automatically.

[![Crates.io](https://img.shields.io/crates/v/harumi.svg)](https://crates.io/crates/harumi)
[![docs.rs](https://docs.rs/harumi/badge.svg)](https://docs.rs/harumi)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen)](https://kent-tokyo.github.io/harumi/)

[中文](README_zh.md) | [日本語](README_ja.md) | [한국어](README_kr.md)

**[Try the live browser demo →](https://kent-tokyo.github.io/harumi/)** — annotation editor (text · rect · line · freehand pen) running entirely in your browser via WASM

**Use harumi for:**
- Digital PDF translation (extract text → translate with an LLM → write back with layout intact)
- Scanned PDF translation (pass OCR JSON → translate → mask original → overlay translated text)
- OCR searchable text layers on scanned PDFs
- Japanese / Chinese / Korean text overlays and stamps
- Page manipulation, annotation, form editing, and PDF merging
- WASM, Lambda, Tauri, and MCP-based AI document workflows

> Not an OCR engine. Not a PDF viewer.  
> harumi is the PDF write-back layer for document automation.

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

## Pair with an OCR engine or LLM

**ocrs-cjk reads. LLMs translate. harumi writes back.**

harumi is the PDF write-back layer. Whatever reads or rewrites the document —
an OCR engine, a translation model, or an AI agent — harumi handles the final step:
putting Unicode/CJK text back into the PDF without rasterizing the page.

```
scanned.pdf
  └─ ocrs-cjk (--json)
       ├─ recognized text
       ├─ bounding boxes
       └─ confidence scores
  └─ harumi (this library)
       ├─ CJK font subsetting + ToUnicode CMap
       ├─ invisible text layer (render mode 3)
       └─ append-only save → original image untouched

=> searchable.pdf  (text is selectable and indexable in any PDF viewer)
```

```rust
// Minimal sketch — see examples/ocrs_cjk_to_searchable_pdf.rs for the full pipeline.
let mut doc = Document::from_file("scanned.pdf")?;
let font = doc.embed_font(&std::fs::read("NotoSansCJKjp-Regular.ttf")?)?;

for word in ocr_words {                         // from ocrs-cjk JSON
    if word.confidence >= 0.5 {
        doc.page(1)?.add_invisible_text(
            &word.text, font, [word.pdf_x, word.pdf_y], word.font_size,
        )?;
    }
}
doc.save("searchable.pdf")?;
```

Run the full example with the bundled fixture:
```bash
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable.pdf
```

### AI-assisted OCR correction

OCR engines sometimes misread characters with similar shapes — especially in CJK scripts.
An LLM can correct these errors while preserving the original bounding boxes.

**The key constraint for this mode: AI corrects text only. Bounding boxes must not move.**
harumi writes the corrected text at the exact same position as the raw OCR output.

> **OCR correction ≠ translation.** Translation changes word count, line count, and language,
> so it cannot use fixed word bboxes. Translation write-back uses region-level layout with
> `extract_layout_regions` → AI → `plan_text_for_regions` → `quality gate` → apply.
> See `harumi-ai` for the translation path.

```text
scanned.pdf
  └─ ocrs-cjk → ocrs_sample_raw.json       (raw: 請求害, 株式会杜, 品各)
  └─ LLM call → ocrs_sample_corrected.json  (corrected: 請求書, 株式会社, 品名)
                  ai_correction_report.json  (what changed and why)
  └─ harumi (same example, corrected JSON as input — bboxes unchanged)
       └─ searchable_corrected.pdf
```

```bash
# Run with the AI-corrected JSON — same example, different input
cargo run --example ocrs_cjk_to_searchable_pdf -- \
  examples/fixtures/scanned_sample.pdf \
  examples/fixtures/ocrs_sample_corrected.json \
  /path/to/NotoSansCJKjp-Regular.ttf \
  searchable_corrected.pdf
```

See `examples/fixtures/ocrs_sample_raw.json`, `ocrs_sample_corrected.json`, and
`ai_correction_report.json` for a worked example of the raw → corrected → report pattern.

> For legal, medical, financial, or audit documents: retain the raw OCR JSON and the
> correction report alongside the output PDF. Do not let AI invent text not visible in the scan.

harumi is not an OCR engine. For the translation path, use `harumi-ai` on top of any LLM.

---

## What you get

| Challenge | harumi's answer |
|---|---|
| CJK font subsetting is complex | One `embed_font()` call — only used glyphs are included, GIDs correctly remapped; GSUB/GPOS/variable-font tables stripped for macOS Preview and PSPDFKit compatibility |
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
| Need to extract text positions from an existing PDF | `extract_text_runs` decodes CID fonts and standard simple fonts (Type1, TrueType, Type3, WinAnsi, etc.) |
| Need to read or write PDF metadata (title, author…) | `doc.metadata()` reads `/Info`; `doc.set_metadata(&meta)` writes it |
| Need to replace text in an existing PDF (new font) | `page.replace_text(old, new, font)` rewrites the content stream in-place; returns the match count as `usize`; automatic font-switching and width compensation |
| Need to replace text using the original font | `page.replace_text_preserve_font(old, new)` — no `FontHandle` needed; returns match count; validates glyphs eagerly (not at `save()`) |
| Need to check replaceability without modifying | `page.can_replace_text(old, new)` — pure read-only scan; returns match count or `Err(FontCharNotMapped)` |
| Need to draw an ellipse or circle | `add_ellipse(rect, color, opacity, filled, stroke_width)` (`draw` feature) |
| Need fill + stroke on same shape | pass `filled=true` and `stroke_width>0` to `add_ellipse` / `add_polygon` / `add_path` — uses PDF `B` operator |
| Need open or closed path (polyline + polygon unified) | `add_path(points, closed, color, filled, stroke_width, opacity)` (`draw` feature) |
| Need rotated text (watermarks, stamps at an angle) | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| Need to replace text spanning multiple Tj operators or font runs | `replace_text` / `replace_text_preserve_font` — cross-operator **and** cross-Tf matching supported |
| Need to extract an embedded image from a scanned PDF | `extract_page_image` returns JPEG or PNG bytes (`image` feature); scanned PDFs only |
| Need clickable URL links in a PDF | `add_link_url([x, y, w, h], url)` — invisible URI annotation; click opens the URL in any viewer |
| Need internal navigation links (TOC) | `add_link_internal([x, y, w, h], target_page)` — jumps to a page within the same document |
| Need a bookmarks / navigation outline | `add_bookmark(title, page, y)` — flat PDF outline entries; CJK titles stored as UTF-16BE automatically |
| Need page numbers / running headers–footers on every page | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` with `HeaderFooter` (`flow` feature); `{{page}}` / `{{total}}` substituted at render |
| Need headings to auto-generate outline entries | `FlowOptions { auto_bookmarks: true, .. }` (default) — every `push_heading` creates a bookmark |
| Need to load a password-protected PDF | `Document::from_file_with_password(path, pw)` / `from_bytes_with_password(bytes, pw)` — decrypts on load; both user and owner passwords accepted |
| Need to save a PDF with password protection | `doc.set_encryption(user_pw, owner_pw)` — encrypts at `save()` time with 128-bit RC4 |
| Need to check if a PDF was originally encrypted | `doc.is_encrypted()` — `true` even after successful decryption |
| Need to highlight / underline / strike through text | `add_highlight` / `add_underline` / `add_strikeout` / `add_squiggly` with color — standard PDF markup annotations with QuadPoints |
| Need to add a sticky-note comment to a page | `add_sticky_note([x, y], "note text")` — Text annotation, Unicode contents |
| Need to read PDF form field values | `doc.form_fields()` — returns `Vec<FormField>` with name, type, and current value |
| Need to fill in a PDF form programmatically | `doc.fill_form(&[("FieldName", "value")])` — sets values and triggers NeedAppearances |
| Need to set/read page crop or print boxes | `page.crop_box()` / `set_crop_box(rect)` / `trim_box()` / `bleed_box()` — all box types in `[x,y,w,h]` format |
| Need to scale page content (e.g. A4 → A3) | `page.scale_page_content(sx, sy)` inserts a `cm` matrix before existing content; `resize_page_with_content(w, h)` scales + resizes MediaBox in one call (v1.4+) |
| Need to overlay one PDF on top of another | `doc.overlay_from(other)` stamps each page of `other` onto the matching page of `self` as a Form XObject; fonts, images, and opacity are preserved (v1.4+) |
| Need to remove all bookmarks / TOC | `doc.clear_outline()` removes both pending bookmarks and any existing `/Outlines` tree in a loaded PDF (v1.4+) |
| Need to attach files to a PDF | `doc.attach_file(name, data, mime)` embeds any file as a PDF attachment (EmbeddedFiles, FlateDecode-compressed, sorted name tree); `doc.list_attachments()` → `Vec<AttachmentInfo>` (v1.4+) |
| Need bold/italic/font-family from extracted text | `TextFragment::is_bold`, `is_italic`, `font_family`, `base_font` — parsed from the PostScript `/BaseFont` name (v1.4.1+) |
| Need to detect column layout from extracted text | `detect_text_columns(&frags, page_width)` — X-density histogram splits text into `Vec<ColumnZone>` by gap detection (v1.4.1+) |
| Need to group extracted fragments into lines or paragraphs | `group_text_fragments(&frags, GroupingStrategy::Paragraph)` — merges adjacent `TextFragment`s into `TextGroup`s; `Paragraph` groups by inter-line gap, `Line` groups same-baseline fragments — useful for feeding paragraph-level context to a translation model (v1.5+) |
| Need to check whether a font file covers a given character | `font_covers_char(font_bytes, ch) -> bool` — queries the font cmap via ttf-parser; use to select between primary and fallback fonts before embedding (v1.5+) |
| Need to extract text from a PDF table cell by cell | `extract_table_cells(&frags, page_width, page_height)` — returns `Vec<TableCell>` with 0-based `row`/`col`, `text`, and bounding box; columns from `detect_text_columns`, rows from Y-gap clustering; heuristic only (no grid-line support) (v1.5+) |
| Need to use CMYK colors (print workflow) | `Color::Cmyk([c, m, y, k])` — unified `Color` enum; `Color::Rgb()` still works via `From<[f32; 3]>` (v1.0+, breaking change) |
| Need to verify digital signatures on a PDF | `doc.verify_signatures(&pdf_bytes)` — extracts all signature data (signer, timestamp, field name); performs RSA PKCS#1 v1.5 cryptographic verification; returns `SignatureInfo` with `is_valid: bool` (`digital-signature` feature, v1.2.2+) |
| Need to create and sign a PDF digitally | `doc.add_signature_field(page, rect, options)` + `SigningContext::from_cert_and_key(cert, key)` + `doc.sign_document(context, field_name)` → signed PDF bytes — PKCS#7 DER structure, SHA-256 + RSA signing, ByteRange per spec, full v1.2.2+ support (`digital-signature` feature) |
| Need to trace which PDF operator produced a TextFragment | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — byte offset of the originating `Tj`/`TJ` keyword in the decompressed content stream (v1.5.15+) |
| Need to replace per-character text (one glyph per Tj) | `page.replace_text_fragments(&frags, new_text, font)` — suppresses each source `Tj`/`TJ` operator with `() Tj` and places `new_text` at the first fragment's position; works on PScript5/Distiller and Type3 layouts where `replace_text()` cannot match (v1.5.15+) |
| Need the Tm horizontal scale for non-uniform text matrices | `TextFragment.tm_x_scale: Option<f32>` — √(a²+b²) from the Tm matrix; symmetric to `tm_y_scale`; `None` when no Tm in the current BT block. Lets callers compute correct visual widths and column offsets for PDFs that encode text with `font_size=1` and a large Tm scale (v1.6.0+) |
| Need a stable column/row anchor for in-place form PDF translation | `TextFragment.tm_lm_x / tm_lm_y: Option<f32>` — text line matrix (T_lm) position at each `Tj`. Unlike `tm_origin_x` (Tm-only) or `x` (accumulates glyph advances), `tm_lm_x` resets cleanly on every `Td`, so label and value columns always report their correct page position regardless of how much text the previous row contained (v1.7.0+) |
| Need to extract table cells with source fragments for translation | `extract_table_cells(&frags, w, h)` now returns `TableCell` with `fragments: Vec<TextFragment>` and `bbox() -> [f32; 4]`. Pass `&cell.fragments` + `cell.bbox()` directly to `replace_fragments_fit_to_bbox` or `replace_text_fragments_batch_opts` — no manual fragment lookup needed (v1.8.0+) |
| Need per-cell font size / width in batch translation | `page.replace_text_fragments_batch_opts(entries, font)` — each `BatchEntry` carries its own `FragmentReplaceOpts`, enabling different `font_size`, `max_width`, `shrink_to_fit`, and `color` per cell in one stable-offset pass (v1.8.0+) |
| Need to place translated text fitted into the original cell bbox | `page.replace_fragments_fit_to_bbox(&cell.fragments, text, font, cell.bbox(), FitOptions::default())` — suppresses originals and places replacement sized to cell width with optional shrink-to-fit (v1.8.0+) |
| Need to measure text width before placing it | `doc.measure_text(text, font, font_size) -> Result<f32>` — advance width in PDF points using the registered font's TTF metrics (v1.9.0+) |
| Need to plan text layout in a fixed rectangle before drawing | `doc.fit_text_to_box(text, font, rect, font_size, opts) -> Result<FitResult>` — pure planning pass; returns wrapped lines, effective font size, `used_rect`, and `overflow_horizontal`/`overflow_vertical` flags; four policies via `OverflowPolicy`: `Shrink`, `WrapThenShrink`, `Truncate`, `Report` (v1.9.0+) |
| Need to detect overlaps between planned text boxes | `detect_collisions(boxes: &[PlacedBox]) -> Vec<Collision>` — O(n²) AABB overlap check; each `Collision` carries `index_a`, `index_b`, `overlap_rect`, and `overlap_area` (pre-computed area in pt² for severity ranking); combine with `fit_text_to_box` `used_rect` to pre-flight collisions before mutating content streams (v1.9.0+, `overlap_area` v1.13.0+) |
| Need the full column width (not just source glyph width) for replacement text | `extract_layout_regions(&frags, page_w, page_h, opts) -> Vec<LayoutRegion>` — returns each cell with both `source_bbox` (glyph bounds) and `usable_rect` (full available area); `usable_rect.width` extends to the next column start so translated text has the real column space, not just the original label width (v1.10.0+) |
| Need to batch-plan translated text into layout cells before drawing | `doc.plan_text_for_regions(regions, replacements, font, opts) -> Result<Vec<RegionFitPlan>>` — fits each replacement into `region.usable_rect` via `fit_text_to_box`, detects cross-region collisions, returns `RegionFitPlan { region, fit, collisions }` per pair; combines the layout, fit, and collision APIs into one pass (v1.10.0+) |
| Need stable baselines and column-safe widths for form PDF translation | `doc.plan_text_for_regions_with_policy(regions, replacements, font, options)` — per-region `RegionTextFitOptions` with `BaselinePolicy` (`PreserveSourceBaseline` keeps original row position) and `WidthPolicy` (`SourceLineWidth` prevents label expansion into value columns); `RegionTextFitOptions::for_role(&region.role)` returns safe defaults per role; pass `&[]` for fully automatic role-based policy (v1.11.0+) |
| Need to know whether a region is a label, value, heading, or paragraph | `LayoutRegion::role: LayoutRegionRole` — `LeftLabel` / `RightValue` / `ParagraphBody` / `SectionHeading` / `HeaderFooter` / `Unknown`; assigned automatically by `extract_layout_regions` from column position, row siblings, and page-edge proximity (v1.11.0+) |
| Need to classify collision structural relationships (same row, adjacent row, header/footer) | `classify_collisions(regions, collisions) -> Vec<ClassifiedCollision>` — annotates each raw `Collision` with `CollisionKind` (`SameRegion`, `SameRow`, `AdjacentRow`, `SameColumn`, `HeaderFooter`, `Unknown`), the `LayoutRegionRole` of each side, and `CollisionSeverity` (`Minor` / `Moderate` / `Major`); `plan_text_for_regions*` now returns `Vec<ClassifiedCollision>` directly in `RegionFitPlan.collisions` (v1.12.0+, severity v1.14.0+) |
| Need to assess collision severity using raw box sizes (without LayoutRegion) | `collision_severity(overlap_area, box_a_area, box_b_area) -> CollisionSeverity` — standalone severity computation for callers who build `PlacedBox`es directly; falls back to absolute pt² thresholds when a box area is 0 (v1.14.0+) |
| Need to extract label/value region pairs from a form or table PDF | `extract_label_value_pairs(regions) -> Vec<LabelValuePair>` — pairs each `LayoutRegionRole::LeftLabel` region with its same-row `RightValue` siblings; useful for detecting dense SDS/form layouts and building translation context (v1.14.0+) |
| Need to know the outcome of a text fitting operation (shrunk / overflowed / truncated) | `FitResult::status: PlacementStatus` — `Ok` (fit without adjustment), `Shrunk` (font reduced above floor), `ShrunkToMin` (font at `min_font_size` floor — text may still overflow), `Overflow` (overflow reported under `OverflowPolicy::Report`), `Truncated` (lines dropped under `OverflowPolicy::Truncate` or `Report` + `max_lines`); use as a decision signal without re-examining all overflow flags (v1.13.0+) |
| Need a page-level quality gate for translated layout | `PageFitSummary::from_plans(plans) -> PageFitSummary` — aggregate over a batch of `RegionFitPlan`s; fields: `overflow_count`, `collision_count`, `shrunk_count`, `worst_overlap_area`, `worst_overlap_rect`; use before writing the final PDF to decide whether to retry or fall back (v1.13.0+) |
| Need to visually debug layout collisions and text placement in a PDF | `page.add_fit_debug_overlay(&plans, DebugOverlayOptions::default())` — draws color-coded stroke rectangles: blue = source bbox, green = placed text, red = collision overlap; configurable colors and line width via `DebugOverlayOptions`; NaN/invalid coordinates are silently skipped (`draw` feature, v1.13.0+) |
| Need to overlay text while preserving character spacing for minor width differences | `add_text_styled_with_char_spacing(text, font, pos, size, color, bold, italic, char_spacing)` — PDF `Tc` operator; negative `char_spacing` compresses text width to fit original bounding box without changing font size (v1.15.0+) |
| Need bounding boxes of images on a page (for overlay avoidance) | `doc.page_image_bboxes(page)` — returns `[x, y, width, height]` for every axis-aligned Image XObject; no `image` feature required (v1.15.0+) |
| Need a unified layout quality report (overflow + collision + image overlap) | `doc.assess_page_layout_quality(page, &plans) -> PageLayoutQuality` — calls `page_image_bboxes` + `PageLayoutQuality::from_plans`; fields: `issues: Vec<LayoutIssue>`, `overflow_count`, `collision_count`, `image_overlap_count`, `worst_severity`; `LayoutIssue` carries `kind` (`TextOverflow`/`TextCollision`/`ImageOverlap`/`BboxDrift`/`AcceptedShrink`), `severity` (`Minor`/`Moderate`/`Major`), rects, and a human-readable message (v1.16.0+) |

---

## Comparison with similar tools

| Feature | **harumi** | pdf-lib (JS) | printpdf (Rust) | lopdf (Rust) | pdfium-render (Rust) |
|---|:---:|:---:|:---:|:---:|:---:|
| Pure Rust — no C/C++ deps | Yes | N/A | Yes | Yes | No (C++ PDFium) |
| WASM / cross-platform | Yes | Yes | Yes | Yes | Partial (complex setup) |
| CJK text on existing PDF | Yes | Yes | No (new PDFs only) | No (manual) | Yes |
| Text extraction | Yes (CID + simple) | Partial (basic) | No | Partial (basic) | Yes full |
| Text replacement (with re-subsetting) | Yes | No | No | No | No |
| Page manipulation | Yes | Yes | Partial (limited) | Yes (low-level) | Yes |
| Draw shapes | Yes | Yes | Yes | No (manual) | Yes |
| Flow document / auto-pagination | Yes | No | No | No | No |
| HTML → PDF | Yes | No | No | No | No |
| Inline bold / italic / color | Yes (synthetic) | No | No | No | Yes |
| Encryption (read) | Yes (RC4) | Yes | No | Partial | Yes |
| Encryption (write) | Yes (RC4-128) | Yes | No | No | Yes |
| Markup annotations | Yes | Partial (basic) | No | No | Yes |
| CMYK color support | Yes (v1.0+) | Yes | Yes | No | Yes |
| Digital signature creation | Yes (v1.2.2+) | No | No | No | No |
| Digital signature verification | Yes (v1.2.2+) | Partial (basic) | No | No | Yes |

> Yes = supported  Partial = partial / limited  No = not supported  N/A = language-level feature

---

## Comparison with modern Rust PDF alternatives

| Feature | **harumi** | unpdf | pdf_oxide | justpdf-core |
|---|:---:|:---:|:---:|:---:|
| **Direction** | Read + Write | Read only | Full lifecycle | Full lifecycle |
| **Primary use case** | CJK text overlay on existing PDFs | PDF → Markdown/text extraction | Multi-language PDF ops | Comprehensive PDF engine |
| Pure Rust (zero C/C++ deps) | Yes | Yes | Likely | Yes |
| WASM support | Yes (verified) | Yes | Yes | Not documented |
| **Text extraction** |
| — CID fonts (ToUnicode CMap) | Yes | Yes ⭐ | Yes | Yes |
| — Simple fonts (Type1/TrueType/Type3) | Yes | Yes | Yes | Yes |
| — Form XObject recursion | Yes (v1.5+) | Yes ⭐ | Yes | Unknown |
| — Graphic state preservation | Yes (v1.5+) | Yes ⭐ | Yes | Unknown |
| — `uni<XXXX>` glyph names | No (v1.3) | Yes ⭐ | Unknown | Unknown |
| — Reading order / XY-Cut | No | Yes ⭐ | Yes | Unknown |
| — RTL / BiDi support | No | Yes ⭐ | Unknown | Unknown |
| **Text writing** |
| — CJK font embedding | Yes ⭐ | N/A | Partial | Yes |
| — Font subsetting | Yes ⭐ (deferred) | N/A | Unknown | Yes |
| — Identity-H / Identity-V | Yes ⭐ | N/A | Unknown | Yes |
| — Type0 CID generation | Yes ⭐ | N/A | Unknown | Yes |
| **Page operations** | Yes | No | Yes | Yes |
| **Drawing (shapes, images)** | Yes | No | Yes (partial) | Yes |
| **Encryption (read)** | Yes (RC4) | Yes (RC4) | Yes | Yes (RC4, AES) |
| **Encryption (write)** | Yes (RC4-128, AES-256) | No | Yes | Yes (RC4, AES-256) |
| **Digital signatures** | Partial (metadata) | No | Yes | Yes (PKCS#7/CMS) |
| **PDF/A compliance** | Planned (v1.3) | No | Yes (validate) | Yes (validate) |
| **Performance focus** | Correctness | Speed (specialized) | Speed (5× PyMuPDF) | Comprehensive |
| **Multi-language bindings** | WASM only | None | 7 languages | C FFI only |

**Key differences:**
- **harumi** — Specialized for *writing* CJK text onto existing PDFs; explicit deferred subsetting strategy; confirmed WASM support
- **unpdf** — Specialized for *reading* PDFs and extracting clean Markdown/text; superior CJK extraction quality (XY-Cut, RTL, Form XObject)
- **pdf_oxide** — General-purpose PDF engine with multi-language bindings; 5× faster extraction via zero-copy tokenization; Rust core with Python/JS/Go/C#/Java bindings
- **justpdf-core** — Full PDF engine; uses region-specific CID orderings (Japan1/GB1/CNS1/Korea1) for legacy PDF compatibility

**Recommendation:** Use **harumi** if you're overlay writing CJK onto existing PDFs (OCR layers, stamps, watermarks). Use **unpdf** if you need to extract text from CJK PDFs and fix garbled characters. Use **pdf_oxide** if you need multi-language support and fast extraction. Use **justpdf-core** if you need a comprehensive PDF engine without specialized CJK focus.

⭐ = unique strength in this category

---

## Why this gap existed

JS has [`pdf-lib`](https://pdf-lib.js.org/) — it handles font subsetting, CMap generation, and text layer composition transparently. In Rust, the existing options force you to choose between:

- **`lopdf`** — low-level binary surgery; you hand-assemble CID font objects from the PDF spec
- **`printpdf`** — create-only; cannot modify existing PDFs
- **`pdfium-render`** — C++ bindings that break WASM, cross-compilation, and Lambda deploys

`harumi` fills the gap.

---

## MCP Server (harumi-mcp)

Use harumi directly from Claude Code, Cursor, or Continue via the **[harumi-mcp](harumi-mcp/)** Model Context Protocol server:

```bash
# Build the MCP server
cargo build -p harumi-mcp

# Use in Claude Code, Cursor, or Continue (configure in your IDE settings)
# MCP tools available: pdf_extract_text, pdf_extract_all_pages, pdf_replace_text,
# pdf_add_invisible_text, pdf_html_to_pdf, pdf_merge, pdf_page_info
```

For layout-preserving PDF translation, extract all pages with `pdf_extract_all_pages`,
translate the fragments, then apply replacements with `pdf_replace_text`. If a PDF
cannot be resubset because it uses a non-Identity `CIDToGIDMap`, use
`mode: "new_font"` with a Unicode TTF font.
The `harumi-ai` CLI defaults to `overlay` mode when you want to keep the original
page layout intact; pass `new` only when you want a regenerated document.
Overlay mode uses `detect_text_columns` for multi-column layout detection and
places translated text at the precise original baseline with font-accurate
descender coverage.

Register on [smithery.ai](https://smithery.ai) or [mcp.so](https://mcp.so) for one-click installation.

---

## Quick Start

```toml
[dependencies]
harumi = "1"
```

### Getting Fonts for CJK Support

For Japanese, Chinese, Korean, or multilingual PDF processing, download **NotoSansCJK** fonts from Google Fonts (free, OFL licensed):

```bash
# Japanese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKjp-Regular.ttf

# Simplified Chinese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKsc-Regular.ttf

# Traditional Chinese
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKtc-Regular.ttf

# Korean
wget https://github.com/notofonts/cjk/releases/download/Sans-v2.004/NotoSansCJKkr-Regular.ttf
```

**Alternative sources:**
- **Google Fonts**: https://fonts.google.com (search "Noto Sans CJK")
- **Adobe Fonts**: https://fonts.adobe.com (subscription-based)
- **System fonts**: Check with `fc-list | grep -i noto`

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
for frag in &runs {
    println!(
        "{:?} at ({:.1}, {:.1}) font={} color={:?} invisible={}",
        frag.text, frag.x, frag.y, frag.font_name, frag.color, frag.invisible,
    );
}
```

Each `TextFragment` carries: `text`, `x`/`y` (PDF-point coordinates), `width`, `font_size`, **`font_name`** (PDF resource name e.g. `"HR0"`), **`color`** (RGB fill `[f32; 3]`), **`invisible`** (`true` for OCR `Tr 3` text), **`is_bold`**, **`is_italic`**, **`font_family`**, and **`base_font`** (parsed from the PostScript `/BaseFont` name).

Works on arbitrary PDFs — Identity-H CID fonts (harumi output) and standard simple fonts (Type1, TrueType) with WinAnsiEncoding, MacRomanEncoding, StandardEncoding, or `/Differences` encoding dicts.

### Replace text in an existing PDF

```rust
let mut doc = Document::from_file("contract.pdf")?;
let font = doc.embed_font(include_bytes!("NotoSansJP-Regular.ttf"))?;
// Returns the number of matches found (0 means old_text was not present)
let n = doc.page(1)?.replace_text("Hello", "こんにちは", font)?;
doc.save("translated.pdf")?;
```

Matches text that spans consecutive `Tj`/`TJ` operators within the same BT/ET block (cross-operator matching), including across `Tf` font-change operators (cross-Tf matching). Japanese PDFs that encode a single visual line across multiple font runs — e.g. body Kanji in one font and bracket characters in another — are matched correctly. Only splits across vertical `Td` or `Tm` (new visual line) are not matched.

### Replace text using the original embedded font

When you don't have the font file but know the replacement text uses only glyphs already in the PDF.
Glyph validation is **eager**: `Err(FontCharNotMapped)` is returned immediately at call time if a glyph is missing, so you can fall back in one pass:

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.replace_text_preserve_font("Draft", replacement) {
    Ok(n) if n > 0 => { /* n replacements queued — no extra font needed */ }
    Ok(_) => { /* old_text not found */ }
    Err(_) => {
        // glyph missing from subset — fall back to explicit font
        let font = doc.embed_font(include_bytes!("font.ttf"))?;
        doc.page(1)?.replace_text("Draft", replacement, font)?;
    }
}
doc.save("output.pdf")?;
```

### Pre-flight check without modifying the document

Use `can_replace_text` to inspect replaceability before queuing any operations:

```rust
let mut doc = Document::from_file("contract.pdf")?;
match doc.page(1)?.can_replace_text("Draft", "Final") {
    Ok(0) => println!("'Draft' not found on page 1"),
    Ok(n) => println!("{n} occurrence(s) found; glyphs OK"),
    Err(e) => println!("glyph missing: {e}"),
}
```

### Replace text with font subset expansion

When the new text contains characters **not present in the original font subset**, use `replace_text_resubset`. Pass the original (unsubsetted) TTF/OTF bytes — harumi expands the subset, re-encodes all content streams, and performs the replacement in one `save()` call.

```rust
let font_bytes = include_bytes!("NotoSansJP-Regular.ttf");
let mut doc = Document::from_file("contract.pdf")?;

// replace_text_preserve_font would fail with FontCharNotMapped here
let n = doc.page(1)?.replace_text_resubset("Hello", "日本語", font_bytes)?;
doc.save("output.pdf")?;
```

Works for any language — Chinese, Korean, Arabic — as long as the supplied font contains the characters.

> **Note**: Requires the original unsubsetted font file, not the subset embedded in the PDF.
> Only CIDFontType2 fonts with `CIDToGIDMap /Identity` are supported (what harumi embeds).
> PDFs generated by other tools may use a non-Identity `CIDToGIDMap`; for those,
> use `replace_text` with a newly embedded font, or MCP `pdf_replace_text` with `mode: "new_font"`.

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
harumi = { version = "1", features = ["draw"] }
```

```rust
// Yellow filled highlight rectangle (x, y, width, height in PDF points)
doc.page(1)?.add_rect([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0], 0.4)?;

// Blue border rectangle (stroke only, no fill)
doc.page(1)?.add_rect_stroke([72.0, 400.0, 200.0, 100.0], [0.0, 0.0, 1.0], 1.5, 1.0)?;

// Filled triangle (callout arrow tip) — last arg is stroke_width (0.0 = no stroke)
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [1.0, 0.5, 0.0], 1.0, true, 0.0,
)?;

// Filled + stroked triangle simultaneously (fill-then-stroke, PDF `B` operator)
doc.page(1)?.add_polygon(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    [0.0, 0.6, 1.0], 1.0, true, 2.0,
)?;

// Black underline stroke
doc.page(1)?.add_line([72.0, 600.0], [300.0, 600.0], [0.0, 0.0, 0.0], 1.5, 1.0)?;

// Semi-transparent blue filled ellipse
doc.page(1)?.add_ellipse([200.0, 300.0, 150.0, 100.0], [0.0, 0.4, 1.0], 0.7, true, 0.0)?;

// Circle outline only (no fill, 2pt border)
doc.page(1)?.add_ellipse([100.0, 100.0, 80.0, 80.0], [1.0, 0.0, 0.0], 1.0, false, 2.0)?;

// Open polyline path (triangle without closing edge)
doc.page(1)?.add_path(
    &[[100.0, 500.0], [150.0, 600.0], [200.0, 500.0]],
    false,               // open path (no closepath)
    [0.2, 0.8, 0.2],    // green
    false, 1.5, 1.0,    // stroke only, 1.5pt line width, full opacity
)?;

// Rotated watermark text (45° counter-clockwise)
let font = doc.embed_font(include_bytes!("NotoSansCJK.ttf"))?;
let (w, h) = doc.page(1)?.size()?;
doc.page(1)?.add_text_with_rotation(
    "CONFIDENTIAL",
    font,
    [w / 2.0, h / 2.0],
    48.0,
    [0.8, 0.0, 0.0],   // red
    0.3,               // 30 % opacity
    45.0,              // degrees (counter-clockwise)
)?;
```

### Embed images (`image` feature)

```toml
harumi = { version = "1", features = ["image"] }
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

### Extract an embedded image from a scanned PDF (`image` feature)

Designed for OCR workflows: load a scanned PDF, extract the raster image, run OCR, then write the invisible text layer back.

```rust
use harumi::{Document, PageImageFormat};

let doc = Document::from_file("scanned.pdf")?;
let img = doc.extract_page_image(1)?;

match img.format {
    PageImageFormat::Jpeg => std::fs::write("page1.jpg", &img.bytes)?,
    PageImageFormat::Png  => std::fs::write("page1.png", &img.bytes)?,
}
println!("{}×{} pixels", img.width, img.height);
```

> **Scanned PDFs only.** This extracts an existing Image XObject — it does not rasterize the page. Text and vector PDFs have no Image XObject and will return `Error::InvalidInput`.

### Build a structured document with auto-pagination (`flow` feature)

```toml
harumi = { version = "1", features = ["flow"] }
```

```rust
use harumi::{FlowDocument, FlowOptions, Margins};

let font = include_bytes!("NotoSansCJK-Regular.ttf");
let mut doc = FlowDocument::new(font.as_ref(), FlowOptions::default())?;

doc.push_heading("Annual Report", 1)?;
doc.push_paragraph("This document summarizes our performance.")?;
doc.push_key_value_table(&[
    ("Revenue", "$1,000,000"),
    ("Expenses", "$800,000"),
    ("Profit", "$200,000"),
])?;
doc.push_list(&["Expanded to 3 new markets", "Launched 2 new products"], false)?;

// Page breaks are inserted automatically when content overflows.
// Call push_page_break() to force a manual break.

let pdf_bytes = doc.render()?;
```

Supports Japanese / Chinese / Korean out of the box — pass a CJK TTF font and text wraps at any character boundary.

### Inline text styling in FlowDocument (`flow` feature)

Bold, italic, and color can be mixed inline within a paragraph:

```rust
use harumi::{FlowDocument, FlowOptions, InlineSpan};

let mut doc = FlowDocument::new(font_bytes, FlowOptions::default())?;
doc.push_paragraph_styled(&[
    InlineSpan::plain("Normal text, "),
    InlineSpan::bold("bold text, "),
    InlineSpan::italic("italic text, "),
    InlineSpan::colored("and red.", [0.8, 0.0, 0.0]),
])?;
let pdf = doc.render()?;
```

Bold and italic are **synthetic** (fill+stroke and 12° shear respectively) — no separate bold/italic font file is required.

### Header / footer with page numbers (`flow` feature)

```rust
use harumi::{FlowDocument, FlowOptions, HeaderFooter};

let opts = FlowOptions {
    // Left "harumi docs", right "v0.5" on every page
    header: Some(HeaderFooter {
        left:  Some("harumi docs".into()),
        right: Some("v0.5".into()),
        ..Default::default()
    }),
    // Centred "1 / 3" page counter
    footer: Some(HeaderFooter::page_number()),
    // push_heading() automatically creates a bookmark entry (default: true)
    auto_bookmarks: true,
    ..Default::default()
};

let mut doc = FlowDocument::new(font, opts)?;
doc.push_heading("Chapter 1", 1)?;
doc.push_paragraph("Body text here.")?;
let pdf_bytes = doc.render()?;
```

### Link annotations

```rust
// Clickable URL region (x, y, width, height)
doc.page(1)?.add_link_url([72.0, 40.0, 200.0, 18.0], "https://example.com")?;

// Internal link: clicking the area jumps to page 3 of the same document
doc.page(1)?.add_link_internal([72.0, 700.0, 150.0, 18.0], 3)?;
```

### Markup annotations (highlight, underline, strikeout, squiggly)

```rust
// Yellow highlight
doc.page(1)?.add_highlight([72.0, 690.0, 200.0, 14.0], [1.0, 1.0, 0.0])?;

// Red underline
doc.page(1)?.add_underline([72.0, 640.0, 200.0, 12.0], [1.0, 0.0, 0.0])?;

// Strikethrough
doc.page(1)?.add_strikeout([72.0, 590.0, 200.0, 12.0], [0.0, 0.0, 0.0])?;

// Squiggly (wavy) underline
doc.page(1)?.add_squiggly([72.0, 540.0, 200.0, 12.0], [0.0, 0.6, 0.2])?;

// Sticky-note comment
doc.page(1)?.add_sticky_note([500.0, 700.0], "Review this section")?;
doc.save("annotated.pdf")?;
```

### Password-protected PDFs

```rust
// Load an encrypted PDF
let mut doc = Document::from_file_with_password("protected.pdf", "secret")?;
assert!(doc.is_encrypted());

// Wrong password returns Error::WrongPassword
match Document::from_bytes_with_password(&bytes, "wrong") {
    Err(harumi::Error::WrongPassword) => println!("Bad password"),
    _ => {}
}

// Save with password protection
let mut doc = Document::new((595.0, 842.0))?;
doc.set_encryption("userpass", "ownerpass")?;
doc.save("protected_output.pdf")?;
```

### AcroForm: read and fill form fields

```rust
// Read all form fields
let mut doc = Document::from_file("form.pdf")?;
for field in doc.form_fields()? {
    println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
}

// Fill fields by name
let updated = doc.fill_form(&[
    ("FullName",    "Jane Doe"),
    ("Agree",       "yes"),       // checkbox → /Yes
    ("Department",  "Engineering"),
])?;
println!("{updated} fields updated");
doc.save("filled_form.pdf")?;
```

### Page boxes (print workflow)

```rust
// Read/write CropBox (visible area clip)
let cb = doc.page(1)?.crop_box()?;   // Option<[f32;4]>

doc.page(1)?.set_crop_box([10.0, 10.0, 575.0, 822.0])?;   // [x,y,w,h]
doc.page(1)?.set_trim_box([0.0, 0.0, 595.0, 842.0])?;
doc.page(1)?.set_bleed_box([0.0, 0.0, 601.0, 848.0])?;
doc.save("print_ready.pdf")?;
```

### Document bookmarks (outline)

```rust
// Builds the bookmarks panel in PDF viewers.
// Non-ASCII titles (CJK, accented Latin…) are encoded as UTF-16BE automatically.
doc.add_bookmark("Chapter 1",   1, 800.0)?;   // title, page (1-indexed), y coord
doc.add_bookmark("第2章 概要",  2, 800.0)?;
doc.save("report.pdf")?;
```

### Convert HTML to PDF (`html` feature)

```toml
harumi = { version = "1", features = ["html"] }
```

```rust
use harumi::{render_html_to_pdf, HtmlRenderOptions};

let font = include_bytes!("NotoSansCJK-Regular.ttf").to_vec();
let html = r#"
    <h1>Annual Report</h1>
    <p>Introduction paragraph.</p>
    <table>
      <tr><th>Revenue</th><td>$1,000,000</td></tr>
      <tr><th>Profit</th><td>$200,000</td></tr>
    </table>
    <h2>Highlights</h2>
    <ul><li>Expanded to 3 new markets</li><li>Launched 2 new products</li></ul>
    <div style="page-break-after: always"></div>
    <h1>Page Two</h1>
"#;

let pdf_bytes = render_html_to_pdf(html, HtmlRenderOptions {
    font_bytes: font,
    ..HtmlRenderOptions::default()
})?;
```

Supported elements: `<h1>`–`<h6>`, `<p>`, `<table>/<tr>/<th>/<td>`, `<ul>/<ol>/<li>`, `<div>/<section>/<article>` (block containers).  
Page breaks: `style="page-break-after: always"` or `class="page-break"`.  
Skipped: `<script>`, `<style>`, `<head>`.  
Inline styles: `<strong>`/`<b>` (bold), `<em>`/`<i>` (italic), `<span style="color: #RRGGBB">` (color), `<a href>` (blue link color).  
Handles deeply nested HTML without stack overflow (iterative parser, tested with 5 000 nested `<div>`s).

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

// Replace text in existing content stream (single-operator match); returns match count
let n: usize = doc.page(1)?.replace_text(old_text, new_text, font)?;
// Replace using the original embedded font; eager glyph validation; returns match count
let n: usize = doc.page(1)?.replace_text_preserve_font(old_text, new_text)?;
// Read-only scan: returns match count or Err(FontCharNotMapped)
let n: usize = doc.page(1)?.can_replace_text(old_text, new_text)?;
// Replace text + expand font subset to include new characters
let n: usize = doc.page(1)?.replace_text_resubset(old, new, font_bytes)?;
// Suppress source Tj/TJ operators for per-character PDFs and place new text
let n: usize = doc.page(1)?.replace_text_fragments(&frags, new_text, font)?;

// Styled visible text (bold/italic synthetic effects, no extra font file needed)
doc.page(1)?.add_text_styled(text, font, [x, y], size, [r, g, b], bold, italic)?;

// Link annotations (no feature gate)
doc.page(1)?.add_link_url([x, y, w, h], "https://example.com")?;   // URL link
doc.page(1)?.add_link_internal([x, y, w, h], target_page)?;         // in-document link

// Document outline / bookmarks (no feature gate)
doc.add_bookmark("Section Title", page, y)?;  // appends a flat outline entry

// Markup annotations (no feature gate)
doc.page(1)?.add_highlight([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_underline([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_strikeout([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_squiggly([x, y, w, h], [r, g, b])?;
doc.page(1)?.add_sticky_note([x, y], "comment text")?;

// AcroForm (no feature gate)
let fields: Vec<FormField> = doc.form_fields()?;
let n: usize = doc.fill_form(&[("field_name", "value")])?;

// Page boxes (no feature gate)
let cb: Option<[f32; 4]> = doc.page(1)?.crop_box()?;
doc.page(1)?.set_crop_box([x, y, w, h])?;
doc.page(1)?.set_trim_box([x, y, w, h])?;
doc.page(1)?.set_bleed_box([x, y, w, h])?;
let mb: [f32; 4] = doc.page(1)?.media_box()?;
doc.page(1)?.set_media_box([x, y, w, h])?;

// Password protection (no feature gate)
Document::from_file_with_password(path, password)?;
Document::from_bytes_with_password(bytes, password)?;
doc.is_encrypted()                     // true if PDF was encrypted when loaded
doc.set_encryption(user_pw, owner_pw)?; // encrypt on next save()
```

### Coordinate system

Coordinates are in **PDF points** (1 pt = 1/72 inch), origin at the **bottom-left** of the page. If your OCR engine (e.g. Tesseract / hOCR) gives pixel coordinates from the top-left, use the `ocr` feature helper:

```toml
harumi = { version = "1", features = ["ocr"] }
```

### Feature flags

| Flag | What it enables | Extra dependencies |
|---|---|---|
| *(default)* | Text overlay, font embedding, `add_text_box`, `add_text_box_aligned`, `add_text_with_opacity`, `add_text_box_with_opacity` | lopdf, ttf-parser |
| `draw` | `add_rect`, `add_line`, `add_rect_stroke`, `add_polygon`, `add_polyline`, `add_ellipse` — shapes | none |
| `image` | `add_image`, `add_image_with_opacity` — JPEG/PNG raster images; `extract_page_image` — extract embedded image from scanned PDF (enables `draw`) | `png` crate (pure Rust) |
| `ocr` | `ocr::hocr_y_to_pdf`, `ocr::hocr_x_to_pdf`, `ocr::pixel_size_to_pt` — Tesseract coordinate conversion | none |
| `flow` | `FlowDocument` push-style builder with automatic pagination (`push_heading`, `push_paragraph`, `push_paragraph_styled`, `push_key_value_table`, `push_list`, `push_page_break`, `render`); `InlineSpan` for inline bold/italic/color within a paragraph; `HeaderFooter` for per-page header/footer with `{{page}}`/`{{total}}` substitution; `auto_bookmarks` for automatic outline from headings | none |
| `html` | `render_html_to_pdf` — HTML → PDF (h1–h6, p, table, ul/ol, page-break; enables `flow`); internal pure-Rust HTML tokenizer | none |

```rust
let pdf_y = harumi::ocr::hocr_y_to_pdf(pixel_y, page_height_pts, image_dpi);
let pdf_x = harumi::ocr::hocr_x_to_pdf(pixel_x, image_dpi);
let pt    = harumi::ocr::pixel_size_to_pt(pixel_size, image_dpi);
```

---

## Supported Fonts

| Font format | Status |
|---|---|
| TrueType (`.ttf`, `sfntVersion = 0x00010000`) | ✅ Fully supported — pure-Rust subsetting |
| TrueType Collections (`.ttc`, multiple font faces) | ✅ Fully supported — face index selection via `embed_font_at(bytes, face_index)` |
| OpenType with CFF outlines (`.otf`, `OTTO`) | ⚠️ Accepted (no subsetting) — embedded as-is |

For Japanese/Chinese/Korean, use the **TrueType** variant of [Noto Sans CJK](https://github.com/notofonts/noto-cjk) — end-to-end verified:

```
NotoSansCJKjp-Regular.ttf  (Japanese)
NotoSansCJKsc-Regular.ttf  (Simplified Chinese)
NotoSansCJKtc-Regular.ttf  (Traditional Chinese)
NotoSansCJKkr-Regular.ttf  (Korean)
```

> **OTF note**: harumi accepts `.otf` files and routes them through `FontFile3 /OpenType` embedding, but **does not subset CFF fonts** — all glyphs in the font are embedded. Use the TTF variants above to minimize PDF size via subsetting.

---

## Internals

```
harumi
├── lopdf v0.40          — parse and modify existing PDF object graph
├── ttf-parser           — font metadata (bbox, units_per_em, ascender)
└── [internal TTF subsetter] — pure-Rust TrueType subsetting (no external crates)
```

The font pipeline:

1. Parse used characters → collect Unicode code points
2. Map code points → original Glyph IDs via the font's `cmap` table (ttf-parser)
3. Subset the TTF to used glyphs only (internal pure-Rust subsetter); GIDs are **compacted to 0..N**
4. Remap `gid_to_char` and advance widths from original GIDs to the new compact GIDs
5. Build the CID font object graph: `Type0 → CIDFontType2 → FontDescriptor → FontFile2`
6. Generate a `/ToUnicode` CMap stream so viewers can copy/search the text
7. Append a new content stream to the page's `/Contents` array

Subsetting is **deferred**: `embed_font()` stores the raw TTF bytes; at `save()` time, harumi collects all characters used across every page, subsets once per font, and writes everything in one pass.

### Dependency minimization

harumi aims for **zero external runtime dependencies** beyond core PDF handling.

- **TrueType subsetting** — custom pure-Rust implementation (v1.1+); supports TTF + TTC (collections) with recursive composite-glyph resolution
- **Font parsing** — ttf-parser (single-purpose, no transitive deps)
- **Image decoding** — `png` crate (optional, feature-gated)
- **Crypto** — getrandom (OS entropy only; required for AES-256 encryption keys)

**Direct dependency count:** 3 (getrandom, lopdf, ttf-parser, plus optional `png`)  
**Transitive deps (default build):** ~8 (lopdf's internal utilities only)

---

## Why "harumi"

晴海 — *haru* (clear sky) + *umi* (sea). Calm on the surface, a lot going on underneath.

---

## Roadmap

| Version | Scope |
|---|---|
| **v0.1** | TrueType fonts, invisible + visible text, batch placement, `page.size()`, `save_to_bytes()`, GID remapping, OTF accepted |
| **v0.2** | `draw` feature (`add_rect`, `add_line`), `image` feature (`add_image`, PNG SMask transparency), page manipulation (`rotate_page`, `remove_page`, `insert_blank_page`, `reorder_pages`) |
| **v0.3** | `add_text_box`, `add_rect_stroke`, `add_polygon`, `add_ellipse`, `add_path`; `add_text_with_rotation`; security hardening; `merge_from`; `Document::new`; `extract_pages` |
| **v0.4** | `extract_text_runs` (CID + standard fonts), PDF metadata r/w, `replace_text` (Tj/TJ rewrite, cross-operator matching, width compensation, preserve-font mode), `flow` feature (`FlowDocument`, CJK auto-pagination), `html` feature, `extract_page_image` |
| **v0.5** | `add_link_url`, `add_link_internal` — clickable PDF link annotations; `add_bookmark` — document outline/bookmarks with CJK UTF-16BE titles; `HeaderFooter` + `{{page}}`/`{{total}}` for `FlowDocument`; `auto_bookmarks` from headings; security fixes |
| **v0.6** | `from_file_with_password` / `from_bytes_with_password` / `is_encrypted` / `Error::WrongPassword`; markup annotations (highlight, underline, strikeout, sticky-note); AcroForm `form_fields()` / `fill_form()`; AGL table +116 entries (Central EU, ligatures, euro); Identity-H text extraction fallback |
| **v0.7** *(current)* | `set_encryption` — write password-protected PDFs; `add_squiggly` — wavy underline annotation; full page-box API (`crop_box`, `trim_box`, `bleed_box`, `media_box` read/write) |
| **v0.8** | `replace_text_resubset` — expand font subset at replacement time (any language); MCP `pdf_replace_text` layout-preserving translation workflow and non-Identity `CIDToGIDMap` diagnostics; `InlineSpan` bold/italic/color in `FlowDocument` + HTML `<strong>`/`<em>`/`<span>` inline styles; nested `/Pages` tree inherited-attribute fix; TTC E2E tests; `wasm-pack test --node` CI; `cargo semver-checks` CI |
| **v1.4.1** | `TextFragment` font attributes (`is_bold`, `is_italic`, `font_family`, `base_font`); `detect_text_columns` + `ColumnZone` for column-layout inference |
| **v1.4.2** | `harumi-ai` overlay mode accuracy: per-line white-rect sizing (height, width, descender coverage), exact baseline Y placement, multi-column layout via `detect_text_columns`, bold-based heading detection, NaN-safe reading-order sort |
| **v1.5.0** | `group_text_fragments` — merge `TextFragment`s into line/paragraph `TextGroup`s; `font_covers_char` — cmap coverage check; Form XObject recursive text extraction (`Do` operator); cross-content-stream graphics state preservation; `extract_table_cells` — heuristic table row/col detection; `harumi-ai`: `OverflowStrategy` (Shrink/Truncate), `font_fallbacks` multi-font rendering per character, `on_progress` translation callback |
| **v1.5.1** | `harumi-ai` v0.1.0 first crates.io release; fix CJK byte-slice panic in InPlace debug log; Clippy lint cleanups (`repeat_n`, `div_ceil`) |
| **v1.5.2** | Fix font inheritance from ancestor Pages nodes (`collect_fonts_inner` parent-chain walk); JSON repair for LLM unescaped quotes; raise default `max_tokens` 4096→16000 |
| **v1.5.3** | Fix Form XObject discovery via inherited `/Resources` (Chrome/Skia PDFs — parent-chain walk in `extract_text_from_xobjects`); `replace_text()` now rewrites Form XObject content streams so `harumi-ai` InPlace mode works on Chrome/Skia PDFs |
| **v1.5.4** | Fix Type3 font support in text extraction (`collect_font_dict_entries` now includes `/Subtype /Type3`); fixes zero-output on Chrome/Skia PDFs that use only Type3 fonts |
| **v1.5.5** | Fix Overlay CTM coordinate transform — `parse_content_stream` now tracks `q`/`Q`/`cm` and transforms `TextFragment` coordinates to page space; Chrome/Skia PDFs (scale + Y-flip CTM) now have correctly positioned overlay text |
| **v1.5.6** | Three Chrome/Skia PDF bug fixes: (1) `overlay_from` wraps existing page contents in `q`/`Q` to isolate unbalanced `cm` operators; (2) `extract_text_from_xobjects` tracks per-`Do` CTM via `ParseCarryState.do_ctm_map`, applying each XObject's specific CTM instead of the last accumulated one; (3) `diagnose_match_failure` adds cross-BT scan returning `"type3-char-per-tj"` for Type3 font text spanning BT/ET boundaries |
| **v1.5.8** | Three translation visual bug fixes: (1) `finalize()` now calls `wrap_page_contents_in_q_q()` before `append_to_contents()` so unbalanced `cm` in existing content doesn't misplace appended text/draw streams; (2) `ctm_stack` moved into `ParseCarryState` so it persists correctly across multiple `Contents`-array streams (per-XObject CTM for Form XObjects); (3) cross-BT `Tj` replacement for Chrome/Skia Type3 PDFs — `find_cross_bt_matches()` + `CrossBtMatch` handle text split across BT/ET blocks, enabling `replace_text()` to work on these PDFs |
| **v1.5.9** | `ReplaceOptions { normalize_whitespace }` + `replace_text_opts()` — strip whitespace from `old_text` before matching so harumi-ai's space-joined fragments (e.g. `"T h e F r e e"`) match Chrome/Skia Type3 PDFs without changes to harumi-ai's grouping logic; `TextFragment.space_advance` — advance width of U+0020 at the fragment's font size, enabling callers to detect word gaps vs tight character spacing and avoid unconditional space insertion |
| **v1.5.10** | Fix cross-BT match count always returning zero: added cross-BT counting pass to `count_matches_in_raw_streams()` so `replace_text_opts(normalize_whitespace: true)` actually queues the replacement for Chrome/Skia Type3 PDFs; fix `find_cross_bt_matches()` font tracking — removed `cur_font.clear()` on `BT` and `in_bt` guard on `Tf` so font persists correctly across `BT`/`ET` pairs (per PDF spec) |
| **v1.5.11** | Improve InPlace match rate for traditional Japanese PDFs (GHS SDS etc.): horizontal `Tm` operators between characters in the same visual line no longer break cross-op/cross-Tf matching — `collect_char_segments` and `collect_cross_tf_segments` now flush only on vertical Tm (y-delta ≥ 1 pt); `Tm` and text-state ops (`Tc`/`Tw`/`Tz`/`TL`/`Ts`) added to the intermediate-ops whitelist and suppressed in `rewrite_content_stream` |
| **v1.5.12** | Fix silent stream skip for AES-256 encrypted PDFs: `page_content_streams()` and `decode_form_xobject()` now fall back to `stream.content` when `decompress()` fails (lopdf may have already decoded the stream during `load_with_password`), fixing 13→40+ fragments/page; `ExtractionWarning`/`WarningKind` + `extract_text_runs_verbose()` diagnostic API; `TextFragment.tf_font_size` + `TextFragment.tm_y_scale` new fields; zero advance-width fallback (`0.5em` per char) |
| **v1.5.13** | Fix XObject font resolution for PScript5/Distiller PDFs: `xobject_fonts()` now uses page-level fonts as a base so Form XObjects with a `/Resources` dict but no `/Font` sub-entry can still resolve fonts declared on the parent page |
| **v1.5.14** | Fix cross-stream BT/ET state: `in_bt`, current font, and text position are now stored in `ParseCarryState` and survive stream boundaries; fixes Distiller PDFs that split a single BT…ET block across multiple `/Contents` array streams (e.g. 48 bare `Tj` ops discarded after an unclosed BT in a preceding stream) |
| **v1.5.15** | `TextFragment.source_stream` / `source_op_start` / `source_op_end` — operator-level source tracking linking each fragment to its originating `Tj`/`TJ` byte offset; `PageHandle::replace_text_fragments(fragments, new_text, font)` — suppresses source operators with `() Tj` and places translation at the first fragment's position; enables clean InPlace translation of per-character PDFs (PScript5/Distiller, Type3 layouts) |
| **v1.5.16** | `TextFragment.source_xobject: Option<(u32, u16)>` — Form XObject source reference; `replace_text_fragments_opts` with `FragmentReplaceOpts` (`font_size`, `max_width`, `y_offset`, `color`); XObject stream rewriting support in fragment replacement |
| **v1.5.17** | `text_fragment_bounds(&[TextFragment]) -> Option<[f32; 4]>` — aggregate bounding box with ascender/descender estimation; `FragmentReplaceOpts.shrink_to_fit` + `min_font_size` for auto-shrinking replacement text to `max_width` |
| **v1.5.18** | `replace_text_fragments_batch` — single-pass multi-line batch replacement (no byte-offset drift between calls); `FragmentReplaceOpts.dry_run` pre-flight check; `FragmentReplaceFailureReason` + `can_suppress_fragment` diagnostic API |
| **v1.5.19** | Fix `tm_origin_x/y` reporting spurious `Some(0.0)` in Td-only BT blocks (no `Tm`); added `ParseCarryState::tm_origin_set` flag |
| **v1.6.0** | `TextFragment::tm_x_scale: Option<f32>` — X-scale from Tm matrix; fix glyph advance width and TJ kerning to use Tm x-scale for non-uniform Tm matrices |
| **v1.7.0** | `TextFragment.tm_lm_x / tm_lm_y: Option<f32>` — text line matrix (T_lm) position; resets cleanly on `Td` for stable column/row anchors in form PDF translation |
| **v1.8.0** | `TableCell.fragments: Vec<TextFragment>` + `bbox()`; `replace_text_fragments_batch_opts` with per-cell `BatchEntry` / `FragmentReplaceOpts`; `replace_fragments_fit_to_bbox` — cell-width-fitted replacement |
| **v1.9.0** | `measure_text` — advance width query; `fit_text_to_box` + `FitResult` + `BoxFitOptions` / `OverflowPolicy` — pure-planning layout pass; `detect_collisions` + `PlacedBox` + `Collision` — AABB overlap detection for pre-flight collision checks |
| **v1.10.0** | `extract_layout_regions` + `LayoutRegion` / `LayoutRegionKind` — returns `source_bbox` (glyph bounds) and `usable_rect` (full column width); `plan_text_for_regions` → `Vec<RegionFitPlan>` — one-pass layout fit + collision detection |
| **v1.11.0** | `LayoutRegionRole` (`LeftLabel`, `RightValue`, `ParagraphBody`, `SectionHeading`, `HeaderFooter`); `BaselinePolicy` + `WidthPolicy` + `RegionTextFitOptions`; `plan_text_for_regions_with_policy` with per-region fitting policies and `for_role()` defaults; NaN guard in HeaderFooter proximity check |
| **v1.12.0** | `CollisionKind` + `ClassifiedCollision` + `classify_collisions` — structural collision classification (SameRegion, SameRow, AdjacentRow, SameColumn, HeaderFooter, Unknown); `RegionFitPlan.collisions` changed to `Vec<ClassifiedCollision>` |
| **v1.13.0** | `Collision::overlap_area` (pt² severity field); `PlacementStatus` enum + `FitResult::status` (Ok/Shrunk/ShrunkToMin/Overflow/Truncated); `PageFitSummary::from_plans` — page-level quality gate (overflow_count, collision_count, shrunk_count, worst_overlap); `add_fit_debug_overlay` + `DebugOverlayOptions` (`draw` feature) — visual collision/placement debug overlay; bug fixes: Report+max_lines Truncated status, WrapThenShrink at min-font overflow, Truncate rh=0 false Ok, NaN coordinate guard in overlay |
| **v1.14.0** | `CollisionSeverity` (Minor/Moderate/Major) + `ClassifiedCollision::severity` field computed from `source_bbox` area ratio; `collision_severity()` standalone function for raw PlacedBox callers; `LabelValuePair` + `extract_label_value_pairs()` — pairs LeftLabel/RightValue regions for dense form/SDS PDF detection |
| **v1.15.0** | `add_text_styled_with_char_spacing` — PDF `Tc` character-spacing operator; negative `char_spacing` compresses text to fit without font-size reduction; `page_image_bboxes` — returns image bounding boxes for overlay-avoidance without `image` feature; `harumi-ai` v0.5.0: Tc optimization + image-region avoidance in overlay |
| **v1.16.0** *(current)* | `LayoutIssue` / `LayoutIssueKind` / `LayoutIssueSeverity` / `PageLayoutQuality` — rich per-issue layout quality reporting (overflow, collision, image-overlap, bbox-drift); `Document::assess_page_layout_quality` — single-call quality gate combining image bbox detection + `PageFitSummary`; `harumi-ai` v0.6.0: `LayoutRepairMode` (Off / GeometryOnly / GeometryThenVision / VisionAllPages), `VisionProvider` trait + `AnthropicTranslator implements VisionProvider`, Poppler `RasterizeOptions`, improved correction prompt |

---

## Contributing

Issues and PRs welcome at [github.com/kent-tokyo/harumi](https://github.com/kent-tokyo/harumi).

The most complex part of this codebase is `src/font/embed.rs` — the CID font object graph construction. When reporting rendering bugs in a specific PDF viewer, include the viewer name and version in your issue.

---

## License

MIT OR Apache-2.0
