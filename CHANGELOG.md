# Changelog

Notable user-facing changes for `harumi` and `harumi-ai`.

This file is intentionally concise. Detailed implementation history is
available in the [Git tags](https://github.com/kent-tokyo/harumi/tags) and pull
requests. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

No changes yet.

- Pin `aes` to 0.9.2 so fresh dependency resolution remains compatible with the
  declared Rust 1.88 MSRV.

## 1.23.1 — 2026-10-25

- Fix SVG text anchoring, baseline positioning, short hex colors, and marker definitions.
- Update `lopdf` to 0.46 and remove its transitive `ttf-parser` dependency.
- Fix the report-generation CI fixture for the updated `lopdf` API.

## 1.23.0 — 2026-10-25

- Add optional pure-Rust `svg_to_pdf` conversion for the supported chemical
  drawing subset, including vector shapes, text, transforms, opacity, dashes,
  and arrowheads.
- Preserve inherited page `/Resources` during repair instead of replacing a
  valid parent resource dictionary with an empty one.
- Replace direct `ttf-parser` usage with maintained `skrifa` font metadata APIs;
  `lopdf` may still retain `ttf-parser` transitively.
- Add validated dashed line and polyline drawing APIs using native PDF dash
  operators.
- Refresh CI artifact actions and audited dependency versions.
- Refactor text extraction, replacement, layout-region, outline, Flow-table,
  and finalization internals without changing the public API.
- Add the concise project roadmap and align documentation with the tested
  feature boundaries.

## 1.22.1 — 2026-09-07

- Added measured paragraph and table keep-together behavior.
- Added HTML `break-before`/`break-after` page aliases and paragraph controls.
- Added footnotes, section templates, page decorations, header/footer geometry,
  nested tables, row-span pagination, and mixed-font fallback support.
- Added layout regions, collision classification, quality reports, vector-rule
  extraction, and translation-oriented planning APIs.
- Improved CJK extraction, vertical metrics, opacity tracking, bookmarks,
  annotations, forms, encryption, attachments, and page-box operations.
- Added bounded feature verification, WASM dependency checks, renderer probes,
  and reproducible comparison fixtures.

## 1.22.0 — 2026-09-06

- Expanded Flow tables, nested blocks, section geometry, and report fixtures.
- Added HTML pagination and layout diagnostics for complex reports.

## 1.21.0 — 2026-09-06

- Added PDF ecosystem comparison fixtures and adapter-boundary checks.
- Improved release, CI, WASM, and renderer verification.

## 1.20.0 — 2026-09-05

- Added document layout quality summaries and translation planning support.
- Improved table measurement and page-boundary decisions.

## 1.19.0–1.17.0 — 2026-06-23 to 2026-06-26

- Added layout regions, fit policies, collision diagnostics, and translation
  overlay quality gates.
- Added OCR correction and scanned-PDF translation workflows in `harumi-ai`.

## 1.16.0–1.9.0 — 2026-06-15 to 2026-06-21

- Added FlowDocument pagination, HTML-to-PDF, text replacement, extraction
  chunks, page operations, annotations, forms, encryption, and image support.
- Added CJK font subsetting, ToUnicode mapping, font fallback, and layout APIs.

## 1.8.0–0.1.0 — 2026-04-xx to 2026-06-14

- Established the pure-Rust PDF writer, CJK font embedding, text overlay,
  extraction, replacement, merge/split, WASM, and MCP foundations.

## Historical detail

For a release-specific change that is not summarized above, inspect the matching
tag or commit history. Do not use this abbreviated changelog as a benchmark or
as a complete API compatibility record; use the public API docs and semver
checks for that purpose.
