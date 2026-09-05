# harumi — Full Feature List

| Challenge | harumi's answer |
|---|---|
| CJK font subsetting is complex | One `embed_font()` call — only used glyphs are included, GIDs correctly remapped; GSUB/GPOS/variable-font tables stripped for macOS Preview and PSPDFKit compatibility |
| Don't want to corrupt existing PDF structure | Append-only: harumi never touches the original object graph |
| Need to run in WASM / Lambda / cross-compile | Pure Rust — zero C/C++ dependencies |
| Need OCR text at specific coordinates | `add_invisible_text` / batch `add_invisible_text_runs` |
| Need to stamp a watermark on PDFs | `add_text(color)` overlays visible text in any RGB color |
| Need to position text relative to page size | `page.size()` reads the MediaBox |
| Need in-memory output for Tauri / WASM | `save_to_bytes()` returns a `Vec<u8>` directly |
| Need to draw highlight rectangles or lines | `add_rect` / `add_line` (`draw` feature, no extra deps) |
| Need to draw a box border or polygon (callout) | `add_rect_stroke` / `add_polygon` (`draw` feature) |
| Need multi-line wrapped text in a box | `add_text_box` (no feature gate needed) |
| Need to embed JPEG / PNG images | `add_image` / `add_image_with_opacity` (`image` feature) |
| Need PNG transparency (signatures, watermarks) | Transparent PNGs use PDF SMask automatically — no white background |
| Need to rotate, remove, or reorder pages | `rotate_page` / `remove_page` / `insert_blank_page` / `reorder_pages` (no feature gate) |
| Need to merge two PDFs into one | `merge_from` appends all pages from another document; content and fonts preserved |
| Need to create a PDF from scratch (no existing file) | `Document::new(size)` creates a blank 1-page PDF; add pages with `insert_blank_page` |
| Need to split a PDF into separate files | `extract_pages` returns a new `Document` with the specified pages in any order |
| Need to extract text positions from an existing PDF | `extract_text_runs` decodes CID fonts and standard simple fonts (Type1, TrueType, Type3, WinAnsi, etc.) |
| Need to read or write PDF metadata (title, author…) | `doc.metadata()` reads `/Info`; `doc.set_metadata(&meta)` writes it |
| Need to replace text in an existing PDF (new font) | `page.replace_text(old, new, font)` rewrites the content stream in-place; returns the match count as `usize`; automatic font-switching and width compensation |
| Need to replace text using the original font | `page.replace_text_preserve_font(old, new)` — no `FontHandle` needed; returns match count; validates glyphs eagerly (not at `save()`) |
| Need to check replaceability without modifying | `page.can_replace_text(old, new)` — pure read-only scan; returns match count or `Err(FontCharNotMapped)` |
| Need to draw an ellipse or circle | `add_ellipse(rect, color, opacity, filled, stroke_width)` (`draw` feature) |
| Need fill + stroke on same shape | pass `filled=true` and `stroke_width>0` to `add_ellipse` / `add_polygon` / `add_path` — uses PDF `B` operator |
| Need open or closed path (polyline + polygon unified) | `add_path(points, closed, color, filled, stroke_width, opacity)` (`draw` feature) |
| Need rotated text (watermarks, stamps at an angle) | `add_text_with_rotation(text, font, pos, size, color, opacity, degrees)` |
| Need to replace text spanning multiple Tj operators or font runs | `replace_text` / `replace_text_preserve_font` — cross-operator **and** cross-Tf matching supported |
| Need to extract an embedded image from a scanned PDF | `extract_page_image` returns JPEG or PNG bytes (`image` feature); scanned PDFs only |
| Need clickable URL links in a PDF | `add_link_url([x, y, w, h], url)` — invisible URI annotation; click opens the URL in any viewer |
| Need internal navigation links (TOC) | `add_link_internal([x, y, w, h], target_page)` — jumps to a page within the same document |
| Need a bookmarks / navigation outline | `add_bookmark(title, page, y)` — flat PDF outline entries; CJK titles stored as UTF-16BE automatically |
| Need page numbers / running headers–footers on every page | `FlowOptions { header: Some(hf), footer: Some(hf), .. }` with `HeaderFooter` (`flow` feature); `{{page}}` / `{{total}}` substituted at render |
| Need headings to auto-generate outline entries | `FlowOptions { auto_bookmarks: true, .. }` (default) — every `push_heading` creates a bookmark |
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
