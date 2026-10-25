# Documentation guide

Use the shortest document that answers the question:

| Need | Read |
|---|---|
| Install and understand the product boundary | [`../README.md`](../README.md) |
| Find an API or feature flag | [`API.md`](API.md) |
| Copy a working example | [`EXAMPLES.md`](EXAMPLES.md) |
| Scan the complete feature surface | [`FEATURES.md`](FEATURES.md) |
| Compare harumi with other PDF tools | [`PDF_ECOSYSTEM.md`](PDF_ECOSYSTEM.md) |
| Read release history | [`../CHANGELOG.md`](../CHANGELOG.md) |

The translated README files cover the same public positioning in Japanese,
Korean, and Simplified Chinese. They intentionally omit the full API matrix;
the API and examples documents are the single source of truth for code snippets.

## Scope and support boundaries

harumi is an existing-PDF extraction and write-back library. It is not an OCR
engine, PDF viewer, general SVG renderer, or pixel-perfect document converter.
Complex rotation, vertical writing, image-backed text, malformed PDFs, and
font-specific behavior require renderer-based visual review.

The normal build is pure Rust and WASM-compatible. Optional host-side renderer
checks and external OCR tools are verification or input workflows, not runtime
dependencies of the core library.
