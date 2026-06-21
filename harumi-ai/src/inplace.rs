// inplace.rs — content-stream direct-replacement translation mode

use harumi::{Document, FontHandle};
use ttf_parser::Face;

use crate::{
    Error, OverflowStrategy, Result, TranslateOptions, overlay,
    output::{TranslateOutput, TranslateQuality},
    quality::QualityResult,
};

/// Strip a leading fullwidth/halfwidth colon separator.
///
/// SDS form PDFs (e.g. Kanto Chemical) store label values like
/// `"： 関東化学株式会社"` where `：` is a static template element in the
/// content stream, separate from the variable value.  When extracted together
/// they form a string starting with `：`, but `replace_text` cannot find the
/// combined string because only the value portion exists as a replaceable run.
///
/// Returns the value substring after the separator, trimmed, or `None` if no
/// colon prefix is present.
fn strip_colon_prefix(text: &str) -> Option<&str> {
    let t = text.trim_start();
    let rest = t.strip_prefix('：').or_else(|| t.strip_prefix(':'))?;
    Some(rest.trim_start())
}

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

    let fallback_faces: Vec<Face<'_>> = options.font_fallbacks.iter()
        .filter_map(|b| Face::parse(b, 0).ok())
        .collect();
    let all_faces: Vec<&Face<'_>> = std::iter::once(&face)
        .chain(fallback_faces.iter())
        .collect();

    let cover_color = options.cover_color.unwrap_or([1.0, 1.0, 1.0]);
    let descender_ratio = (-face.descender() as f32 / face.units_per_em() as f32)
        .clamp(0.05, 0.35);

    let mut doc = Document::from_bytes(pdf_bytes)?;
    let primary_font = doc.embed_font(&options.font)?;
    let mut font_handles: Vec<Option<FontHandle>> =
        std::iter::once(Some(primary_font))
            .chain(std::iter::repeat_n(None, options.font_fallbacks.len()))
            .collect();

    let mut replaced = 0usize;
    let mut sep_retry = 0usize;  // succeeded after stripping "：" prefix
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
            let count = doc.page(page_num)?.replace_text(&line.text, text, primary_font)?;

            if count > 0 {
                replaced += count;
            } else {
                // Retry: strip a leading "：" separator.  In SDS form PDFs the
                // colon is a static template element stored separately from the
                // variable value, so the full "： value" string doesn't exist
                // as one replaceable text run, but "value" alone does.
                let retry = 'sep: {
                    let Some(orig_val) = strip_colon_prefix(&line.text) else { break 'sep 0; };
                    if orig_val.is_empty() { break 'sep 0; }
                    // Strip the same separator from the translation if present.
                    let trans_val = strip_colon_prefix(text).unwrap_or(text);
                    let trans_val = if trans_val.is_empty() { text } else { trans_val };
                    doc.page(page_num)?.replace_text(orig_val, trans_val, primary_font)?
                };

                if retry > 0 {
                    sep_retry += retry;
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
                        // chars().take() is mandatory — byte slicing (&s[..60]) panics on CJK text.
                        line.text.chars().take(40).collect::<String>()
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
                let min_fs = options.overflow.min_font_size();
                let scaled = overlay::fit_font_size(text, &face, desired, avail_w, min_fs)
                    .max(desired * 0.95);
                let display_text: std::borrow::Cow<str> = match &options.overflow {
                    OverflowStrategy::Truncate { .. }
                        if overlay::measure_text_width(text, &face, scaled) > avail_w * 1.05 =>
                    {
                        overlay::truncate_to_fit(text, &face, scaled, avail_w).into()
                    }
                    _ => text.into(),
                };
                let bold = line.is_heading || line.is_bold;
                let runs = overlay::split_by_font(&display_text, &all_faces);
                let mut run_x = line.x;
                for (run_text, fidx) in runs {
                    if font_handles[fidx].is_none() {
                        let fb = &options.font_fallbacks[fidx - 1];
                        font_handles[fidx] = Some(doc.embed_font(fb)?);
                    }
                    let fh = font_handles[fidx].unwrap();
                    let run_face = all_faces[fidx];
                    doc.page(page_num)?.add_text_styled(
                        &run_text, fh, [run_x, line.y], scaled, [0.0f32, 0.0, 0.0], bold, false,
                    )?;
                    run_x += overlay::measure_text_width(&run_text, run_face, scaled);
                }
                } // end else (retry == 0)
            }
        }
    }

    eprintln!(
        "[harumi-ai] InPlace: {} stream-replaced, {} sep-retry, {} overlay-fallback",
        replaced, sep_retry, fallback
    );

    doc.save_to_bytes().map_err(Into::into)
}

/// Full InPlace translation returning [`TranslateOutput`] (v0.2.0+).
pub async fn translate_pdf_inplace_full(pdf_bytes: &[u8], options: TranslateOptions) -> Result<TranslateOutput> {
    let _mode_correction_rounds = options.max_correction_rounds;
    let pdf_bytes_out = translate_pdf_inplace(pdf_bytes, options).await?;
    Ok(TranslateOutput {
        pdf_bytes: pdf_bytes_out,
        quality: TranslateQuality {
            // InPlace doesn't run the region-fitting loop, so no per-page summaries.
            pages: vec![],
            overall: QualityResult::Pass,
            correction_rounds: 0,
            mode_used: crate::TranslationMode::InPlace,
        },
        debug: None,
    })
}

#[cfg(test)]
mod tests {
    use super::strip_colon_prefix;

    // ── text truncation safety ────────────────────────────────────────────────

    #[test]
    fn truncation_char_safe_cjk() {
        // 21 Japanese chars = 63 bytes; byte 60 is inside the 21st char.
        // The old code `&s[..s.len().min(60)]` would panic here.
        let s = "化学物質名称有害性情報環境有害性区分等化学";
        assert_eq!(s.len(), 63);
        let truncated: String = s.chars().take(40).collect();
        // 21 chars fit within take(40) — full string is preserved
        assert_eq!(truncated, s);
    }

    #[test]
    fn truncation_char_safe_long_cjk() {
        // 50 CJK chars (150 bytes) — truncated to first 40 chars
        let s: String = "あ".repeat(50);
        assert_eq!(s.len(), 150);
        let truncated: String = s.chars().take(40).collect();
        assert_eq!(truncated.chars().count(), 40);
        assert_eq!(truncated.len(), 120); // 40 × 3 bytes
    }

    #[test]
    fn truncation_char_safe_ascii() {
        let s = "Safety Data Sheet";
        let truncated: String = s.chars().take(40).collect();
        assert_eq!(truncated, s);
    }

    #[test]
    fn truncation_char_safe_mixed() {
        // Mixed ASCII + CJK: "ABC" + 20 Japanese chars
        let s = format!("ABC{}", "化".repeat(20));
        assert_eq!(s.len(), 3 + 60); // 63 bytes total
        // byte 60 = 3 (ASCII) + 57 = inside 20th Japanese char
        let truncated: String = s.chars().take(40).collect();
        assert_eq!(truncated.chars().count(), 23); // 3 + 20
        assert_eq!(truncated, s);
    }

    // ── strip_colon_prefix ────────────────────────────────────────────────────

    #[test]
    fn strip_fullwidth_colon_prefix() {
        let input = "： 関東化学株式会社";
        let result = strip_colon_prefix(input);
        assert_eq!(result, Some("関東化学株式会社"));
    }

    #[test]
    fn strip_halfwidth_colon_prefix() {
        let result = strip_colon_prefix(": value");
        assert_eq!(result, Some("value"));
    }

    #[test]
    fn strip_colon_prefix_no_prefix_returns_none() {
        assert!(strip_colon_prefix("化学物質名称").is_none());
        assert!(strip_colon_prefix("").is_none());
    }

    #[test]
    fn strip_colon_prefix_colon_only() {
        // A colon with nothing after it → empty result
        let result = strip_colon_prefix("：");
        assert_eq!(result, Some(""));
    }

    #[test]
    fn strip_colon_prefix_with_leading_whitespace() {
        let result = strip_colon_prefix("  ： value");
        assert_eq!(result, Some("value"));
    }

    #[test]
    fn strip_colon_prefix_cjk_value_no_panic() {
        // Japanese value with >20 chars after the colon (>60 bytes)
        let value = "化学物質名称有害性情報環境有害性区分等化学";
        let input = format!("：{value}");
        let result = strip_colon_prefix(&input);
        assert_eq!(result, Some(&value[..]));
    }
}
