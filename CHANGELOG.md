# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [1.11.0] — 2026-06-20

### Added (harumi)

- **`LayoutRegionRole`** (`src/extract.rs`) —
  Functional role assigned by `extract_layout_regions` based on column position, row siblings,
  and proximity to the page edge: `LeftLabel` (col 0 with a higher-col sibling), `RightValue`
  (col ≥ 1 with a col-0 sibling), `ParagraphBody`, `SectionHeading`, `HeaderFooter`, `Unknown`.

- **`LayoutRegion::role`** (`src/extract.rs`) —
  New field added to `LayoutRegion` (`#[non_exhaustive]` — semver-safe additive change).

- **`BaselinePolicy`** (`src/extract.rs`) —
  Vertical placement strategy for `RegionTextFitOptions`:
  `PreserveSourceBaseline` (fits into source glyph rect — safest for dense forms),
  `TopAlignToRegion` (v1.10 behaviour), `CenterInRegion`.

- **`WidthPolicy`** (`src/extract.rs`) —
  Horizontal width strategy: `SourceLineWidth`, `RegionUsableWidth`, `ClampToColumn`,
  `ClampBeforeNextRegion` (extends to just before the nearest same-row sibling).

- **`RegionTextFitOptions`** (`src/extract.rs`) —
  Per-region fitting policy: `baseline`, `width`, `min_font_size`, `max_lines`,
  `preserve_source_x`.  `RegionTextFitOptions::for_role(&role)` returns sensible defaults:
  `LeftLabel` → preserve source baseline + source width;
  `RightValue` / `SectionHeading` → preserve baseline + clamp to column;
  `ParagraphBody` → top-align + full usable width.

- **`Document::plan_text_for_regions_with_policy(regions, replacements, font, options)`**
  (`src/document/mod.rs`) —
  Like `plan_text_for_regions` but with per-region `RegionTextFitOptions`.
  If `options.len() < regions.len()`, the remainder use `RegionTextFitOptions::for_role`.
  Pass `&[]` to use role-based defaults for all regions automatically.

### Fixed (harumi)

- **NaN guard in `HeaderFooter` role detection** (`src/extract.rs`) —
  `top_y` and `bot_y` are now checked with `.is_finite()` before the page-edge comparison,
  preventing non-finite `source_bbox` values (from degenerate cells) from producing incorrect
  `HeaderFooter` classification via `NaN > number` / `Infinity > number` comparisons.

---

## [1.10.0] — 2026-06-20

### Added (harumi)

- **`extract_layout_regions(fragments, page_width, page_height, options) -> Vec<LayoutRegion>`**
  (`src/extract.rs`) —
  Detects layout regions on a page and returns each one with both `source_bbox` (glyph bounds)
  and `usable_rect` (inferred available area).
  The key difference from `extract_table_cells`: `usable_rect.width` extends to the **start of
  the next column** (or the page edge), not just to the end of the source glyphs.
  A short label like "名前:" (~30 pt) in a 150 pt column gets `usable_rect.width ≈ 148 pt`
  so `fit_text_to_box` can plan the full replacement without width constraints.
  `usable_rect.height` spans from the current row's ascender down to the next row's ascender
  (or a 1.5× source-height estimate for the last row).
  Internally reuses `extract_table_cells` + `detect_text_columns`.

- **`LayoutRegion`** (`src/extract.rs`) —
  `#[non_exhaustive]` struct returned by `extract_layout_regions`:
  `kind: LayoutRegionKind`, `row: Option<usize>`, `col: Option<usize>`, `text: String`,
  `source_bbox: [f32; 4]`, `usable_rect: [f32; 4]`, `fragments: Vec<TextFragment>`.

- **`LayoutRegionKind`** (`src/extract.rs`) —
  `#[non_exhaustive]` enum: `Heading(u8)` (levels 1–4 via font-size ratio, same thresholds as
  `TextChunk`), `Paragraph` (single-column non-tabular), `TableCell`, `Unknown`.

- **`LayoutRegionOptions`** (`src/extract.rs`) —
  `#[non_exhaustive]` options struct: `infer_row_heights: bool` (default `true`),
  `infer_column_widths: bool` (default `true`), `margin: f32` (default `2.0` pt).
  Construct with `LayoutRegionOptions::default()` and override fields.

- **`Document::plan_text_for_regions(regions, replacements, font, opts) -> Result<Vec<RegionFitPlan>>`**
  (`src/document/mod.rs`) —
  Fits each replacement string into `region.usable_rect` via `fit_text_to_box`, then runs
  `detect_collisions` on all `used_rect`s and attaches the relevant collisions to each plan.
  Initial font size is derived from the mean of the region's source fragment `font_size` values.

- **`RegionFitPlan`** (`src/extract.rs`) —
  `#[non_exhaustive]` result of `plan_text_for_regions`:
  `region: LayoutRegion`, `fit: FitResult`, `collisions: Vec<Collision>`.

---

## [1.9.0] — 2026-06-20

### Added (harumi)

- **`Document::measure_text(text, font, font_size) -> Result<f32>`** (`src/document/mod.rs`) —
  Returns the total advance width of `text` in PDF points using the font registered as `font`.
  Parses the original TTF bytes at call time via ttf-parser; characters not covered by the font
  contribute zero width.  Useful for sizing boxes before drawing.

- **`Document::fit_text_to_box(text, font, rect, font_size, opts) -> Result<FitResult>`** (`src/document/mod.rs`) —
  Plans how `text` lays out within `rect = [x, y, width, height]` without mutating the document.
  Returns `FitResult` with the final wrapped lines, effective font size, actual occupied
  `used_rect`, and per-axis overflow flags (`overflow_horizontal`, `overflow_vertical`).
  The geometry — `line_height = font_size × 1.2`, `wrap_paragraph` algorithm — is identical to the
  draw path, so `used_rect` values can be fed directly to `detect_collisions` before drawing.

- **`BoxFitOptions`** (`src/document/types.rs`) —
  `#[non_exhaustive]` options struct for `fit_text_to_box`.  Fields:
  `min_font_size: f32` (default `6.0`), `max_lines: Option<usize>` (default `None`),
  `wrap: bool` (default `true`), `overflow: OverflowPolicy` (default `WrapThenShrink`).

- **`OverflowPolicy`** (`src/document/types.rs`) —
  `#[non_exhaustive]` enum controlling how `fit_text_to_box` handles text that does not fit:
  - `Shrink` — shrink font (no wrap) until single line fits in width
  - `WrapThenShrink` — wrap first; if height still overflows, shrink font and re-wrap
  - `Truncate` — wrap and drop lines that exceed `max_lines` or the rect height
  - `Report` — return as-is; only set overflow flags

- **`FitResult`** (`src/document/types.rs`) —
  `#[non_exhaustive]` result of `fit_text_to_box`:
  `lines: Vec<String>`, `font_size: f32`, `used_rect: [f32; 4]`,
  `overflow_horizontal: bool`, `overflow_vertical: bool`.
  Convenience method `overflow() -> bool` returns the OR of both flags.

- **`PlacedBox::new(rect: [f32; 4]) -> PlacedBox`** (`src/extract.rs`) —
  Constructor for the `#[non_exhaustive]` `PlacedBox` struct used as input to `detect_collisions`.

- **`detect_collisions(boxes: &[PlacedBox]) -> Vec<Collision>`** (`src/extract.rs`) —
  O(n²) pairwise axis-aligned bounding-box overlap detection.
  Returns one `Collision { index_a, index_b, overlap_rect }` for every pair of boxes
  whose intersection has positive area.  Adjacent boxes sharing only an edge are not reported.
  NaN/Infinity coordinates are treated as non-overlapping.

---

## [1.8.0] — 2026-06-20

### Added (harumi)

- **`TableCell::fragments: Vec<TextFragment>`** (`src/extract.rs`) —
  Each cell produced by `extract_table_cells` now carries the source fragments
  it was built from.  Pass `&cell.fragments` directly to
  `replace_text_fragments_batch_opts` or `replace_fragments_fit_to_bbox` to
  suppress originals and place translations without any manual fragment lookup.

- **`TableCell::bbox() -> [f32; 4]`** (`src/extract.rs`) —
  Convenience method that returns `[x, y, width, height]`.  Intended for
  direct use as the `bbox` argument to `replace_fragments_fit_to_bbox`.

- **`BatchEntry<'a>`** (`src/document/types.rs`) —
  One entry for the new `replace_text_fragments_batch_opts` method.  Carries
  its own `FragmentReplaceOpts`, enabling per-cell `font_size`, `max_width`,
  `shrink_to_fit`, and `color` in a single batch call.

- **`FitOptions`** (`src/document/types.rs`) —
  Options struct for `replace_fragments_fit_to_bbox`.  Fields:
  `shrink_to_fit: bool` (default `true`), `min_font_size: f32` (default `6.0`),
  `color: Option<Color>` (default black).

- **`PageHandle::replace_text_fragments_batch_opts`** (`src/document/page.rs`) —
  Like `replace_text_fragments_batch` but each entry carries its own
  `FragmentReplaceOpts`.  All suppressions are collected in a single pass
  before any new text is placed, keeping byte offsets stable across entries.

- **`PageHandle::replace_fragments_fit_to_bbox`** (`src/document/page.rs`) —
  Convenience wrapper that suppresses `fragments` and places `new_text` sized
  to fit within a cell bounding box.  Derives `max_width` from `bbox[2]`
  automatically; pass `TableCell::bbox()` directly.

### Changed (harumi)

- **`extract_table_cells` uses `tm_lm_x` for column assignment when available**
  (`src/extract.rs`) — When a majority of fragments have `tm_lm_x` set (from
  the v1.7.0 T_lm tracking), columns are derived from the exact T_lm anchors
  rather than the X-density histogram.  This gives correct label/value column
  separation for form PDFs that use a single BT block with Td jumps.  Falls
  back to the histogram for PDFs without scaled Tm.

- **`TableCell` is now `#[non_exhaustive]`** (`src/extract.rs`) —
  Allows future field additions without a semver-breaking change.

### Internal (no API change)

- **`src/document.rs` split into `src/document/` module** — Pure mechanical
  refactoring; no public API changes.  The 6857-line file is now four files:
  `types.rs` (types), `helpers.rs` (utility functions + tests), `page.rs`
  (`PageHandle` + replace methods), `mod.rs` (`impl Document`).  Maximum file
  length is now ~3100 lines.

- **`src/extract.rs` tests moved to `src/extract_tests.rs`** — `extract.rs`
  reduced from 4748 to 3314 lines.  Tests are linked via
  `#[path = "extract_tests.rs"] mod tests`.

---

## [1.7.0] — 2026-06-19

### Added (harumi)

- **`TextFragment::tm_lm_x / tm_lm_y: Option<f32>`** (`src/extract.rs`) —
  X/Y position of the *text line matrix* (T_lm) at the start of each `Tj`.
  Unlike `tm_origin_x` (set only by `Tm` and never moved by `Td`), this field
  updates on every `Td` operator, giving the **row anchor** for each Td-based
  line inside a BT block.
  For in-place translation of form/table PDFs, use `tm_lm_x` as the placement
  coordinate — it provides a clean column start free of accumulated glyph-advance
  drift, which `x` accumulates across multiple `Tj` calls on the same line.
  `None` when no `Tm` was seen before the first `Tj` in the current BT block.

### Fixed (harumi)

- **`Td` / `TD` now correctly implement PDF spec T_lm semantics** (`src/extract.rs`) —
  Per PDF spec, `tx ty Td` sets
  `T_lm_new = [[1,0,0],[0,1,0],[tx,ty,1]] × T_lm` and resets the text cursor
  (`T_m`) to `T_lm_new`, clearing any intra-line glyph-advance accumulation.
  The previous implementation (`x += tx * tm_x_scale`) instead accumulated from
  the current cursor, causing column positions to drift by the advance width of
  all previous glyphs on the same line.
  After this fix, form/table PDFs that use a single BT block with alternating
  `Td` jumps between a label column and a value column correctly report each
  column's clean starting position in `TextFragment::x` and `tm_lm_x`:
  labels always land at the left margin, values always land at the value column,
  regardless of how much text the previous row contained.
  This is a **behavioral change** for PDFs with non-identity Tm and horizontal Td
  movements; uniform-scale PDFs see correct positions where they previously
  drifted. The regression test `form_pdf_column_stability` covers this pattern.

---

## [1.6.0] — 2026-06-19

### Added (harumi)

- **`TextFragment::tm_x_scale: Option<f32>`** (`src/extract.rs`) —
  X-scale factor from the most recent `Tm` matrix: √(a² + b²).  Symmetric to the
  existing `tm_y_scale` field.  For axis-aligned Tm (no rotation) this equals the
  horizontal scaling factor applied to glyph advances and `Td` offsets.  Useful
  for distinguishing the effective visual size from the raw `Tf` font size when a
  PDF encodes text with `font_size=1` and a large Tm scale factor.
  `None` when no `Tm` was seen before the first `Tj` in the current BT block.

### Fixed (harumi)

- **Glyph advance width uses Tm x-scale, not y-scale** (`src/extract.rs`) —
  `TextFragment::width` now uses `tf_font_size × tm_x_scale × ctm_scale` (the
  horizontal axis scaling) instead of the y-axis `font_size`.  For uniform Tm
  (a == d, b == c == 0) the result is identical; for non-uniform Tm (different
  horizontal and vertical scaling) the reported width is now geometrically
  correct.

- **TJ kerning uses Tm x-scale** (`src/extract.rs`) —
  Numeric elements in `TJ` arrays are in *thousandths of a text-space unit*
  (horizontal).  The kern cursor advance now uses `tf_font_size × tm_x_scale`
  instead of the y-axis `font_size`, fixing cursor drift for non-uniform Tm.

- **`tm_origin_x/y` is `None` when no `Tm` operator precedes the first `Tj`**
  (`src/extract.rs`) — Previously, the `tm_origin_set` flag was missing, causing
  Td-only BT blocks (no `Tm`) to expose a spurious `Some(0.0)` as `tm_origin_x`.
  Regression test `tm_origin_preserves_column_anchor` covers this.

---

## [1.5.19] — 2026-06-19

### Fixed (harumi)

- **`tm_origin_x/y` is `None` when no `Tm` operator precedes the first `Tj`**
  (`src/extract.rs`) — Added `ParseCarryState::tm_origin_set: bool` flag, reset
  on `BT` and set on `Tm`.  Fragments produced by streams that use only `Td` (no
  `Tm`) no longer report a spurious `Some(0.0)` for `tm_origin_x`.

---

## [1.5.18] — 2026-06-19

### Added (harumi)

- **`PageHandle::replace_text_fragments_batch(entries, font, opts)`** (`src/document.rs`) —
  Replaces text for multiple logical lines in a single content-stream pass.
  All suppression targets are collected up-front before any stream is written,
  so each content stream is rewritten exactly once.  This eliminates the
  byte-offset shift that occurs when `replace_text_fragments` is called
  repeatedly on the same stream; subsequent callers no longer see stale
  `source_op_end` values.  Fragments from different `source_stream` indices or
  Form XObjects are all handled correctly in the same batch call.

- **`FragmentReplaceOpts.dry_run: bool`** (`src/document.rs`) —
  When `true`, count how many `Tj`/`TJ` operators would be suppressed without
  writing any content stream or queueing new text.  Both
  `replace_text_fragments_opts` and `replace_text_fragments_batch` respect this
  flag.  Useful for pre-flight checks before committing to an in-place
  replacement.  Default `false`.

- **`FragmentReplaceFailureReason` enum** (`src/document.rs`) —
  Structured diagnostic returned by `can_suppress_fragment`.  Variants:
  `NoSourceInfo`, `StreamIndexOutOfRange`, `XObjectNotFound`,
  `OperatorNotFound`, `DecompressFailed`.  Exported from `lib.rs`.

- **`PageHandle::can_suppress_fragment(fragment)`** (`src/document.rs`) —
  Read-only check: returns `Ok(())` if the fragment's source operator is
  locatable in the current stream state, or `Err(FragmentReplaceFailureReason)`
  when it cannot be suppressed.

### Changed (harumi)

- **`replace_text_fragments` doc comment** — offset stability warning added,
  pointing callers to `replace_text_fragments_batch`.

- **`replace_text_fragments_batch` doc comment** — cross-stream behaviour
  clarified: fragments spanning multiple `source_stream` indices are all handled
  in one batch call; `opts.dry_run` is honoured.

- **Internal refactor** — `suppress_ops_in_object` and `count_ops_in_object`
  extracted as module-level free functions, shared by both
  `replace_text_fragments_opts` and `replace_text_fragments_batch`.

---

## [1.5.17] — 2026-06-19

### Added (harumi)

- **`text_fragment_bounds(fragments: &[TextFragment]) -> Option<[f32; 4]>`** (`src/extract.rs`) —
  Returns the axis-aligned bounding box `[x, y, width, height]` in PDF points
  covering all fragments in the slice.  Each fragment's vertical extent is
  estimated as baseline ± `font_size × 0.25/0.75` (descender/ascender
  approximation, accurate for most Latin and CJK fonts).  Returns `None` for an
  empty slice.  Useful for computing the cover rectangle needed to erase original
  text in overlay fallback mode.  Exported from the top-level crate.

- **`FragmentReplaceOpts.shrink_to_fit: bool`** (`src/document.rs`) —
  When `true` and `max_width` is set, the replacement font size is reduced
  proportionally (using `calculate_text_width`) until the text fits on one line
  within `max_width`.  Size is never reduced below `min_font_size`.  Default `false`.

- **`FragmentReplaceOpts.min_font_size: f32`** (`src/document.rs`) —
  Floor font size for `shrink_to_fit`.  Default `4.0` pt.

---

## [1.5.16] — 2026-06-18

### Added (harumi)

- **`TextFragment.source_xobject`** (`src/extract.rs`) —
  New public field `source_xobject: Option<(u32, u16)>` on `TextFragment`.
  When a fragment is extracted from a Form XObject stream, this field holds the
  lopdf `ObjectId` `(object_number, generation_number)` of that XObject.
  Complements the existing `source_stream` / `source_op_start` / `source_op_end`
  fields so every extractable fragment — whether it comes from a page content
  stream or a Form XObject — carries a complete source reference.

- **`PageHandle::replace_text_fragments_opts(fragments, new_text, font, opts)`** (`src/document.rs`) —
  Like `replace_text_fragments` but with full placement control via
  `FragmentReplaceOpts`.  Both page content streams and Form XObject streams are
  now handled: fragments with `source_xobject` have their originating operator
  suppressed inside the XObject stream directly, and the replacement text is
  placed on the page at the anchor fragment's coordinates.

- **`FragmentReplaceOpts`** (`src/document.rs`, re-exported from `src/lib.rs`) —
  New `#[non_exhaustive]` options struct for `replace_text_fragments_opts`:
  - `font_size: Option<f32>` — override the anchor fragment's font size (`None` = use fragment's).
  - `max_width: Option<f32>` — wrap the replacement text to this width using `wrap_paragraph`.
  - `y_offset: f32` — shift the placement Y coordinate (default `0.0`).
  - `color: Option<Color>` — text color override (default black).

### Changed (harumi)

- **`replace_text_fragments`** now delegates to `replace_text_fragments_opts`
  with `FragmentReplaceOpts::default()`.  Behaviour is unchanged for existing
  callers.

- **`replace_text_fragments` XObject support** — the function now suppresses
  operators in Form XObject streams (in addition to page content streams).
  Fragments from XObjects (identified by `source_xobject.is_some()`) are grouped
  by ObjectId, and each XObject stream is rewritten with the same
  decompress → `parse_ops` → rebuild → write-back pattern as page streams.

---

## [1.5.15] — 2026-06-18

### Added (harumi)

- **`TextFragment` source-operator fields** (`src/extract.rs`) —
  Three new public fields on `TextFragment` link each fragment back to the
  content-stream operator that produced it:
  - `source_stream: Option<usize>` — zero-based index into the page `/Contents`
    array.  `None` for fragments that come from Form XObjects.
  - `source_op_start: Option<usize>` — byte offset of the `Tj` / `TJ` keyword
    in the decompressed stream identified by `source_stream`.
  - `source_op_end: Option<usize>` — byte offset one past the keyword end
    (`source_op_start + 2` for both operators).
  These fields enable `replace_text_fragments` (below) and give callers full
  traceability from a rendered glyph back to its PDF operator.

- **`PageHandle::replace_text_fragments(fragments, new_text, font)`** (`src/document.rs`) —
  Suppress the `Tj`/`TJ` operators that produced the given `TextFragment` slice
  and place `new_text` at the first fragment's position.
  Each fragment with a valid `source_stream` / `source_op_start` has its
  source operator rewritten to `() Tj` (empty string — glyph not rendered).
  `new_text` is then queued as a `PendingOp::Text` run at the anchor fragment's
  `(x, y, font_size)`.  Returns the number of operators suppressed.
  Primary use case: per-character PDFs (PScript5/Distiller or Type3 layouts)
  where each glyph lives in a separate `BT (ch) Tj ET` block, making
  `replace_text("original line", "translation")` structurally impossible.

### Changed (harumi)

- **`tokenize()` internal return type** (`src/extract.rs`) —
  Changed from `Vec<Token>` to `Vec<(Token, usize)>` to carry the byte offset
  of each token.  This is a private function; no public API is affected.

- **`parse_content_stream()` signature** (`src/extract.rs`) —
  Added `stream_idx: Option<usize>` parameter (private function; callers in
  `extract_text_runs_from_page` pass the loop index; XObject paths pass `None`).

---

## [1.5.14] — 2026-06-18

### Fixed (harumi)

- **Cross-stream BT/ET text state carry** (`src/extract.rs`) —
  `parse_content_stream()` previously reset `in_bt`, the current font name,
  font size, and text position to defaults at the start of every call.
  PScript5.dll/Distiller PDFs occasionally split a single logical BT…ET block
  across multiple stream objects in the page `/Contents` array; the Tj/TJ
  operators in the second and subsequent streams were silently discarded because
  `in_bt` was `false` at their start.  All text-state variables (`in_bt`,
  `font_name`, `tf_font_size`, `font_size`, `tm_y_scale`, `text_x`, `text_y`)
  are now stored in `ParseCarryState` and survive stream boundaries, matching
  the existing behaviour for graphics state (CTM, colour, render mode).

---

## [1.5.13] — 2026-06-18

### Fixed (harumi)

- **XObject font resolution for PScript5.dll/Distiller PDFs** (`src/extract.rs`) —
  `xobject_fonts()` previously fell back to page-level fonts only when the Form XObject
  had *no* `/Resources` dict at all.  When the XObject carried a `/Resources` dict that
  lacked a `/Font` sub-entry (common in PDFs generated by PScript5.dll/Distiller, where
  fonts are declared on the page rather than inside each XObject), the function returned
  an empty font map and all text inside that XObject was silently discarded.  The
  function now always starts with the page fonts as a base and overlays any
  XObject-specific fonts on top, so fonts are resolved correctly in all three cases:
  no XObject `/Resources`, `/Resources` with `/Font`, and `/Resources` without `/Font`.

---

## [1.5.12] — 2026-06-17

### Fixed (harumi)

- **Silent content stream skip on decompression failure** (`src/extract.rs`) —
  `page_content_streams()` and `decode_form_xobject()` previously discarded any
  stream where `stream.decompress()` returned `Err`, producing silently incomplete
  text extraction.  For AES-256 encrypted PDFs generated by PScript5.dll/Acrobat
  Distiller, lopdf may have already decoded the stream during `load_with_password()`,
  leaving the final uncompressed bytes in `stream.content` with the `/Filter` entry
  still present; calling `decompress()` on those bytes fails, dropping the stream.
  Both functions now fall back to `stream.content` directly when `decompress()` fails
  and content is non-empty.  This fixes PDFs returning only 13 fragments instead of
  the expected 40–50+ per page.

- **Zero advance-width fallback** (`src/extract.rs`) — `decode_chars_to_fragment()`
  now uses `char_count × font_size × 0.5` when `total_width == 0.0` after the glyph
  loop.  Prevents `detect_text_columns()` from treating zero-width fragments as
  1-point-wide blobs, which skewed column boundary detection.

### Added (harumi)

- **`ExtractionWarning` diagnostic API** (`src/extract.rs`, `src/document.rs`) —
  New `WarningKind` enum (`StreamDecompressFailed` / `XObjectSkipped`) and
  `ExtractionWarning { kind, stream_id, message }` struct exported from `harumi`.
  New `Document::extract_text_runs_verbose(page)` returns
  `(Vec<TextFragment>, Vec<ExtractionWarning>)` — a non-empty warning list identifies
  which stream object IDs fell back to raw content.

- **`TextFragment.tf_font_size` and `TextFragment.tm_y_scale`** (`src/extract.rs`) —
  Two new fields on the `#[non_exhaustive]` `TextFragment` struct (non-breaking):
  `tf_font_size` is the raw size from the `Tf` operator; `tm_y_scale` is `√(c²+d²)`
  from the last `Tm` matrix.  PDFs using the pattern `1 Tf  9 0 0 9 x y Tm` emit
  `tf_font_size=1` and `tm_y_scale=9`, allowing harumi-ai to recover the true visual
  size without guessing.

---

## [1.5.11] — 2026-06-17

### Fixed (harumi)

- **Horizontal `Tm` in cross-op matching** (`src/replace.rs`) —
  Traditional Japanese PDF generators position each character with an absolute
  `Tm` operator on the same visual line (e.g. `100 700 Tm <65E5> Tj  113 700 Tm <672C> Tj`).
  The intermediate-ops whitelist in `find_cross_op_matches_inner()` and
  `find_cross_tf_matches_inner()` rejected `Tm`, discarding every cross-op or
  cross-Tf match that spanned per-character `Tm` positioning.
  Five coordinated changes:
  (1) `collect_char_segments()` now detects vertical `Tm` (y-delta ≥ 1 pt = new line)
  and flushes; horizontal `Tm` (same y) is silently accumulated, guaranteeing that
  any `Tm` inside a cross-op match range is horizontal and safe to suppress.
  (2) `collect_cross_tf_segments()` applies the same y-delta check — horizontal `Tm`
  no longer breaks cross-Tf segment accumulation.
  (3) `find_cross_op_matches_inner()` adds `Tm` and text-state ops (`Tc`, `Tw`, `Tz`,
  `TL`, `Ts`) to the allowed-intermediate-ops whitelist.
  (4) `find_cross_tf_matches_inner()` same extensions.
  (5) `rewrite_content_stream()` suppresses `Tm`, `Tc`, `Tw`, `Tz`, `TL`, `Ts` ops
  that fall inside a cross-op match region, analogous to the existing `Td`/`Tf`
  suppression.

---

## [1.5.10] — 2026-06-17

### Fixed (harumi)

- **Cross-BT match count always zero** (`src/replace.rs`) —
  `count_matches_in_raw_streams()` had no cross-BT counting pass; only
  intra-segment and cross-Tf passes existed.  For Chrome/Skia Type3 PDFs (one
  character per `BT`/`Tj`/`ET` block), `count_matches_in_page()` returned 0
  even after whitespace normalization, so `replace_text_opts()` never queued
  `PendingOp::Replace`.  Added a cross-BT pass that tracks `cur_font` without
  an `in_bt` guard (PDF text state persists across `BT`/`ET`), collects
  characters with their BT-block index, and counts only matches where the
  first and last characters come from different BT blocks — preventing
  double-counting of intra-BT matches already handled by the existing passes.

- **`find_cross_bt_matches()` font tracking** (`src/replace.rs`) —
  The character-collection loop cleared `cur_font` on every `BT` operator and
  gated `Tf` processing on `in_bt`, causing empty font names for BT blocks
  where a `Tf` from outside or before the block was still in effect (legal per
  PDF spec: text state persists across `BT`/`ET`).  Fixed by removing
  `cur_font.clear()` from the `BT` handler and removing the `in_bt` guard from
  `Tf`, mirroring the proven logic in `diagnose_match_failure()` Tier 3.

---

## [1.5.9] — 2026-06-17

### Added (harumi)

- **`ReplaceOptions` + `PageHandle::replace_text_opts()`** (`src/document.rs`) —
  New options struct and method variant of `replace_text()`.  The primary option is
  `normalize_whitespace: bool`: when `true`, all whitespace is stripped from `old_text`
  before matching and replacement.  This lets callers assembled from
  `TextFragment.text` values joined with spaces (e.g. harumi-ai's default grouping
  produces `"T h e F r e e"`) still match text stored as bare glyphs in Chrome/Skia
  Type3 BT-per-char PDFs (`"TheFree"`).  The matching pipeline in `replace.rs` is
  unchanged; normalization is a one-liner at the API boundary.  `replace_text()`
  itself is unchanged — no semver break.

- **`TextFragment.space_advance`** (`src/extract.rs`) —
  New field on the `#[non_exhaustive]` `TextFragment` struct (non-breaking addition).
  Holds the advance width of the space glyph (U+0020) in PDF points at the
  fragment's font size, or `0.0` when the font has no space glyph.  Callers
  (e.g. harumi-ai) can compare `gap = next.x - (prev.x + prev.width)` against
  `prev.space_advance` to decide whether adjacent fragments represent a word space
  or tight character spacing, avoiding unconditional space insertion that turns
  `"10M+"` into `"1 0 M +"`.

---

## [1.5.8] — 2026-06-17

### Fixed (harumi)

- **`finalize()` CTM isolation** (`src/document.rs`) —
  `finalize()` now calls `wrap_page_contents_in_q_q()` before `append_to_contents()`
  when flushing pending text/draw operations onto an existing page.  Previously, any
  unbalanced `cm` operator in the existing page content could leak into the newly
  appended stream and misplace the added content.  `overlay_from()` already had this
  fix; the `finalize()` path was simply missed.

- **`ctm_stack` persists across multiple content streams** (`src/extract.rs`) —
  `ParseCarryState` gains a `ctm_stack: Vec<[f32; 6]>` field (initialized to
  `[IDENTITY_CTM]`) that replaces the local `ctm_stack` variable inside
  `parse_content_stream()`.  Per the PDF spec, multiple streams in a `Contents`
  array share the same graphics state; previously each stream restarted the CTM
  stack from `state.ctm` (last `Do`-time CTM only), causing incorrect text
  coordinates when a page had several content streams with `cm` operators between
  them.  `extract_text_from_xobjects()` saves/restores the stack around each
  Form XObject call so every XObject gets a fresh stack seeded with its own
  `multiply_ctm(do_ctm, xobj_matrix)`.

- **Cross-BT `Tj` replacement for Type3 fonts** (`src/replace.rs`) —
  Chrome/Skia PDFs place each character in its own `BT … Tj … ET` block.
  The existing `rewrite_content_stream()` only matched text within a single BT/ET
  block, so `replace_text()` always returned 0 on these PDFs.
  New `CrossBtMatch` struct + `find_cross_bt_matches()` function detect replacement
  targets that span multiple BT/ET blocks.  The rewriter condenses all matched
  blocks into one, preserving the first block's positioning setup (Tf/Tm/Td),
  emitting the replacement text, and suppressing the remaining blocks.

---

## [1.5.6] — 2026-06-15

### Changed

- Japanese changelog (`CHANGELOG_ja.md`) now documents cross-`Tf` text matching,
  `TranslationMode::InPlace`, `TranslateOptions::cover_color`, synthetic bold
  rendering, and related harumi-ai features (previously documented only in English
  under v1.4.5).

---

## [1.5.5] — 2026-06-15

### Fixed (harumi)

- **Overlay CTM coordinate transform** (`src/extract.rs`) —
  `parse_content_stream()` now tracks `q`/`Q`/`cm` graphics-state operators and
  maintains an internal CTM stack.  Text coordinates are transformed to page space
  at emission time via `apply_ctm()`.  The CTM that is active at each `Do` operator
  is captured in `ParseCarryState.ctm` and forwarded to
  `extract_text_from_xobjects()`, which composes it with each Form XObject's own
  `/Matrix` before parsing the XObject's content stream.

  Chrome/Skia PDFs open with `q → 0.24 0 0 -0.24 0 841.92 cm → Do → Q` at the top
  of the page content stream.  Before this fix, `TextFragment` coordinates were in
  the XObject's local space (e.g. x=500, y=3000) rather than page space (x=120,
  y=121).  Overlay mode used the raw local coordinates, causing translated text to
  appear tiny, inverted, and clustered in the top-left corner of the page.

---

## [1.5.4] — 2026-06-15

### Fixed (harumi)

- **Type3 font support in text extraction** (`src/extract.rs`) —
  `collect_font_dict_entries()` matched only `Type0`, `Type1`, `MMType1`, and
  `TrueType` subtypes; `/Subtype /Type3` fonts fell through to `_ => continue`
  and were never added to the font map.  Chrome/Skia-generated PDFs (e.g. fonts
  F34/F35/F36 in Sample.pdf) use exclusively Type3 fonts, so the `fonts`
  `HashMap` was empty and no `TextFragment`s were produced — causing
  `harumi-ai` translation output to be zero.
  Type3 fonts share the same 1-byte character-code scheme and `/ToUnicode` CMap
  structure as Type1/TrueType, so routing them through `collect_simple_font()`
  is sufficient.  A single `| Some(b"Type3")` added to the match arm fixes the
  extraction path.

---

## [1.5.3] — 2026-06-15

### Fixed (harumi)

- **Form XObject discovery via inherited `/Resources`** (`src/extract.rs`) —
  `extract_text_from_xobjects()` previously looked only at the page dict's own
  `/Resources` key; when it was absent (Chrome/Skia places `/Resources` on a
  parent `/Pages` node per PDF §7.7.3), the function returned early and produced
  zero text fragments.  A new `collect_inherited_xobject_ids()` helper walks the
  `/Parent` chain to find `/Resources/XObject`, matching the fix applied to
  `collect_fonts_inner()` in v1.5.2.  Chrome/Skia-generated PDFs now have their
  text correctly extracted via the Overlay fallback path.

- **`replace_text()` now rewrites Form XObject content streams** (`src/replace.rs`,
  `src/document.rs`) — `count_matches_in_page()` and `rewrite_page_streams()` only
  scanned the page's `/Contents` streams.  Text in Form XObjects (the common structure
  in Chrome/Skia PDFs) was never found, so `replace_text()` always returned 0 and
  `harumi-ai` InPlace mode fell back to Overlay for every line.
  - `count_matches_in_page()` refactored into `count_matches_in_raw_streams()` (shared
    helper, ReDoS guard + cross-Tf pass) and extended to also search XObject streams via
    `count_matches_in_inherited_xobjects()`.
  - `rewrite_form_xobject_streams()` discovers Form XObjects from inherited resources,
    rewrites each one's content with the new font encoding, and returns the modified
    `(xobj_id, new_content, fonts_used)` triples.
  - `add_font_to_xobject_resources()` registers the new font in the XObject's own
    `/Resources/Font` dict (inline or indirect), keeping XObject resources self-contained.
  - `finalize()` Replace pass calls `rewrite_form_xobject_streams()` after
    `rewrite_page_streams()`, updates XObject stream content in-place (removes `/Filter`,
    sets `/Length`), and registers the new font per XObject.

### Tests

- `extract_xobjects_from_inherited_resources` — Type1 font + inherited `/Resources/XObject`
  (validates the P0 parent-chain walk).
- `extract_cid_xobject_inherited_resources` — Type0/CID font + Identity-H + ToUnicode CMap
  + `<XXXX> Tj` hex glyph IDs inside an inherited XObject (validates the actual Chrome/Skia
  decode path that was previously untested).
- `replace_text_in_form_xobject_inherited_resources` — end-to-end round-trip: build a
  synthetic Chrome/Skia style PDF (Type0/CID + inherited `/Resources/XObject`), call
  `replace_text("Hi", "Bye", font)`, save and reload, assert text was replaced.

---

## [1.5.2] — 2026-06-15

### Fixed (harumi)

- **Font inheritance from ancestor Pages nodes** (`src/extract.rs`) —
  `collect_fonts_inner()` now walks the `/Parent` chain when a page has no
  `/Resources` dictionary.  PDF §7.7.3 permits fonts to be declared on ancestor
  Pages nodes; Chrome/Skia-generated PDFs commonly do this, causing the font map
  to be empty and all text extraction to fail silently.

### Fixed (harumi-ai)

- **JSON repair for LLM-emitted unescaped quotes** (`src/extractor.rs`) —
  `json_to_translated_pages()` now tries a direct `serde_json` parse first, then
  falls back to `repair_json_strings()`, a char-level state machine that escapes
  interior unescaped `"` inside `"text"` string values that LLMs sometimes emit
  without the required `\"` escaping (e.g. section references).  4 unit tests added.
- **Prompt instruction for quote escaping** (`src/prompts.rs`) —
  `translation_system_prompt()` and `layout_correction_prompt()` now explicitly
  instruct the model to escape double-quotes inside translated text values as `\"`.
- **Default `max_tokens` raised 4 096 → 16 000** (`src/providers/anthropic.rs`) —
  prevents mid-translation truncation on dense documents.

---

## [1.5.1] — 2026-06-15

### Fixed (harumi-ai)

- **CJK byte-slice panic in InPlace debug log** (`harumi-ai/src/inplace.rs`) —
  debug logging truncated `line.text` by byte index (`&s[..s.len().min(60)]`), which panicked
  when a Japanese/Chinese/Korean string had byte offset 60 inside a multi-byte UTF-8 character
  (e.g. the kana `く`).  Fixed by switching to `chars().take(60).collect::<String>()`.
  Added 10 unit tests to guard against regression.

### Changed (harumi-ai)

- `repeat().take()` → `std::iter::repeat_n()` in `inplace.rs` and `overlay.rs` (Clippy lint).
- Manual `div_ceil` arithmetic replaced with `div_ceil()` in `overlay.rs` (Clippy lint).

---

## [1.5.0] — 2026-06-15

### Added (harumi)

- **`group_text_fragments(fragments, strategy) -> Vec<TextGroup>`** (`src/extract.rs`) —
  merges individual `TextFragment`s into logical text blocks before handing them to a
  translation model.  Three strategies are available via `GroupingStrategy`:
  - `Raw` — identity (one fragment = one group)
  - `Line` — fragments within ±½ font-size on the same baseline are merged
  - `Paragraph` — adjacent lines separated by a gap ≤ 1.5 × line height are merged
  `TextGroup` exposes `text: String`, `fragments: Vec<TextFragment>`, and a bounding box
  `(x, y, width, height)`.  Exported from the crate root along with `GroupingStrategy` and
  `TextGroup`.

- **`font_covers_char(font_bytes: &[u8], ch: char) -> bool`** (`src/document.rs`) —
  queries the font's `cmap` table via ttf-parser to determine whether the font contains a
  glyph for `ch`.  Returns `false` when `font_bytes` cannot be parsed.  Useful for choosing
  between primary and fallback fonts before embedding.

- **Form XObject recursive text extraction** (`src/extract.rs`) —
  `extract_text_runs` now descends into `Form`-subtype XObjects referenced by `Do` operators
  in the page content stream.  Headers, footers, and watermarks stored as Form XObjects are
  included in the extracted fragments.  Recursion depth is capped at 5.  Each XObject uses
  its own `/Resources` font dictionary, falling back to page-level fonts when absent.

- **Cross-content-stream graphics state** (`src/extract.rs` — `ParseCarryState`) —
  When a page `/Contents` entry is an array of streams, the PDF spec requires colour and
  render-mode state to carry over between streams.  A new `ParseCarryState { cur_color,
  cur_render_mode }` struct is propagated across `parse_content_stream` calls so that text
  in the second stream inherits colour or render-mode operators set in the first stream.

- **`extract_table_cells(fragments, page_width, page_height) -> Vec<TableCell>`** (`src/extract.rs`) —
  detects table structure from a flat `TextFragment` slice using two independent passes:
  - **Columns** — delegates to `detect_text_columns` (reuses the X-density gap algorithm).
  - **Rows** — after `sort_by_reading_order`, fragments are clustered by Y proximity: a gap
    larger than `½ × font_size` of the row's first fragment starts a new row.
  Each occupied `(row, col)` pair becomes one `TableCell` with 0-based `row`/`col` indices,
  merged `text` (fragments within the cell are joined left-to-right), and a bounding box
  `(x, y, width, height)`.  Results are sorted by row then column.
  **Limitation (documented):** detection is heuristic.  PDFs without visible grid lines,
  merged cells, or nested tables may produce incorrect row/column assignments.
  `TableCell` and `extract_table_cells` are exported from the crate root.

### Added (harumi-ai)

- **`OverflowStrategy`** (`harumi-ai/src/pdf_translator.rs`) — controls what happens when
  translated text is wider than the original bounding box:
  - `Shrink { min_font_size: f32 }` — scale the font down to `min_font_size` (default
    `6.0 pt`).  This was the only behaviour before; it is now the explicit default.
  - `Truncate { min_font_size: f32 }` — scale down first; if still too wide at
    `min_font_size`, clip the text and append `"…"`.
  Set via `TranslateOptions::overflow` or `TranslateOptionsBuilder::overflow(strategy)`.

- **`TranslateOptions::font_fallbacks: Vec<Vec<u8>>`** — additional TTF/OTF fonts tried in
  order when the primary `font` does not contain a glyph for a character.  harumi-ai
  partitions each translated text run into sub-runs by font (using `split_by_font`), embeds
  only fonts that are actually used, and renders each sub-run at the correct x offset.
  Add fallbacks via `TranslateOptionsBuilder::add_font_fallback(bytes)`.

- **`TranslateOptions::progress_fn`** — optional callback invoked after each batch of pages
  is translated, with signature `Fn(pages_done: u32, total_pages: u32)`.  Intended for
  streaming progress to clients that would otherwise time out on large PDFs.  Register via
  `TranslateOptionsBuilder::on_progress(fn)`.  Works for all three `TranslationMode`s.

---

## [1.4.5] — 2026-06-15

### Added (harumi)

- **Cross-Tf (cross-font) text matching in `replace_text`** (`harumi/src/replace.rs`) —
  `replace_text` / `can_replace_text` now match text that spans `Tf` (font-change) operators
  within the same BT/ET block.  Japanese PDFs frequently encode a single visual line across
  multiple font runs (e.g. body Kanji in `F1`, bracket characters in `F2`), so the old
  single-segment matching missed these lines entirely.  New helpers:
  `collect_cross_tf_segments()` merges characters across `Tf` without splitting; the
  existing `find_cross_op_matches_inner()` whitelist is extended with `b"Tf" => true`;
  `find_cross_tf_matches_inner()` emits matches only when the op range contains at least
  one `Tf` (no double-counting with same-font matches); and `rewrite_content_stream()`
  suppresses intermediate `Tf` ops inside a match region the same way it already
  suppressed intermediate `Td` ops.  `Tm` operators always break the merged segment
  (absolute position reset = new visual line).  `CharEntry` gains a `font_name` field so
  per-character advance widths are computed from the correct font in the cross-Tf case.

- **`PageHandle::diagnose_replace_failure(old_text) -> &'static str`** — public helper
  that classifies why `replace_text` returned 0 for a given string.  Returns one of
  `"cross-Tf"` (text found in merged view but not in per-font segments — should not
  appear after this release), `"vertical-Td-or-Tm"` (text found in a single-font segment
  but a line break interrupted the op range), or `"text-not-in-stream"` (text not present
  in any segment at all).  Intended for debug logging in harumi-ai fallback branches.

### Added (harumi-ai)

- **Per-character PDF support for InPlace mode** (`harumi/src/replace.rs`) — `find_cross_op_matches_inner()`
  now allows horizontal-only `Td`/`TD` operators (ty == 0) between `Tj`/`TJ` operators when building
  cross-operator matches.  The per-character advance pattern common in Japanese PDFs (`(A)Tj 12 0 Td (B)Tj …`)
  is now matched and the intermediate `Td` operators are suppressed in the rewritten stream.
  Width compensation uses `Tz` (horizontal scale) when 70%–130% of the original advance, falling back
  to `Td` otherwise.  Both `rewrite_content_stream` and `rewrite_stream_preserve_font` updated.

- **InPlace fallback debug logging** (`harumi-ai/src/inplace.rs`) — in debug builds
  (`cfg!(debug_assertions)`), each line that falls back to overlay now emits an
  `[harumi-ai] fallback page=N reason=R text=…` line to stderr, where `R` is the reason
  returned by `PageHandle::diagnose_replace_failure`.  Zero overhead in release builds.

- **`TranslationMode::InPlace`** (`harumi-ai/src/inplace.rs`, `pdf_translator.rs`) — new translation
  mode that rewrites PDF content streams directly via `harumi::PageHandle::replace_text()`. The
  original `Tj`/`TJ` operators are replaced in-place so the source text is eliminated from the
  stream; white cover rectangles are not needed. Lines where no match is found (e.g. per-character
  Japanese PDFs with `Td` between each `Tj`) fall back automatically to the overlay approach for
  that line only.  Enable with `opts.mode = TranslationMode::InPlace`.

- **`TranslateOptions::cover_color`** (`harumi-ai/src/pdf_translator.rs`) — optional RGB cover
  color for overlay mode (default: `None` = white `[1.0, 1.0, 1.0]`). Useful when the source
  PDF has a non-white background (safety signs, coloured headers). Also exposed via
  `TranslateOptionsBuilder::cover_color()`.

### Changed (harumi-ai)

- **Synthetic bold for headings/bold lines** (`harumi-ai/src/overlay.rs`) — translated text on
  lines where `is_heading || is_bold` is now rendered with `add_text_styled(bold=true)`, which
  uses PDF render mode 2 (fill+stroke, `stroke_width ≈ font_size × 0.04`). No extra font file
  required.

---

## [1.4.3] — 2026-06-15

### Changed (harumi-ai)

- **Per-line font size in overlay mode** (`harumi-ai/src/overlay.rs`) — replaced the
  global `global_body_fs` heuristic (derived from inter-line gaps) with
  `TextFragment.font_size` for each translated line. The global estimate was
  systematically smaller than the actual font size in dense CJK layouts, causing
  translated text to render too small and white cover rectangles to be too short.

---

## [1.4.2] — 2026-06-15

### Changed (harumi-ai)

- **Overlay mode layout accuracy** (`harumi-ai/src/overlay.rs`) — seven fixes for
  layout-preserving PDF translation:
  - **White rect height** now uses per-line `line_height` (derived from actual inter-line
    gap) instead of the global `body_font_size * 1.3` constant.
  - **White rect width** now uses the actual right edge of text fragments plus 2 pt
    padding instead of `page_width - x - 20` fixed margin.
  - **White rect descender coverage** now computed from the font's real descender ratio
    via `ttf-parser` (`face.descender() / face.units_per_em()`) instead of `body_fs * 0.08`.
  - **Translated text Y coordinate** now places text exactly at the original baseline Y
    (`line.y`) instead of applying an arbitrary `- scaled * 0.1` shift.
  - **Multi-column support** — `harumi::detect_text_columns()` detects column boundaries;
    fragments are grouped per column independently, preventing row interleaving on
    2-column PDF layouts. `avail_w` is now bounded by the column right edge rather than
    the page edge.
  - **Heading detection improvement** — `TextFragment::is_bold` (v1.4.1) now contributes
    to `is_heading` detection alongside the existing gap + margin heuristic.
  - **Reading-order sort** now delegates to `harumi::sort_by_reading_order()` (NaN-safe)
    instead of a custom inline comparator.

---

## [1.4.1] — 2026-06-15

### Added

- **`TextFragment::is_bold`** — `true` when the font name indicates a bold weight
  (keywords: Bold, Heavy, Black, Semibold, Demibold, Extrabold).

- **`TextFragment::is_italic`** — `true` when the font name indicates italic or oblique
  style (keywords: Italic, Oblique, Slanted).

- **`TextFragment::font_family`** — font family name derived from the PostScript
  `/BaseFont` entry, with subset prefix (e.g. `"ABCDEF+"`) and style suffixes stripped.
  Empty string when no `/BaseFont` is present.

- **`TextFragment::base_font`** — full PostScript base font name (subset prefix stripped).
  Examples: `"Helvetica-BoldOblique"`, `"NotoSansJP-Regular"`.
  Empty string when no `/BaseFont` is present.

- **`detect_text_columns(fragments, page_width) -> Vec<ColumnZone>`** — infers column
  layout from an X-density histogram of text fragments. Gaps of at least 15 pt with no
  text are treated as column separators. Returns one `ColumnZone` per detected column.

- **`ColumnZone`** — struct with `x_start: f32` and `x_end: f32` (PDF-point coordinates)
  returned by `detect_text_columns`.

---

## [1.4.0] — 2026-06-14

### Added

- **`PageHandle::scale_page_content(scale_x, scale_y)`** — scales all existing page content
  by inserting a `cm` (Concatenate Matrix) operator as a new leading content stream.
  Useful for enlarging a page from A4 to A3 while keeping content proportional.
  See also `resize_page_with_content` for the common "change page size + scale content" workflow.

- **`PageHandle::resize_page_with_content(new_width, new_height)`** — resizes the page's
  MediaBox and scales all existing content proportionally in one call.

- **`Document::overlay_from(other)`** — overlays each page of `other` on top of the
  corresponding page of `self` using a PDF Form XObject. Useful for stamping watermarks,
  signatures, or full-page graphics from a second document. Pages in `self` beyond
  `other`'s page count are left untouched.

- **`Document::clear_outline()`** — removes all bookmarks/table-of-contents entries from
  the document, including any not-yet-saved pending bookmarks and any `/Outlines` tree
  already present in a loaded PDF.

- **`Document::attach_file(filename, data, mime_type)`** — embeds an arbitrary file as a
  PDF attachment (`/EmbeddedFiles`). The file is compressed with FlateDecode before
  embedding.

- **`Document::list_attachments()`** — lists all file attachments embedded in the document.
  Returns `Vec<AttachmentInfo>` with filename, size, and optional MIME type.

- **`AttachmentInfo`** — returned by `list_attachments()`. Fields: `filename: String`,
  `size: usize`, `mime_type: Option<String>`.

### Fixed

- **`add_text_with_opacity` and `add_text_with_rotation` now appear on docs.rs** — these
  methods were defined inside the `#[cfg(feature = "draw")]` impl block (because opacity
  uses the `draw` feature's `ExtGStateRegistry`), so they were invisible when docs.rs
  built with default features only. Added `[package.metadata.docs.rs]` to Cargo.toml
  to build docs with all features enabled.

---

## [1.3.2] — 2026-06-14

### Fixed

- **`build_hmtx` advance_width wrong for "mono" glyphs** (`gid >= num_h_metrics`) —
  the lsb-only section bytes were misread as advance_width and the next glyph's lsb
  was misread as the current glyph's lsb. Fixed to read the last longHorMetric's
  advance_width for mono glyphs and the correct per-glyph lsb offset.

- **`hhea.numberOfHMetrics` undercount** — was capped with `.min(original_num_h_metrics)`.
  Since `build_hmtx` writes all subset glyphs as full 4-byte longHorMetric entries, the
  field must equal `gids_to_keep.len()`.

- **`head.checkSumAdjustment` always 0** — the per-table and full-font checksums were
  never computed. `assemble_font` now writes correct per-table checksums to the table
  directory and sets `checkSumAdjustment = 0xB1B0AFBA − full_font_sum`. The original
  font's non-zero checkSumAdjustment was also being copied into the subset, corrupting
  the sum; it is now zeroed before assembly.

- **Glyph data not 4-byte aligned** — TrueType requires each glyph's data to start on
  a 4-byte boundary. `build_glyf` now pads each glyph with zero bytes to the next
  4-byte boundary. The `loca` format auto-upgrades to long (format 1) if any offset
  exceeds the 131 070-byte short-format limit, with `head.indexToLocFormat` updated
  accordingly.

- **`hhea` and `head` advisory metrics not updated after subsetting** —
  `advanceWidthMax`, `minLeftSideBearing`, `minRightSideBearing`, and `xMaxExtent` in
  `hhea`, and the font bounding box in `head`, are now recomputed from the rebuilt hmtx
  and glyf tables.

- **`char_to_gid` dropped second char when two chars share a glyph** — `char_to_gid`
  was built by inverting `gid_to_char` (one char per GID), silently dropping any
  Unicode codepoint that maps to the same glyph as an earlier codepoint. `SubsetResult`
  now carries a pre-built `char_to_gid` mapping derived directly from all input chars,
  so both codepoints correctly encode to the same new GID.

- **`hhea`/`maxp` bounds checks too loose** — `hhea.len() >= 34` allowed reading bytes
  34–35 with only 34 bytes present; corrected to `>= 36`. `maxp.len() >= 4` allowed
  reading bytes 4–5 with only 4 bytes; corrected to `>= 6`.

- **`head.indexToLocFormat` upgrade logic dead and incorrect** — the previous upgrade
  check compared a long-format loca length against a short-format expected size,
  triggering incorrectly for large subsets of long-format fonts. When it did trigger it
  wrote only the high byte of the big-endian u16, producing 0x0100 (256) instead of
  0x0001. Replaced with a correct check: upgrade only when the max glyph offset exceeds
  the short-format limit (131 070 bytes).

---

## [1.3.1] — 2026-06-14

### Fixed

- **Embedded fonts rendered as ● in macOS Preview and PSPDFKit** — the internal TTF
  subsetter was copying all optional tables verbatim from the source font, including
  `GSUB`, `GPOS`, `gvar`, `fvar`, `post`, `vhea`, `vmtx`, and others. After subsetting
  to a small character set these tables contain stale GID references pointing to glyphs
  that no longer exist. macOS Core Text and PSPDFKit validate embedded fonts and reject
  them as malformed when GID references are inconsistent, causing every glyph to render
  as a ● replacement character.

  The subsetter now uses a whitelist approach: only the core TrueType tables (`head`,
  `hhea`, `maxp`, `glyf`, `loca`, `hmtx`) and safe hinting tables (`fpgm`, `prep`,
  `cvt`, `gasp`) are included in the subset. All other tables — OpenType layout
  (`GSUB`, `GPOS`, `GDEF`, `BASE`), variable-font data (`gvar`, `fvar`, `avar`, `HVAR`,
  `STAT`), and metadata tables (`post`, `name`, `vhea`, `vmtx`, `kern`) — are stripped.
  This is correct for PDF CIDFont embedding with Identity-H encoding, where no OpenType
  shaping is applied.

- **Composite glyph component GIDs not rewritten after subsetting** — when a glyph
  composited from component glyphs was included in the subset, `build_glyf` was copying
  the composite record verbatim, leaving component GID references pointing to the original
  (pre-subset) GID values. After subsetting those GIDs are remapped to new sequential
  positions, so the component references were wrong. `build_glyf` now rewrites component
  GIDs in composite glyph records to their new positions.

- **GID→char mapping incorrect when composite deps expand the kept-glyph set** — the
  char-to-GID mapping was derived from `GlyphRemapper` which only knew about explicitly
  requested glyphs, not the composite dependencies added inside `subset()`. If a composite
  dependency glyph had an original GID that sorted before a requested glyph, the new-GID
  positions were off by one or more. The mapping now uses the final `gids_to_keep` set
  (returned from `subset()`) which includes all composite dependencies.

---

## [1.3.0] — 2026-06-13

### Added (Phase 24: Text Extraction Quality)

- **`sort_by_reading_order(fragments: &mut [TextFragment])`** — reorder text fragments from
  content-stream order to human-readable reading order: top-to-bottom, left-to-right. Handles
  NaN/Infinity coordinates safely. Useful for post-processing `extract_text_runs()` output
  when visual scanning order matters (e.g., multi-column layouts, RTL scripts with fall-back).

- **`glyph_name_to_char()` uni<XXXX> pattern support** — extended AGL glyph name decoding
  to recognize `uni0041`-style (AGL 2.0) patterns, where the hex code directly maps to a
  Unicode scalar. Example: `uni30A2` → `'ア'` (U+30A2). Validates hex length (1-8 chars)
  to reject malformed glyphs silently rather than panic.

- **`Document::extract_page_images(page) -> Vec<PageImage>`** — extract all images from a
  scanned PDF page (previously only returned the largest image). Returns an error if no
  Image XObjects are found.

### Added (Phase 25: AI/RAG Utilities)

- **`TextChunk` struct** — semantic text block extracted from a page, with fields:
  - `text: String` — concatenated fragment text
  - `bbox: [f32; 4]` — bounding box `[x, y, width, height]`
  - `chunk_type: ChunkType` — `Heading(1..=4)` or `Paragraph`
  - `avg_font_size: f32` — average font size of constituent fragments

- **`ChunkType` enum** — `Heading(u8)` (levels 1–6 inferred from font size ratio) or
  `Paragraph`. Marked `#[non_exhaustive]` for future heading levels.

- **`Document::extract_text_chunks(page) -> Vec<TextChunk>`** — extract semantic text blocks
  from a page with automatic heading detection. Algorithm:
  1. Extract text fragments via `extract_text_runs()`
  2. Sort by reading order (top→bottom, left→right)
  3. Filter out invisible fragments (OCR layers)
  4. Group fragments into lines by y-coordinate (±`font_size * 0.5` tolerance)
  5. Estimate baseline font size (minimum of first 10 lines)
  6. Classify lines as headings or paragraphs by font size ratio:
     - ≥1.8× → H1, ≥1.5× → H2, ≥1.3× → H3, ≥1.15× → H4
     - Otherwise → Paragraph
  7. Merge consecutive same-type lines into chunks
  8. Compute bbox as union of constituent fragments

- **`Document::extract_as_markdown(page) -> String`** — extract text from a page as
  Markdown-formatted output. Uses `extract_text_chunks()` internally:
  - Headings: `"#".repeat(level) + " " + text`
  - Paragraphs: plain text
  - Double newlines between blocks

### Security & Robustness

- **NaN/Infinity defensive programming** — all new functions filter malformed floating-point
  values (NaN, Infinity, negative font sizes) that can occur in untrusted PDFs:
  - `group_into_lines()`: fallback tolerance if `font_size` is non-finite
  - `estimate_baseline_font_size()`: exclude NaN/Infinity from baseline calculation
  - `classify_by_ratio()`: treat NaN ratios as paragraphs (unclassified)
  - `sort_by_reading_order()`: place NaN/Infinity coordinates at the end of sort order

### Test Coverage

- Phase 24: 2 integration tests (`uni_glyph_name_pattern_decoding`, `sort_by_reading_order_*`)
- Phase 25: 7 integration tests (`extract_text_chunks_*`, `extract_as_markdown_*`)
- All doctests compile and pass

---

## [0.8.0] — 2026-06-06

### Added

- **`PageHandle::replace_text_resubset(old_text, new_text, font_bytes)`** — replace text in an
  existing content stream while expanding the font subset to include new characters. Accepts the
  original (unsubsetted) TTF/OTF bytes; harumi generates a new subset covering all existing
  characters plus the new ones, re-encodes every content stream that references the font (GIDs
  may shift), and performs the replacement in one `save()` call. Works for any language —
  Chinese, Korean, Arabic — as long as the supplied font contains the characters.
  Only CIDFontType2 fonts with `CIDToGIDMap /Identity` are supported.

- **`InlineSpan`** struct + **`FlowDocument::push_paragraph_styled(spans)`** — mixed bold/italic/
  color inline text within a `FlowDocument` paragraph. Bold is rendered with PDF fill+stroke mode
  (render mode 2, stroke width = 4% of font size); italic uses a 12° horizontal shear text matrix.
  Both are synthetic effects that require no separate bold/italic font file.

- **`PageHandle::add_text_styled(text, font, pos, size, color, bold, italic)`** — lower-level
  styled text method exposed on `PageHandle` for use outside `FlowDocument`.

- HTML inline styles in `render_html_to_pdf`: `<strong>`/`<b>` → bold, `<em>`/`<i>` → italic,
  `<span style="color: #RRGGBB">` → color (hex, 3-digit, and `rgb()` forms), `<a href>` → blue
  link color `[0, 0, 0.8]`.

- TTC (TrueType Collection) E2E tests — `tests/e2e_ttc.rs` (5 cases) using a synthetic 2-face
  TTC constructed at runtime from the existing NotoSansJP fixture.

- WASM smoke test — `tests/wasm_smoke.rs` with `#[wasm_bindgen_test]`, runnable via
  `wasm-pack test --node`.

- CI: `cargo semver-checks` job (default + all-features); `wasm-test` job (`wasm-pack test --node`).

- **`Document::set_encryption_aes256(user_password, owner_password)`** — AES-256-CBC write
  encryption (PDF 2.0 V5/R6). A fresh 32-byte key is generated per `save()` via
  `getrandom::fill()` (OS RNG — never falls back to a weaker source). Requires
  Acrobat X+ / Chrome / Firefox or any modern PDF reader. Use `set_encryption` (RC4-128) for
  maximum backward compatibility with older viewers. `getrandom = "0.4"` added as a direct
  dependency; WASM targets continue to use the `wasm_js` backend.

### Fixed

- **Nested `/Pages` tree inherited-attribute loss** (`remove_page`, `insert_blank_page`,
  `reorder_pages`) — when these methods re-parent pages directly to the root `/Pages` node, any
  attributes inherited from intermediate `/Pages` nodes (`/MediaBox`, `/CropBox`, `/Rotate`,
  `/Resources`, `/UserUnit`) were silently lost. The new `realize_page_inherited_attrs()` helper
  materializes those values directly onto each page dict before the `/Parent` reference is changed.
  Three regression tests added (`nested_pages_*`).

---

## [0.7.0] — 2026-06-03

### Added

- **`Document::set_encryption(user_password, owner_password)`** — encrypts the document at
  `save()` / `save_to_bytes()` / `save_to_writer()` time using 128-bit RC4 (PDF revision 3).
  Pass an empty `user_password` to allow anyone to open the file while still restricting editing
  to the owner password. The document `/ID` trailer entry required by encryption is generated
  automatically from system time + process ID.
  Returns `Error::InvalidInput` if called after `save()`.

- **`PageHandle::add_squiggly(rect, color)`** — wavy underline markup annotation.
  Completes all four PDF standard text-markup subtypes: Highlight, Underline, StrikeOut, Squiggly.

- **`PageHandle::media_box()`** / **`PageHandle::set_media_box(rect)`** — read (with parent-chain
  inheritance) and override the page's physical size (`/MediaBox`).
  
- **`PageHandle::crop_box()`** / **`PageHandle::set_crop_box(rect)`** — read/write the visible-area
  clip (`/CropBox`). Returns `None` when unset.
  
- **`PageHandle::trim_box()`** / **`PageHandle::set_trim_box(rect)`** — read/write the intended
  print area (`/TrimBox`). Returns `None` when unset.
  
- **`PageHandle::bleed_box()`** / **`PageHandle::set_bleed_box(rect)`** — read/write the bleed area
  for print production (`/BleedBox`). Returns `None` when unset.

All box methods use `[x, y, width, height]` in PDF points (bottom-left origin), consistent with
the rest of the harumi API.

---

## [0.6.0] — 2026-06-03

### Added

- **`Document::from_file_with_password(path, password)`** /
  **`Document::from_bytes_with_password(bytes, password)`** — load and decrypt password-protected
  PDFs. Both user and owner passwords are accepted; the document is fully decrypted in memory.

- **`Document::is_encrypted()`** — returns `true` if the PDF was encrypted when it was loaded.
  Remains `true` after a successful `from_*_with_password` call.

- **`Error::WrongPassword`** — dedicated error variant returned when the supplied password does not
  match the document's user or owner password.

- **`PageHandle::add_highlight(rect, color)`** — highlight markup annotation.
  Includes auto-generated `QuadPoints` (upper-left → upper-right → lower-left → lower-right order,
  matching Adobe Acrobat's convention).

- **`PageHandle::add_underline(rect, color)`** — underline markup annotation (with `QuadPoints`).

- **`PageHandle::add_strikeout(rect, color)`** — strikethrough markup annotation (with `QuadPoints`).

- **`PageHandle::add_sticky_note(point, contents)`** — Text (sticky-note) annotation.
  `contents` is encoded as UTF-16BE for full Unicode support. The icon appears at `[x, y]`
  in PDF points; default collapsed state (`/Open false`).

- **`Document::form_fields() -> Result<Vec<FormField>>`** — lists all interactive form fields.
  Recursively traverses the AcroForm `/Fields` tree; only leaf fields are returned with their
  full dotted name path, `FieldType`, and current string value.

- **`Document::fill_form(values: &[(&str, &str)]) -> Result<usize>`** — fills form fields by name.
  Text fields receive the string value directly. Checkbox / radio fields are set to
  `/Yes` for truthy strings (`"true"`, `"yes"`, `"on"`, `"1"`, case-insensitive) and `/Off` for
  everything else. Automatically sets `/NeedAppearances true` on the AcroForm dictionary so
  viewers regenerate the visual appearance. Returns the number of updated fields.
  Returns `Error::InvalidInput` if called after `save()`.

- **`FormField`** — public struct: `name: String`, `field_type: FieldType`, `value: String`.
  Marked `#[non_exhaustive]`.

- **`FieldType`** enum — `Text`, `Checkbox`, `Radio`, `Choice`, `Signature`, `Unknown`.

- **AGL table expanded 214 → ~330 entries** — adds Central European characters (Abreve/abreve,
  Cacute/cacute, Dcaron/dcaron, Nacute/nacute, Uring/uring, and ~60 more), the common ligatures
  `ff` / `ffi` / `ffl`, and lowercase `euro`. Improves `extract_text_runs` coverage for Polish,
  Czech, Hungarian, Turkish, and Romanian documents.

- **Identity-H GID fallback** in `extract_text_runs` — Type0 CID fonts that lack a `/ToUnicode`
  CMap entry are now decoded by treating the 2-byte character code directly as a Unicode scalar
  value (best-effort; correct for BMP characters encoded in Identity-H fonts).

### Fixed

- `from_file_with_password` / `from_bytes_with_password` — `lopdf::Error::IO` is now correctly
  mapped to `harumi::Error::Io` instead of `harumi::Error::Pdf`.

---

## [0.5.1] — 2026-05-28

### Changed

- **WASM demo** — replaced the simple stamp/OCR form with a full annotation editor:
  text placement, rectangle highlight, straight line, and freehand pen tools.
  PDF.js renders a live preview; annotations are applied via harumi's `draw` API
  and downloaded as a modified PDF.
- **WASM demo** — default Hack Regular font bundled; no font upload required.
- **CI** — fixed macOS-only `Geneva.ttf` test skipping on Linux runners.

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
  single-image scanned PDF page. Returns a `PageImage` with `format` (`Jpeg` or `Png`) and `bytes` bytes.
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
