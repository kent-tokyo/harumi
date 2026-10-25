# PDF ecosystem comparison contract

This document defines the comparison boundary for harumi. Rendering, extraction,
new-document layout, PDF object editing, and existing-PDF write-back are different
jobs and must not be reduced to one ranking.

## Current position

harumi **v1.23.6** extracts positioned text from existing PDFs and writes translated,
replaced, or OCR-searchable CJK text back using Pure Rust. The default build has no
native PDF rendering runtime.

| Primary job | Useful reference | harumi's boundary |
|---|---|---|
| Viewer display and raster rendering | `pdfium-render` | Optional renderer/oracle; not a default dependency |
| New reports and typesetting | `printpdf` / `genpdf` | Flow/HTML provide tables, templates, and pagination |
| Direct PDF object editing | `lopdf` | Higher-level page, text, annotation, and save/reload APIs |
| Bulk extraction | `unpdf` / `pdf_oxide` | Position-aware extraction and JSON/Markdown handoff |
| Low-level PDF writing | `pdf-writer` | CJK fonts, ToUnicode maps, and existing-PDF editing |

## Comparison rules

Record these axes separately:

- **Preservation:** whether the original objects and page content survive
- **Extraction:** text, order, coordinates, CJK, and table recovery
- **Write-back:** replacement count, glyph coverage, coordinates, and diagnostics
- **Layout:** pagination, tables, fonts, and templates
- **Rendering:** output from a pinned renderer
- **Dependencies:** native runtimes, external tools, and WASM support
- **Throughput:** time and memory on the same input, host, and repetitions

A fixed corpus is a contract signal, not a general accuracy or speed claim. Raster
differences are diagnostic; they are not pixel-parity evidence across renderers.

## Verification assets

| Area | Contract |
|---|---|
| WASM dependencies | [`scripts/check-wasm-deps.sh`](../scripts/check-wasm-deps.sh) |
| PDF specification | [`docs/fixtures/pdf-spec-coverage.json`](fixtures/pdf-spec-coverage.json) |
| Bulk extraction | [`docs/fixtures/bulk-extraction.json`](fixtures/bulk-extraction.json) |
| Report generation | [`docs/fixtures/report-generation.json`](fixtures/report-generation.json) |
| Domain comparison | [`docs/fixtures/competitive-domains.json`](fixtures/competitive-domains.json) |
| Poppler rendering | [`scripts/check-poppler-render-fixture.sh`](../scripts/check-poppler-render-fixture.sh) |
| Pdfium probe | [`scripts/check-pdfium-render-fixture.sh`](../scripts/check-pdfium-render-fixture.sh) |

When updating a result, record the input, version, host, build profile, repetitions,
renderer, and whether memory was measured. The fixture and runner are the primary
source; prose summaries should not introduce stronger claims.

## Implemented contract

- CJK TrueType/TTC embedding, ToUnicode maps, and subsetting
- Existing-PDF extraction, overlays, replacement, and page operations
- Flow/HTML paragraphs, tables, templates, footnotes, and page breaks
- Basic SVG shapes, curves, ellipses, transforms, and colors
- Page-scoped extraction, JSON/Markdown handoff, and structure audits
- WASM dependency checks and post-save Poppler smoke tests

## Open boundaries

- There is no built-in PDF page renderer; use Pdfium or Poppler separately.
- Full Unicode line breaking, RTL, vertical writing, and HTML/CSS compatibility
  remain incomplete.
- Complex SVG paths, fill rules, and text transforms need input-specific review.
- Pixel-level rendering parity, a pinned Pdfium golden image, and same-corpus
  competitor measurements remain open.
- Peak RSS and real-document CJK write-back quality need larger measurements.

## Next evidence

1. Measure extraction time, p95, and peak RSS on 100, 300, and 1,000-page inputs.
2. Check text, coordinates, fonts, original objects, and render differences together
   on representative real PDFs.
3. Add fixed-image coverage for SVG subpaths and transforms.
4. Publish axis-level results for the same input across extractors, layout engines,
   and renderers.

Detailed history remains in `CHANGELOG.md`, tests, fixtures, and their reports.
