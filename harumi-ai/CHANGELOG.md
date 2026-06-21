# Changelog — harumi-ai

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
