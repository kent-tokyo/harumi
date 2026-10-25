# harumi — Feature map

## Core API

`Document` loads, edits, saves, merges, splits, rotates, and reorders PDFs.
Use `embed_font` with `add_text` or `add_invisible_text` for CJK text; use
`extract_text_runs` and `replace_text` for positioned extraction and editing.
`draw`, `image`, `flow`, `html`, `svg`, `ocr`, and `digital-signature` add the
capabilities listed in [`API.md`](API.md).

## Advanced API index

| Need | API |
|---|---|
| Need page numbers / running headers–footers on every page | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` with `HeaderFooter` (`flow` feature); `{{page}}` / `{{total}}` substituted at render |
| Need headings to auto-generate outline entries | `FlowOptions { auto_bookmarks: true, .. }` (default) — every `push_heading` creates a bookmark |
| Need a named bookmark at a Flow position | `FlowDocument::push_bookmark(title, level)` records a deterministic outline destination at the current page position |
| Need a generated table of contents | `FlowDocument::push_table_of_contents(title)` appends a trailing TOC page from bookmarks collected so far |
| Need page footnotes | `FlowDocument::push_footnote(text)` reserves the current page's bottom band and renders a numbered note; automatic reference insertion and endnotes are not included |
| Need CSS page breaks | HTML supports legacy `page-break-before/after` and modern `break-before/after: page` forms, plus page-break classes |
| Need an HTML paragraph kept together | `style="break-inside: avoid"` or `page-break-inside: avoid` uses Flow's measured keep-together behavior when the paragraph fits on one page |
| Need mixed Latin/CJK or symbol-heavy body text | `FlowOptions { fallback_font_bytes: Some(bytes), .. }` selects the fallback per missing glyph while preserving run advances |
| Need stronger paragraph page-boundary control | `FlowOptions { paragraph_min_lines: 3, .. }` keeps at least three lines together at widow/orphan boundaries |
| Need headings to avoid page-bottom isolation | `FlowOptions { keep_headings_with_next: true, .. }` reserves the first following body line when it fits |
| Need a paragraph to start as one block | `FlowDocument::push_paragraph_keep_together(...)` moves a fitting paragraph to the next page; paragraphs taller than one page paginate normally |
| Need different headers/footers by section | `FlowDocument::push_section(header, footer)` starts a new page and applies the new template to that page and later pages |
| Need different body margins by section | `FlowDocument::push_section_with_margins(header, footer, margins)` starts a new page with independent content geometry |
| Need first/odd/even page templates | `PageTemplateVariants` with `set_page_template_variants` selects independent header/footer, margins, and decoration by page class |
| Need body content protected from small header/footer margins | Flow geometry automatically reserves the excess header/footer height when the configured margin is smaller than the template |
| Need page backgrounds or borders by section | `PageDecoration` with `set_page_decoration` or `push_section_with_decoration` applies bounded background and border templates |
| Need tables to avoid page-bottom isolation | `FlowDocument::push_table_cells_keep_together(...)` keeps a fitting table together; `TableOptions { keep_with_next: true, .. }` also reserves the first following body line |
| Need a measured table inside a report cell | `FlowDocument::push_table_blocks(...)` or HTML `<table>` nesting uses `FlowTableBlockCell`/`FlowTableCellBlock::NestedTable` for one nested level; outer and isolated child rows split safely across pages, including synchronized child tables |
| Need stable paragraph alignment | `FlowOptions { body_alignment: FlowTextAlignment::Center, .. }` aligns measured normal and styled lines |
| Need to load a password-protected PDF | `Document::from_file_with_password(path, pw)` / `from_bytes_with_password(bytes, pw)` — decrypts on load; both user and owner passwords accepted |
| Need to save a PDF with password protection | `doc.set_encryption(user_pw, owner_pw)` — encrypts at `save()` time with 128-bit RC4 |
| Need to check if a PDF was originally encrypted | `doc.is_encrypted()` — `true` even after successful decryption |
| Need to highlight / underline / strike through text | `add_highlight` / `add_underline` / `add_strikeout` / `add_squiggly` with color — standard PDF markup annotations with QuadPoints |
| Need to add a sticky-note comment to a page | `add_sticky_note([x, y], "note text")` — Text annotation, Unicode contents |
| Need to read PDF form field values | `doc.form_fields()` — returns `Vec<FormField>` with name, type, and current value |
| Need to fill in a PDF form programmatically | `doc.fill_form(&[("FieldName", "value")])` — sets values and triggers NeedAppearances |
| Need to set/read page crop or print boxes | `page.crop_box()` / `set_crop_box(rect)` / `trim_box()` / `bleed_box()` — all box types in `[x,y,w,h]` format |
| Need to scale page content (e.g. A4 → A3) | `page.scale_page_content(sx, sy)` inserts a `cm` matrix before existing content; `resize_page_with_content(w, h)` scales + resizes MediaBox in one call |
| Need to overlay one PDF on top of another | `doc.overlay_from(other)` stamps each page of `other` onto the matching page of `self` as a Form XObject; fonts, images, and opacity are preserved |
| Need to remove all bookmarks / TOC | `doc.clear_outline()` removes both pending bookmarks and any existing `/Outlines` tree in a loaded PDF |
| Need to attach files to a PDF | `doc.attach_file(name, data, mime)` embeds any file as a PDF attachment; `doc.list_attachments()` returns `Vec<AttachmentInfo>` |
| Need bold/italic/font-family from extracted text | `TextFragment::is_bold`, `is_italic`, `font_family`, `base_font` are parsed from `/BaseFont` |
| Need to detect column layout from extracted text | `detect_text_columns(&frags, page_width)` returns `Vec<ColumnZone>` by gap detection |
| Need to group extracted fragments into lines or paragraphs | `group_text_fragments(&frags, GroupingStrategy::Paragraph)` returns `TextGroup`s |
| Need to check whether a font file covers a given character | `font_covers_char(font_bytes, ch) -> bool` queries the font cmap |
| Need to extract text from a PDF table cell by cell | `extract_table_cells(&frags, page_width, page_height)` returns heuristic row/column/text/bbox cells |
| Need to use CMYK colors (print workflow) | `Color::Cmyk([c, m, y, k])` |
| Need to verify digital signatures on a PDF | `doc.verify_signatures(&pdf_bytes)` (`digital-signature` feature) |
| Need to create and sign a PDF digitally | `add_signature_field` + `SigningContext` + `sign_document` (`digital-signature` feature) |
| Need to plan text layout in a fixed rectangle | `doc.fit_text_to_box(text, font, rect, font_size, opts) -> FitResult` |
| Need to detect overlaps between planned text boxes | `detect_collisions(boxes) -> Vec<Collision>` |
| Need layout regions for translation | `extract_layout_regions(&frags, page_w, page_h, opts) -> Vec<LayoutRegion>` |
| Need to batch-plan translated text into layout cells | `doc.plan_text_for_regions(regions, replacements, font, opts) -> Vec<RegionFitPlan>` |
| Need a page-level layout quality gate | `doc.assess_page_layout_quality(page, &plans) -> PageLayoutQuality` |
| Need to extract PDF table borders (vector rules) | `doc.extract_vector_rules(page) -> Vec<VectorRule>` |
| Need font-size normalization for translation | `ReplaceOptions::font_size_override` / `char_spacing` |
| Need to preserve 90°/270° text direction during overlay | `TextFragment::rotation_degrees` is propagated by `harumi-ai` for common vertical lines |
| Need basic CJK vertical extraction | Type0 `/Identity-V` with `/DW2`/`/W2` metrics and common 90°/270° placement; complex mixed composition remains best-effort |
| Need source text opacity in extraction | `TextFragment::opacity` from page or directly processed Form XObject `/ExtGState /ca` |

For the current limits and supported SVG subset, see [`API.md`](API.md) and
[`ROADMAP.md`](../ROADMAP.md). This table is an index, not a compatibility
promise for every PDF producer or viewer.
