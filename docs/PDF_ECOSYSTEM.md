# PDF ecosystem comparison contract

This document defines the comparison boundary for harumi. It is not a claim that
harumi replaces every PDF library. Rendering, new-document generation, direct
object editing, extraction, and layout-aware write-back are different jobs.

## Version snapshot

The external versions below are a reproducible snapshot from 2026-10-25.
This is a role comparison, not a same-condition benchmark.

| Role | Project | Pinned reference | Default boundary for this comparison |
|---|---|---:|---|
| Screen display, rendering, editing | `pdfium-render` | 0.9.4 / Pdfium API 7881 | host-side Pdfium wrapper; native Pdfium runtime is part of the deployment boundary |
| New forms and reports | `printpdf` | 0.12.8 | document generation and serialization |
| High-level report generation | `genpdf` | 0.2.0 | document tree, pagination, and text alignment; built on the printpdf ecosystem |
| Direct PDF object manipulation | `lopdf` | 0.46.0 in this repository | low-level object graph and content-stream access |
| Bulk extraction and Markdown | `unpdf` | 0.42.0 | structured extraction, Markdown/text/JSON, and parallel page processing |
| Bulk extraction and broader PDF lifecycle | `pdf_oxide` | 0.3.78 | text/Markdown extraction plus broader PDF operations and bindings |
| Low-dependency PDF writing | `pdf-writer` | 0.15.0 | step-by-step creation of new PDF objects |
| Existing-PDF CJK write-back | `harumi` | 1.23.7 in this repository | extraction, CJK font embedding, overlay, replacement, page operations, and quality diagnostics |

The external references are the package documentation pages:

- [`pdfium-render 0.9.4`](https://docs.rs/crate/pdfium-render/0.9.4)
- [`printpdf 0.12.8`](https://docs.rs/crate/printpdf/0.12.8)
- [`genpdf 0.2.0`](https://docs.rs/crate/genpdf/0.2.0)
- [`lopdf 0.46.0`](https://docs.rs/crate/lopdf/0.46.0)
- [`unpdf 0.42.0`](https://docs.rs/crate/unpdf/0.42.0)
- [`pdf_oxide 0.3.78`](https://docs.rs/crate/pdf_oxide/0.3.78)
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

## Latest tagged evaluation

The v1.23.1 review used the same Mac and debug report-generation script for all
three harumi tags. It included three warm-up pages, then generation, save,
reload, and page-count verification. These are indicative results, not a
cross-library benchmark.

| Tag | 100 pages (median, 3 runs) | 1,000 pages (1 run) |
|---|---:|---:|
| v1.23.1 | 379 ms | 18.87 s |
| v1.23.0 | 382 ms | 19.32 s |
| v1.22.1 | 202 ms | 17.19 s |

The v1.23.1 fixed corpus recovered 14/14 target markers with 100% coordinate
coverage. The corpus is small, and no peak-memory or same-condition competitor
measurement was recorded. SVG fixes cover text anchoring, baseline placement,
short colors, transforms, and markers; multi-subpath rendering and raster-image
comparison remain open.

The score used in the review was 67.5/100 for v1.23.1, compared with 67.3 for
v1.23.0 and 66.7 for v1.22.1. It is a weighted fit score, not Elo, accuracy,
or a speed rating. The strongest open work is Flow performance, real-PDF CJK
write-back evidence, and SVG visual regression coverage.

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
`pdfium-render` to `0.9.3` in the reproducibility fixture, while the current
comparison reference above is 0.9.4. It requires an explicit
`PDFIUM_LIBRARY_PATH` and writes one selected page to PNG. The runner rejects an
unpinned or missing runtime before starting Cargo. It is deliberately outside the
main workspace, so using the probe does not add Pdfium or a C++ runtime to
harumi's normal dependency graph.

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

## Domain-specific competitive roadmap

The next roadmap is split by the actual job users hire a PDF library to do. It does
not assume that a renderer, a report generator, an object editor, an extraction
engine, and a minimal writer should have the same API or the same winning metric.
Phase 52 in [`tasks/todo.md`](../tasks/todo.md) defines the implementation order and
release gates. The machine-readable common contract is
[`docs/fixtures/competitive-domains.json`](fixtures/competitive-domains.json),
validated by [`scripts/check-competitive-domain-manifest.sh`](../scripts/check-competitive-domain-manifest.sh).
Each domain also has a contract fixture under [`docs/fixtures`](fixtures), so a
missing operation or runtime boundary is visible before an external runner is used.

Phase 52 now treats functional coverage as an explicit goal: native coverage for
Pure Rust/WASM workflows, adapter coverage for optional native renderers, and
interchange coverage for bulk readers and AI pipelines. This is coverage of common
user jobs, not an attempt to clone every competitor API.

| Domain | Reference role | harumi strategy | Evidence gate |
|---|---|---|---|
| PDF rendering and viewer-like editing | `pdfium-render` | Keep Pdfium optional; strengthen CJK-safe write-back, save-reload, and quality diagnostics | Pinned Pdfium plus extraction and render artifacts |
| New reports and complex typesetting | `genpdf` / `printpdf` | Extend Flow/HTML for measured CJK/Latin pagination, tables, templates, and anchors | Same semantic fixture, structure checks, and fixed renderer |
| Direct PDF structure operations | `lopdf` | Expose safe high-level mutations with object-graph preservation evidence | Direct/indirect object-graph round trips and warnings |
| Bulk extraction and Markdown/JSON | `unpdf` / `pdf_oxide` | Provide bounded, schema-versioned extraction handoff with coordinates and warnings | Fixed multi-page corpus, recall/order/coordinate/RSS artifacts |
| Lightweight Pure Rust writer | `pdf-writer` | Combine minimal dependency boundary with CJK fonts, ToUnicode, and deterministic output | Native/WASM matrix, artifact size, extraction, reload, and validity |

"Win" is recorded per axis and per fixture. A result in one domain must not be
presented as a general ranking of PDF libraries, and Poppler output must not be
substituted for Pdfium, Chrome, or Acrobat evidence.

The Phase 52C contract currently enumerates CJK/Latin paragraphs, mixed styles,
spans, nested tables, repeated headers, template variants, page footnotes, stable
TOC anchors, and modern HTML break/keep semantics. Its acceptance policy requires
zero unexpected overflow and stable marker order; raster differences remain
diagnostic only.

## New-document typesetting roadmap

The report-generation fixture is a smoke contract, not proof that harumi
outperforms `printpdf` or `genpdf`. The next checks are:

1. Freeze paragraph/table fixtures with overflow, marker order, structure,
   raster dimensions, time, and memory metrics.
2. Unify measured Flow/HTML paragraphs with Unicode breaking, font fallback,
   keep constraints, and widow/orphan handling.
3. Add deterministic table spans, nested blocks, repeated headers, and
   continuation borders.
4. Verify page templates, sections, footnotes, and TOC anchors.
5. Rerun all backends when supported HTML/CSS semantics change.

Report extraction, overflow, table structure, geometry, rendering, and speed
separately. Pixel differences are diagnostic only; Pdfium, Chrome, and Acrobat
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
Its reproducible fixture remains pinned to `unpdf 0.17.0` and `pdf_oxide 0.3.77`
outside the main workspace; the comparison table above tracks newer package
references separately. The adapter emits the same per-input JSON shape. On the
current five-input corpus, harumi was the reference baseline at 100% marker
recall and coordinate coverage.
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
