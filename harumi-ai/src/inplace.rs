// inplace.rs — content-stream direct-replacement translation mode

use harumi::Document;
use ttf_parser::Face;

use crate::{Error, Result, TranslateOptions, overlay};

/// Translate a PDF by rewriting content streams in-place.
///
/// For each extracted text line, [`harumi::PageHandle::replace_text`] rewrites
/// the original `Tj`/`TJ` operators directly, eliminating the original text
/// from the content stream.  Lines where no match is found (e.g. per-character
/// Japanese PDFs with `Td` between each `Tj`) fall back to the overlay
/// approach: a cover rectangle followed by [`harumi::PageHandle::add_text_styled`].
pub async fn translate_pdf_inplace(pdf_bytes: &[u8], options: TranslateOptions) -> Result<Vec<u8>> {
    let (overlay_pages, page_translations, global_body_fs) =
        overlay::extract_and_translate(pdf_bytes, &options).await?;

    let face = Face::parse(&options.font, 0)
        .map_err(|e| Error::FontParse(e.to_string()))?;

    let cover_color = options.cover_color.unwrap_or([1.0, 1.0, 1.0]);
    let descender_ratio = (-face.descender() as f32 / face.units_per_em() as f32)
        .clamp(0.05, 0.35);

    let mut doc = Document::from_bytes(pdf_bytes)?;
    let font = doc.embed_font(&options.font)?;

    let mut replaced = 0usize;
    let mut fallback = 0usize;

    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;

        // OCR / invisible-text rectangles always need a cover (render-mode-3
        // text is invisible to readers but present in the stream).
        for &rect in &overlay_page.invisible_rects {
            doc.page(page_num)?.add_rect(rect, cover_color, 1.0)?;
        }

        let Some(translations) = page_translations.get(&page_num) else { continue };

        for (line, trans_text) in overlay_page.lines.iter().zip(translations.iter()) {
            let text = trans_text.trim();
            if text.is_empty() { continue; }

            // Attempt content-stream rewrite.  replace_text() returns the
            // match count immediately (read-only scan) and queues the actual
            // rewrite for finalize(); so we can branch on it right away.
            let count = doc.page(page_num)?.replace_text(&line.text, text, font)?;

            if count > 0 {
                replaced += count;
            } else {
                // Fall back to overlay: cover the original text with a
                // rectangle, then draw the translation on top.
                fallback += 1;
                if cfg!(debug_assertions) {
                    let reason = doc.page(page_num)?.diagnose_replace_failure(&line.text);
                    eprintln!(
                        "[harumi-ai] fallback page={} reason={} text={:?}",
                        page_num,
                        reason,
                        &line.text[..line.text.len().min(60)]
                    );
                }
                let x = line.x - 1.0;
                let below = line.font_size.max(global_body_fs) * descender_ratio;
                let y = line.y - below;
                let w = (line.right - x + 2.0).max(10.0);
                let h = line.line_height + below;
                doc.page(page_num)?.add_rect([x, y, w, h], cover_color, 1.0)?;

                let fs = if line.font_size > 0.0 { line.font_size } else { global_body_fs };
                let desired = if line.is_heading {
                    (fs * 1.4).min(line.line_height * 0.85)
                } else {
                    fs.min(line.line_height * 0.85)
                };
                let avail_w = (line.col_right - line.x).max(50.0);
                let scaled = overlay::fit_font_size(text, &face, desired, avail_w)
                    .max(desired * 0.95);
                let bold = line.is_heading || line.is_bold;
                doc.page(page_num)?.add_text_styled(
                    text, font, [line.x, line.y], scaled, [0.0f32, 0.0, 0.0], bold, false,
                )?;
            }
        }
    }

    eprintln!(
        "[harumi-ai] InPlace: {} stream-replaced, {} overlay-fallback",
        replaced, fallback
    );

    doc.save_to_bytes().map_err(Into::into)
}
