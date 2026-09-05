# Changelog — harumi-ai

---

## [0.10.0] — 2026-09-05

### Added

- Rotation-aware overlay placement for common 90°/270° text lines.
- Multi-page OCR JSON validation and page-specific coordinate conversion.
- Safer blank-translation handling and expanded page-level layout diagnostics.
- Synchronized dependency requirement with `harumi` 1.20.

Complex vertical writing, mixed styles, and pixel-identical output remain best-effort
and should be visually reviewed.

---

## [0.6.0] — 2026-06-21

### Added

- **`LayoutRepairMode`** enum — post-translation layout repair strategy:
  `Off` (no repair), `GeometryOnly` (geometry diagnostics only),
  `GeometryThenVision` (geometry first, then vision for pages that still fail — **default**),
  `VisionAllPages` (rasterize and compare every page).

- **`RasterizeOptions`** struct — settings for Poppler-based page rasterization:
  `command` (binary name or path, default `"pdftoppm"`), `dpi` (default 144),
  `timeout_per_page` (default 30 s).

- **`VisionProvider`** trait — provider-agnostic async interface for vision-based layout
  repair.  One method: `repair_layout(&self, request: VisionRepairRequest) -> Result<Vec<LayoutCorrection>>`.

- **`VisionRepairRequest`** struct — input sent to a vision provider for one page:
  `page`, `source_png`, `translated_png`, `geometry_issues_json`, `target_lang`,
  `source_lang`.

- **`LayoutCorrection`** struct — one corrected translation returned by a repair pass:
  `page`, `id`, `text`, `reason`.  Serializable via `serde`.

- **`TranslateOptions::layout_repair_mode`** — `LayoutRepairMode` field, default
  `GeometryThenVision`.  When set to `Off`, the AI correction loop is skipped entirely.

- **`TranslateOptions::vision_provider`** — optional `Arc<dyn VisionProvider>`.  Required
  for `GeometryThenVision` and `VisionAllPages` modes to actually send images to a model.

- **`TranslateOptions::rasterize`** — `RasterizeOptions` for the Poppler `pdftoppm`
  rasterization step used by vision repair passes.

- **`TranslateOptionsBuilder::layout_repair_mode(mode)`** — builder setter for
  `layout_repair_mode`.

- **`TranslateOptionsBuilder::vision_provider(provider)`** — builder setter that wraps
  the provider in an `Arc`.

- **`TranslateOptionsBuilder::rasterize_options(options)`** — builder setter for
  `rasterize`.

- **`AnthropicTranslator implements VisionProvider`** (`anthropic` feature) — the
  Anthropic provider can now be used for vision-based page comparison and correction.
  Sends the source and translated PNG renderings with geometry diagnostics to Claude's
  vision API and returns `LayoutCorrection`s.  Requires the `base64` dependency
  (added to the `anthropic` feature gate).

- **Improved `layout_correction_prompt`** — prompt now distinguishes repair priorities
  (major collision / overflow / image overlap > accepted shrink), gives per-element-type
  guidance (table cells preserve numbers/codes, headings shorten wording, paragraphs
  remove redundancy), and explicitly names the issues-JSON parameter.

### Dependencies

- `base64 = "0.22"` added as an optional dependency, gated under the `anthropic` feature.
- `tokio` feature set expanded to include `process` and `time` (required by
  `rasterize_page_png` for async `pdftoppm` invocation with per-page timeout).

---

## [0.5.0] — 2026-06-21

### Added

- **Tc character-spacing optimization** — before scaling the font down, `apply_overlay`
  now tries absorbing minor text overflow (≤ 25 % wider than `avail_w`) by setting a
  negative `Tc` character-spacing value.  If the required compression is ≥ −1.0 pt/char,
  the original font size is preserved and only character spacing is adjusted — matching
  the technique used by Adobe Acrobat, DeepL Document, and PDFMathTranslate.
  Falls back to the existing `fit_font_size()` path for larger overflows.
  Requires harumi ≥ 1.15 (`add_text_styled_with_char_spacing`).

- **Image region avoidance** — `OverlayPage` now carries `image_bboxes` populated from
  `Document::page_image_bboxes()` (harumi 1.15.0).  The white cover rectangle is skipped
  for any line that overlaps an image XObject (logo, watermark, company header image),
  leaving the original visual content intact beneath the overlay.

### Fixed (post-v0.4.0 security/correctness pass)

- **Cache key cross-language poisoning** — translation cache keys are now namespaced with
  `target_lang` (`"{lang}\x00{text}"`), preventing a second `translate_pdf` call to a
  different language from returning a cached translation from the first call.
- **SDS comparison pattern end anchor** — `^[<>≤≥±]\s*\d` changed to
  `^[<>≤≥±]\s*[\d.,]+(\s*[\w%/°]+)?$`; sentences starting with `< 5%` are no longer
  silently skipped as skip-pattern matches.
- **Terminal injection via eprintln!** — raw PDF text logged by `auto_skip_math` is now
  sanitized (control characters replaced with U+FFFD) before output.
- **`text_is_primarily_math` false positives** — short-text branch (≤ 20 chars) now
  requires ≥ 2 math chars OR > 25 % math-char fraction; prevents "β-carotene content"
  (1 Greek char in 17) from being classified as math and silently dropped.
- **Skip-pattern lines covered by white rect** — lines matched by `skip_patterns` now set
  `is_skip = true`, so `apply_overlay` omits both the white cover rectangle and the text
  placement, leaving the original PDF content completely untouched.
- **Correction loop modifying verbatim-protected text** — skip-pattern resolved lines now
  store `""` (not original text), so the AI correction loop cannot shorten CAS numbers or
  chemical formulas.
- **AI-dropped block silent regression** — when the AI omits a block from its response,
  the merge now stores `""` (original left in place) and logs a warning to stderr instead
  of silently embedding untranslated source text.
- **InPlace fallback width inconsistency** — overlay-fallback path in `inplace.rs` now
  uses `region_usable_right` (consistent with `apply_overlay`).
- **Builder missing setters** — `TranslateOptionsBuilder::skip_header_footer(bool)` and
  `auto_skip_math(bool)` added.

---

## [0.4.0] — 2026-06-21

### Added

- **Region-aware overlay** — `extract_overlay_pages` now calls
  `harumi::extract_layout_regions()` per page, annotating every `OverlayLine`
  with the containing layout region's `usable_rect`.  The right edge of that
  rect (`region_usable_right`) replaces the previous histogram-based `col_right`
  heuristic as the available-width constraint in `apply_overlay`,
  `run_correction_loop`, and `compute_page_quality`.  The match requires both
  y-overlap and x-containment to prevent cross-column false matches.

- **Heading detection via region kind** — lines inside a
  `LayoutRegionKind::Heading` region are classified as headings even when the
  gap-above / bold heuristics do not fire.

- **`TranslateOptions::skip_header_footer`** (default `false`) — when `true`,
  lines whose layout-region role is `HeaderFooter` are left completely untouched:
  no white-rectangle coverage and no AI translation.

- **`TranslateOptions::auto_skip_math`** (default `false`) — automatically
  skips text lines that are primarily math/formula characters (Greek letters
  U+0370–U+03FF, Mathematical Operators U+2200–U+22FF, Mathematical Alphanumeric
  Symbols U+1D400–U+1D7FF, superscript/subscript digits).  Short tokens (≤ 20
  chars) are flagged on any math character; longer text requires math chars to
  be the majority while prose alphabetics are sparse, so sentences like "The
  coefficient α represents…" are not dropped.  Skipped lines are logged to
  stderr.

- **`OverlayLine::is_skip`** (internal) — when `true`, the line is excluded
  from the AI batch, its white-rectangle cover is not drawn, and no translated
  text is placed.  Set by `skip_header_footer` and `auto_skip_math` processing
  in `extract_and_translate`.

---

## [0.3.0] — 2026-06-21

### Added

- **Translation cache** (`TranslationCache`, `TranslateOptions::cache`) — an
  in-memory `Arc<Mutex<TranslationCache>>` that deduplicates repeated phrases
  within a document (or across multiple `translate_pdf` calls when the same
  `Arc` is reused).  Cache hits are resolved before the AI batch, keeping the
  lock held only for map lookups — never across an `await`.  Hit/miss stats are
  logged to stderr and available via `TranslationCache::hits()` /
  `TranslationCache::misses()` / `TranslationCache::hit_rate()`.

- **Skip patterns** (`TranslateOptions::skip_patterns`,
  `TranslateOptions::with_sds_patterns()`) — regex patterns for text that must
  not be translated (passed through verbatim).  Built-in SDS defaults protect
  chemical formulas (H₂SO₄), CAS numbers (7664-93-9), UN numbers (UN1830),
  numeric value+unit strings, and comparison expressions.  Invalid regex
  patterns are silently ignored.

- **Bilingual PDF mode** (`TranslationMode::Bilingual`) — each original page
  is immediately followed by its translated version.  Output page order:
  `[orig_1, trans_1, orig_2, trans_2, …, orig_n, trans_n]`.  Useful for
  side-by-side review and QC workflows.

- **`TranslateOptionsBuilder::with_cache()`** and
  **`TranslateOptionsBuilder::add_skip_pattern()`** — builder methods for the
  new options.

### Internals

- `extract_and_translate` now builds a `resolved` side-map
  `(page_num, line_idx) → text` for skip/cache hits before the AI batch, and
  merges by walking `0..line_count` after the AI call — preserving positional
  alignment with `overlay_page.lines` that the correction loop and
  `apply_overlay` depend on.

---

## [0.2.1] — 2026-06-21

### Added

- **`TranslateQuality::fallback_reason: Option<String>`** — records why
  `TranslationMode::Auto` switched from the initially selected mode during the
  cascade.  `None` when the first-chosen mode was used throughout.

- **Auto mode quality cascade** — `TranslationMode::Auto` with a non-`BestEffort`
  profile now attempts a multi-stage cascade driven by actual layout quality rather
  than a static heuristic:
  - *Stage 1* — InPlace is attempted first. If the overlay-fallback rate exceeds 30 %
    (too many lines that `replace_text` could not match), cascade to Overlay.
  - *Stage 2 (PreserveLayout / Strict)* — Overlay result is returned (with
    `fallback_reason` set).  `Strict` returns `Error::QualityGateFailed` if the gate
    still fails.
  - *Stage 2 → Stage 3 (Readable only)* — if the Overlay quality gate also fails,
    cascade to NewDocument as a last resort.

- **`TranslationMode` derives `Debug`, `Clone`, `PartialEq`, `Eq`** — enables
  comparisons and debug output in application code.

- **`InPlaceStats`** (internal) — exposes `replaced`, `fallback`, `total_lines` and
  `fallback_rate()` from an InPlace translation pass so the Auto cascade can
  evaluate quality before deciding whether to cascade.

---

## [0.2.0] — 2026-06-21

### Breaking changes

- **`translate_pdf` return type changed** from `Result<Vec<u8>>` to
  `Result<TranslateOutput>`.  Use `output.pdf_bytes` for the raw PDF bytes.

### Added

- **`TranslateOutput`** — structured result wrapping `pdf_bytes`, `quality`, and
  optional `debug` artifacts.  Callers now get layout diagnostics alongside the
  translated PDF without a separate call.

- **`TranslateQuality`** / **`PageQualityReport`** — per-page `PageFitSummary`
  (collision count, overflow count, shrunk count, worst overlap area/rect) plus
  an `overall: QualityResult`, `correction_rounds`, and `mode_used`.

- **`QualityProfile`** enum (`BestEffort` / `PreserveLayout` / `Readable` /
  `Strict`) + **`QualityGate`** struct — configurable layout quality thresholds.
  Set `TranslateOptions::profile` to gate the final PDF.  `Strict` returns
  `Error::QualityGateFailed` when any threshold is exceeded; all other profiles
  still return the PDF with violations recorded in `quality.overall`.

- **`TranslationMode::Auto`** — detects the best mode at runtime by inspecting
  the fraction of `TableCell` layout regions on the first page.  Dense form/SDS
  PDFs (> 60 % table cells) route to `Overlay`; others try `InPlace`.  The
  resolved mode is reported in `TranslateQuality::mode_used`.

- **Multi-pass AI correction loop** in Overlay mode — up to
  `TranslateOptions::max_correction_rounds` (default 2) rounds, each identifying
  lines that overflow **or** are involved in `Moderate`/`Major` harumi collisions
  and asking the AI to shorten them.  Previously only overflow lines were
  corrected in a single pass.

- **`DebugOptions`** / **`DebugArtifacts`** — opt-in debug artifacts:
  `layout_report_json`, `collision_report_json`, `debug_overlay_pdf`
  (via harumi's `add_fit_debug_overlay`), and `correction_history`.  All default
  to `false`.

- **`Error::QualityGateFailed(Vec<QualityViolation>)`** — returned when
  `QualityProfile::Strict` detects layout violations.

- **`is_likely_mojibake`** utility + `find_bad_blocks` helper in `repair.rs` —
  detection logic for garbled/empty translations (wired into the translation loop
  in a future release).

### Changed

- Overlay mode's `evaluate_layout()` has been replaced by `run_correction_loop()`
  which also handles collision-involved lines, not just overflow lines.
- `TranslateOptions` gains `profile: QualityProfile`, `max_correction_rounds:
  usize`, and `debug: DebugOptions` fields (all with sensible defaults).
- harumi dependency bumped to `≥ 1.14` for `CollisionSeverity` and
  `collision_severity()`.

---

## [0.1.1] — 2026-06-xx

Initial crates.io release.
