# PDF ecosystem comparison contract

This document defines the comparison boundary for harumi. It is not a claim that
harumi replaces every PDF library. Rendering, new-document generation, direct
object editing, extraction, and layout-aware write-back are different jobs.

## Version snapshot

The versions below are a reproducible documentation snapshot taken on 2026-09-06.
Update the snapshot date and rerun the comparison before using it as a benchmark.

| Role | Project | Pinned reference | Default boundary for this comparison |
|---|---|---:|---|
| Screen display, rendering, editing | `pdfium-render` | 0.9.3 / Pdfium API 7881 | host-side Pdfium wrapper; native Pdfium runtime is part of the deployment boundary |
| New forms and reports | `printpdf` | 0.12.6 | document generation and serialization |
| High-level report generation | `genpdf` | 0.2.0 | document tree, pagination, and text alignment; built on the printpdf ecosystem |
| Direct PDF object manipulation | `lopdf` | 0.42.0 in this repository | low-level object graph and content-stream access |
| Bulk extraction and Markdown | `unpdf` | 0.17.0 | structured extraction, Markdown/text/JSON, and parallel page processing |
| Bulk extraction and broader PDF lifecycle | `pdf_oxide` | 0.3.77 | text/Markdown extraction plus broader PDF operations and bindings |
| Low-dependency PDF writing | `pdf-writer` | 0.15.0 | step-by-step creation of new PDF objects |
| Existing-PDF CJK write-back | `harumi` | 1.22.0 in this repository | extraction, CJK font embedding, overlay, replacement, page operations, and quality diagnostics |

The external references are the package documentation pages:

- [`pdfium-render 0.9.3`](https://docs.rs/crate/pdfium-render/0.9.3)
- [`printpdf 0.12.6`](https://docs.rs/crate/printpdf/0.12.6)
- [`genpdf 0.2.0`](https://docs.rs/crate/genpdf/0.2.0)
- [`lopdf 0.42.0`](https://docs.rs/crate/lopdf/0.42.0)
- [`unpdf 0.17.0`](https://docs.rs/crate/unpdf/0.17.0)
- [`pdf_oxide 0.3.77`](https://docs.rs/crate/pdf_oxide/0.3.77)
- [`pdf-writer 0.15.0`](https://docs.rs/crate/pdf-writer/0.15.0)

## Separate evaluation axes

Do not collapse these into one score:

| Axis | Question | Evidence required |
|---|---|---|
| Existing-PDF fidelity | Does the original object graph/content survive the operation? | byte/object inspection plus rendered output |
| Extraction | Are text, order, coordinates, CJK, tables, and images recovered? | fixed corpus with missing-text and coordinate checks |
| Write-back | Can text be replaced or overlaid at a target region? | replacement count, glyph coverage, and page-level quality report |
| New-document layout | Can a report paginate, style, and embed fonts predictably? | fixed report fixture and page-count/layout assertions |
| Rendering | Can a page be displayed and rasterized consistently? | fixed renderer, DPI, and image diff policy |
| Dependency boundary | Does the build require C/C++, a native runtime, or external tools? | `cargo tree`, target build, and native-link audit |
| WASM | Does the declared feature set compile and run on the target? | `wasm32-unknown-unknown` build and browser smoke test |
| Throughput | How does extraction scale across pages and files? | same corpus, warm-up policy, peak memory, and repeated runs |

Performance, quality, adoption, and replacement claims must remain separate. A
library may be a better fit for one axis without being a replacement for another.

## Responsibility map

- Use `pdfium-render` when screen rendering or viewer-like editing is the primary
  requirement. It is an optional verification/rendering layer for harumi, not a
  default dependency.
- Use `printpdf` or `genpdf` when the input is a document model and the goal is a
  new report. harumi's `FlowDocument` and HTML path are compared here, but this is
  distinct from repairing an existing PDF.
- Use `lopdf` when direct access to PDF objects and content streams is required.
  harumi currently uses it internally and exposes a higher-level API above it.
- Use `unpdf` or `pdf_oxide` when bulk extraction or Markdown conversion is the
  primary workload. An interchange adapter is preferable to adding either as a
  default harumi dependency.
- Use `pdf-writer` when a small, typed, low-level writer for new PDFs is the goal.
  harumi's differentiator is existing-PDF read/write behavior, CJK subsetting,
  and position-aware diagnostics.

The current repository check is [`scripts/check-wasm-deps.sh`](../scripts/check-wasm-deps.sh).
It rejects known native-runtime packages from the workspace WASM graph. This is
an explicit boundary check, not a universal proof about arbitrary build scripts;
new optional native dependencies must be added to the reviewed list.

The optional host-side renderer probe is
[`tools/pdfium-render-check`](../tools/pdfium-render-check/Cargo.toml). It pins
`pdfium-render` to `0.9.3`, requires an explicit `PDFIUM_LIBRARY_PATH`, and writes
one selected page to PNG. The runner rejects an unpinned or missing runtime before
starting Cargo. It is deliberately outside the main workspace, so using the probe
does not add Pdfium or a C++ runtime to harumi's normal dependency graph.

The reproducible runner is [`scripts/check-pdfium-render-fixture.sh`](../scripts/check-pdfium-render-fixture.sh),
with its input and target-width contract in
[`docs/fixtures/pdfium-render.json`](fixtures/pdfium-render.json). It emits an
artifact report with page count, page dimensions, raster dimensions, and SHA-256.
The golden image is intentionally unset until a Pdfium runtime is pinned for the
host CI.

The renderer-independent smoke contract is
[`docs/fixtures/render-compatibility.json`](fixtures/render-compatibility.json).
[`scripts/check-poppler-render-fixture.sh`](../scripts/check-poppler-render-fixture.sh)
uses a fixed DPI and checks page count, non-empty PNG output, and the PNG signature.
It also writes a renderer artifact report containing PDF page size, raster dimensions,
and per-page SHA-256. This is an execution contract, not pixel parity: Poppler output
must not be used as a Pdfium, Chrome, or Acrobat golden. Phase 48 extends this
contract with the same input PDF and renderer-specific artifacts once those runtimes
are pinned.

Reports can be compared with
[`scripts/compare-render-artifacts.py`](../scripts/compare-render-artifacts.py).
The comparison records metadata mismatches, raster-dimension differences, identical
SHA-256 values, and pixel differences. A pixel difference is deliberately diagnostic
only; it is not sufficient to attribute a defect to harumi or to a renderer.

The new-document comparison fixture is
[`docs/fixtures/report-generation.json`](fixtures/report-generation.json). The
harumi side is covered by `tests/flow.rs::report_generation_fixture_contract`:
it fixes a CJK heading, four-row table, explicit page break, extraction markers,
and embedded-font evidence. The probe exercises both `harumi-flow` and
`harumi-html`, while `printpdf` draws an explicit grid and `genpdf` uses its
table layout element. These are separate comparison artifacts; passing the
contract does not claim pixel parity or replacement of any generator.

The comparison probe is [`tools/report-generation-check`](../tools/report-generation-check).
With the pinned versions, all four backends generated a two-page PDF and Poppler
`pdftotext` recovered the Japanese markers. harumi initially missed those
markers because `/DescendantFonts` was serialized in both direct-array and
indirect-reference forms; extraction now accepts both forms and the probe passes.
The same probe also checks page-boundary overflow, writes a `harumi-writeback`
marker with harumi, saves the PDF, and re-extracts the marker successfully.
The full four-backend run, including fixed-DPI Poppler artifacts, is available
through [`scripts/check-report-generation-fixture.sh`](../scripts/check-report-generation-fixture.sh).

## New-document typesetting roadmap

The v2 report-generation fixture is a smoke contract, not evidence that harumi already exceeds
`printpdf` or `genpdf` for general composition. The next track targets measurable
quality in this order:

1. Freeze a larger paragraph/table fixture with overflow, marker order, cell
   structure, repeated headers, raster dimensions, time, and peak-memory metrics.
2. Build one measured paragraph model for Flow and HTML, including Unicode/CJK
   line breaking, mixed-font fallback, keep constraints, and stronger widow/orphan
   handling.
3. Add deterministic table sizing, spans, nested blocks, repeated headers, and
   continuation borders.
4. Add page-template reservations, section changes, footnotes, and TOC anchors.
5. Map supported HTML/CSS semantics onto the shared model and rerun all four
   backends on every compatibility change.

Competitor comparisons remain axis-specific: extraction and overflow correctness,
table structure, page geometry, renderer artifacts, and performance are reported
separately. A pixel difference is diagnostic and is never treated as proof that
one backend is generally superior. The v1.22.0 release must pass the expanded
paragraph/table fixtures and retain Poppler artifacts; Pdfium, Chrome, and Acrobat
remain optional external-runtime evidence gates.

Phase 45B freezes the bulk-extraction corpus contract in
[`docs/fixtures/bulk-extraction.json`](fixtures/bulk-extraction.json). It
contains five input classes: CJK digital text, one-glyph-per-`Tj` fragments,
two-column/table layout, a scanned page with OCR JSON, and the generated-report
fixture. The adapter boundary is intentionally interchange-based: bulk tools
should emit JSON/Markdown/coordinate records for downstream consumers rather
than become default harumi dependencies. The pinned adapter measurements are
recorded below as failure-class observations, not as a single replacement
score.

The harumi baseline runner is [`tools/bulk-extraction-check`](../tools/bulk-extraction-check).
It generates the five corpus inputs, writes one JSON report with per-input
marker recall, coordinate coverage, Markdown block count, image/OCR counts, and
elapsed time, and leaves the PDFs available for Poppler or another extractor.
The current baseline is 100% marker recall and coordinate coverage for all five
inputs; this is a correctness/contract baseline, not a throughput benchmark.

Phase 49 freezes the first high-impact PDF specification corpus, introduced in v1.21.0, in
[`docs/fixtures/pdf-spec-coverage.json`](fixtures/pdf-spec-coverage.json). It
separates page-tree inheritance, Resources/Contents/Form XObjects, font/CMap,
and image filters. The repeatable unit/save-reload runner is
[`scripts/check-pdf-spec-coverage.sh`](../scripts/check-pdf-spec-coverage.sh).
It does not mark a case fully supported by unit tests alone: the contract still
requires an external renderer check for each case.
The corpus generator and Poppler renderer runner are
[`tools/pdf-spec-coverage-check`](../tools/pdf-spec-coverage-check) and
[`scripts/check-pdf-spec-coverage-render.sh`](../scripts/check-pdf-spec-coverage-render.sh).
They generate five one-page PDFs, including an `/Identity-V` vertical-metrics case,
and save one renderer artifact report per case.

The external adapter is [`tools/bulk-extraction-compare`](../tools/bulk-extraction-compare).
It pins `unpdf 0.17.0` and `pdf_oxide 0.3.77` outside the main workspace and
emits the same per-input JSON shape. On the current five-input corpus, harumi
was the reference baseline at 100% marker recall and coordinate coverage.
`unpdf` recovered the other CJK/table markers, but returned no text-coordinate
records in its JSON model and did not recover the deliberately fragmented
one-glyph marker. `pdf_oxide` produced valid line coordinates and recovered the
other markers, while the same fragmented marker remained unrecovered. These
are failure-class observations, not a combined quality or replacement score.
The adapter accepts a parallelism argument and records wall-clock time plus
process peak RSS; the current five-file p1/p4 runs are reproducibility evidence,
not a production-scale throughput benchmark.

## Evidence status and next step

- Done: pinned matrix, evaluation axes, shared five-input corpus, and
  dependency/WASM boundary checks.
- Done: harumi, `unpdf`, and `pdf_oxide` bulk-extraction adapters with separate
  recall, coordinates, Markdown, timing, memory, and failure-class output.
- Open: configure the repository Pdfium URL/SHA-256 variables and add a golden
  image; this remains optional because Pdfium is not a default harumi dependency.
