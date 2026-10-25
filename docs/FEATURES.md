# harumi — Feature map

## Core API

`Document` loads, edits, saves, merges, splits, rotates, and reorders PDFs.
Use `embed_font` with `add_text` or `add_invisible_text` for CJK text; use
`extract_text_runs` and `replace_text` for positioned extraction and editing.
`draw`, `image`, `flow`, `html`, `svg`, `ocr`, and `digital-signature` add the
capabilities listed in [`API.md`](API.md).

## Layout and document operations

| Area | Main APIs |
|---|---|
| Flow layout | `FlowDocument`, `FlowOptions`, headings, bookmarks, TOC, footnotes, sections, templates, decorations |
| Tables | `push_table_cells`, `push_table_blocks`, nested tables, keep-together options |
| HTML | `render_html_to_pdf`, page breaks, `break-inside: avoid`, supported CSS subset |
| Text fitting | `fit_text_to_box`, `detect_collisions`, `extract_layout_regions`, `plan_text_for_regions` |
| Pages | Crop/trim/bleed boxes, scale/resize, merge, split, reorder, rotate, overlay |
| Forms and security | `form_fields`, `fill_form`, password loading, `set_encryption`, signatures |
| Annotations and files | Links, markup, sticky notes, attachments, metadata |

## Extraction and diagnostics

| Need | API |
|---|---|
| Extract text with warnings | `extract_text_runs_verbose`, `for_each_text_runs_verbose_batch` |
| Group text or detect columns | `group_text_fragments`, `detect_text_columns` |
| Extract tables and vector rules | `extract_table_cells`, `extract_vector_rules` |
| Inspect style and placement | `TextFragment::{font_family, is_bold, is_italic, rotation_degrees, opacity}` |
| Check font coverage | `font_covers_char(font_bytes, ch)` |
| Record structural changes | `audit_structure`, `mutation_report` |

## Limits

- Flow and HTML implement a documented subset; full browser layout, Unicode bidi
  shaping, RTL, and vertical writing are not guaranteed.
- SVG is a vector subset, not a browser renderer. Geometry-only SVG can omit
  font bytes; rendered text still requires a font. See [`API.md`](API.md).
- Extraction and replacement are best-effort for malformed PDFs, missing
  `/ToUnicode`, complex XObjects, and mixed writing modes. Render output when
  visual fidelity matters.
