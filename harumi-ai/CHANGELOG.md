# Changelog — harumi-ai

This is a concise release summary. Detailed history is available in the
[Git tags](https://github.com/kent-tokyo/harumi/tags) and pull requests.

## Unreleased

- Keep translation documentation aligned with the `harumi` core API and its
  layout-quality boundaries.

## 0.10.1 — 2026-09-06

- Preserve source run color, opacity, bold, and italic boundaries in overlays.
- Add basic Identity-V vertical metrics and rotated/sheared image collision
  handling through the core APIs.
- Refresh multi-page OCR, layout-quality, and renderer-boundary documentation.

## 0.10.0 — 2026-09-05

- Add rotation-aware overlay placement for common 90°/270° lines.
- Add multi-page OCR JSON validation and page-specific coordinate conversion.
- Improve blank-translation handling and page-level layout diagnostics.
- Align the dependency requirement with `harumi` 1.21.

## 0.6.0–0.3.0 — 2026-06-21

- Add geometry and optional vision-assisted layout repair.
- Add `VisionProvider`, rasterization options, layout corrections, and builder
  configuration for repair passes.
- Add translation overlays, in-place replacement, scanned-PDF OCR input,
  quality gates, provider adapters, and retry/error handling.

## 0.2.1–0.1.1 — 2026-06-xx

- Establish the async translator API, provider configuration, OCR JSON input,
  and core translation output types.

Complex vertical writing, mixed styles, external rasterizers, and pixel-identical
output remain best-effort. Review generated PDFs with a renderer.
