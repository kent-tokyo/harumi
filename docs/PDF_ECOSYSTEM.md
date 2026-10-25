# PDF ecosystem comparison contract

This document defines role boundaries. It is not a claim that harumi replaces
every PDF library. Rendering, new-document generation, object editing,
extraction, and existing-PDF write-back are different jobs.

## Version snapshot

Snapshot: **2026-10-25**. Refresh versions and rerun fixtures before using this
as a benchmark.

| Role | Reference | Boundary |
|---|---|---|
| Rendering/viewer editing | `pdfium-render 0.9.3` | Host Pdfium runtime and native deployment |
| New reports | `printpdf 0.12.6`, `genpdf 0.2.0` | Document model and pagination |
| Low-level object editing | `lopdf 0.42.0` | PDF object graph and streams |
| Bulk extraction | `unpdf 0.17.0`, `pdf_oxide 0.3.77` | Text/Markdown extraction and broader lifecycle |
| Low-level PDF writing | `pdf-writer 0.15.0` | Typed construction of new PDF objects |
| Existing-PDF CJK write-back | `harumi 1.23.0` | Extraction, CJK fonts, overlay, replacement, layout diagnostics |

Reference documentation: [pdfium-render](https://docs.rs/crate/pdfium-render/0.9.3),
[printpdf](https://docs.rs/crate/printpdf/0.12.6),
[genpdf](https://docs.rs/crate/genpdf/0.2.0),
[lopdf](https://docs.rs/crate/lopdf/0.42.0),
[unpdf](https://docs.rs/crate/unpdf/0.17.0),
[pdf_oxide](https://docs.rs/crate/pdf_oxide/0.3.77), and
[pdf-writer](https://docs.rs/crate/pdf-writer/0.15.0).

## Evaluation axes

Keep these claims separate:

| Axis | Required evidence |
|---|---|
| Existing-PDF fidelity | Object inspection and rendered output |
| Extraction | Fixed corpus with text, order, coordinate, CJK, table, and image checks |
| Write-back | Match count, glyph coverage, and page quality report |
| New-document layout | Fixed report fixture and page/layout assertions |
| Rendering | Fixed renderer, DPI, and image-diff policy |
| Dependency boundary | `cargo tree`, target builds, and native-link audit |
| WASM | Target build and runtime smoke test |
| Throughput | Same corpus, warm-up policy, memory, and repeated runs |

Workflow fit, visual quality, performance, and replacement claims must not be
collapsed into one score.

## Responsibility map

- Use `pdfium-render` for display and rasterization; it is an optional
  verification layer, not a default harumi dependency.
- Use `printpdf`/`genpdf` for new report documents. harumi Flow/HTML is a
  separate pure-Rust path, not an existing-PDF renderer.
- Use `lopdf` or `pdf-writer` when direct object construction is the goal.
- Use `unpdf` or `pdf_oxide` when bulk extraction is primary; prefer an adapter
  over adding either as a default harumi dependency.
- Use harumi when existing PDF content must remain while CJK text, annotations,
  or layout-aware replacements are added.

## Repository checks

- [`check-wasm-deps.sh`](../scripts/check-wasm-deps.sh) checks the normal WASM
  dependency boundary.
- [`tools/pdfium-render-check`](../tools/pdfium-render-check/Cargo.toml) is an
  explicit host-side renderer probe with a pinned Pdfium API.
- [`check-pdfium-render-fixture.sh`](../scripts/check-pdfium-render-fixture.sh)
  reproduces the renderer probe.

These checks are bounded evidence, not universal proof of compatibility with
every PDF producer or viewer.
