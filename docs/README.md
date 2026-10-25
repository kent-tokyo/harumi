# Documentation guide

Use the shortest document that answers the question:

| Need | Read |
|---|---|
| Install and understand the product boundary | [`../README.md`](../README.md) |
| Find an API or feature flag | [`API.md`](API.md) |
| Copy a working example | [`EXAMPLES.md`](EXAMPLES.md) |
| Scan the complete feature surface | [`FEATURES.md`](FEATURES.md) |
| Check current scope and priorities | [`../ROADMAP.md`](../ROADMAP.md) |
| Compare harumi with other PDF tools | [`PDF_ECOSYSTEM.md`](PDF_ECOSYSTEM.md) |
| Read release history | [`../CHANGELOG.md`](../CHANGELOG.md) |

Translated READMEs cover the public positioning only. API signatures and code
snippets live in `API.md` and `EXAMPLES.md`.

## Scope and support boundaries

harumi extracts and writes back existing PDF content. It is not an OCR engine,
PDF viewer, general SVG renderer, or pixel-perfect converter. Review complex
rotation, vertical writing, image-backed text, and malformed PDFs with a
renderer.

The normal build is pure Rust and WASM-compatible. Optional host-side renderer
checks and external OCR tools are verification or input workflows, not runtime
dependencies of the core library.
