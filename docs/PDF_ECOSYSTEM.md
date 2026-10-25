# PDF ecosystem comparison contract

This document defines role boundaries. It is not a claim that harumi replaces
every PDF library. Rendering, new-document generation, object editing,
extraction, and existing-PDF write-back are different jobs.

## Version snapshot

Snapshot: **2026-10-25**. This is a role guide, not a ranking. Pin versions and
rerun fixtures before making benchmark claims.

| Role | Reference | Boundary |
|---|---|---|
| Rendering/viewer editing | `pdfium-render` | Host Pdfium runtime and native deployment |
| New reports | `printpdf`, `genpdf` | Document model and pagination |
| Low-level object editing | `lopdf` | PDF object graph and streams |
| Bulk extraction | `unpdf`, `pdf_oxide` | Text/Markdown extraction and broader lifecycle |
| Low-level PDF writing | `pdf-writer 0.15.0` | Typed construction of new PDF objects |
| Existing-PDF CJK write-back | `harumi 1.23.4` | Extraction, CJK fonts, overlay, replacement, layout diagnostics |

Use the current releases when measuring. Version-pinned results belong in a
fixture report, not in this role guide.

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

These checks are bounded evidence, not universal proof of compatibility.
