# harumi roadmap

Current release: **v1.23.8**. This file is the short public roadmap; release
history lives in [`CHANGELOG.md`](CHANGELOG.md), and implementation contracts
live in the tests and fixture documents.

## Current position

harumi focuses on position-aware CJK text extraction and write-back for existing
PDFs, plus Pure Rust/WASM document automation. It does not provide a general
PDF viewer or pixel-identical rendering.

## Next priorities

1. **Flow performance** — profile 100/1,000-page reports and reduce repeated font
   measurement and page-tree work without changing output.
2. **CJK write-back evidence** — test varied real PDFs for glyph coverage,
   replacement counts, resource preservation, extraction, and rendered output.
3. **SVG visual coverage** — add multi-subpath, transform, baseline, and marker
   fixtures with raster comparisons.
4. **Unicode layout** — extend line breaking, vertical writing, and bidi support
   only with explicit fixtures and bounded claims.
5. **Renderer compatibility** — keep Poppler smoke checks and add pinned Pdfium,
   Chrome, or Acrobat evidence when runtimes are available.

## Evidence policy

Performance, extraction accuracy, visual fidelity, and library fit are reported
separately. A small fixed corpus is a correctness signal, not a general
accuracy claim. See [`docs/PDF_ECOSYSTEM.md`](docs/PDF_ECOSYSTEM.md) for the
comparison contract and evidence status.

## Completed foundations

- CJK TrueType/TTC embedding, ToUnicode maps, and font subsetting
- Existing-PDF extraction, overlay, replacement, page operations, and repairs
- Flow/HTML pagination, tables, templates, footnotes, and layout diagnostics
- Bulk extraction, JSON/Markdown handoff, structure audits, and WASM checks
