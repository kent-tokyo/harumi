// inplace.rs — content-stream direct-replacement translation mode

use harumi::{Document, FontHandle, ReplaceOptions};
use ttf_parser::Face;

use crate::{
    Error, Result, TranslateOptions,
    output::{PageQualityReport, TranslateOutput, TranslateQuality},
    overlay,
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

/// Metrics from an InPlace translation pass used by the Auto cascade.
#[derive(Debug, Clone, Default)]
pub(crate) struct InPlaceStats {
    pub replaced: usize,
    pub fallback: usize,
    pub total_lines: usize,
}

impl InPlaceStats {
    /// Fraction of lines that needed overlay fallback (0.0 – 1.0).
    pub fn fallback_rate(&self) -> f32 {
        if self.total_lines == 0 {
            0.0
        } else {
            self.fallback as f32 / self.total_lines as f32
        }
    }
}

/// Core InPlace logic returning raw PDF bytes + placement stats.
///
/// Separated from the public entry point so the Auto cascade can call it
/// directly and inspect the stats before deciding whether to cascade.
pub(crate) async fn translate_pdf_inplace_inner(
    pdf_bytes: &[u8],
    options: &TranslateOptions,
) -> Result<(Vec<u8>, InPlaceStats, Vec<PageQualityReport>)> {
    let (overlay_pages, page_translations, global_body_fs) =
        overlay::extract_and_translate(pdf_bytes, options).await?;

    let face = Face::parse(&options.font, 0).map_err(|e| Error::FontParse(e.to_string()))?;

    let fallback_faces: Vec<Face<'_>> = options
        .font_fallbacks
        .iter()
        .filter_map(|b| Face::parse(b, 0).ok())
        .collect();
    let all_faces: Vec<&Face<'_>> = std::iter::once(&face)
        .chain(fallback_faces.iter())
        .collect();

    let cover_color = options.cover_color.unwrap_or([1.0, 1.0, 1.0]);
    let descender_ratio = (-face.descender() as f32 / face.units_per_em() as f32).clamp(0.05, 0.35);

    let mut doc = Document::from_bytes(pdf_bytes)?;
    let primary_font = doc.embed_font(&options.font)?;
    let mut font_handles: Vec<Option<FontHandle>> = std::iter::once(Some(primary_font))
        .chain(std::iter::repeat_n(None, options.font_fallbacks.len()))
        .collect();

    let mut stats = InPlaceStats::default();

    for overlay_page in &overlay_pages {
        let page_num = overlay_page.page_num;

        for &rect in &overlay_page.invisible_rects {
            doc.page(page_num)?.add_rect(rect, cover_color, 1.0)?;
        }

        let Some(translations) = page_translations.get(&page_num) else {
            continue;
        };

        for (line, trans_text) in overlay_page.lines.iter().zip(translations.iter()) {
            let text = trans_text.trim();
            if text.is_empty() {
                continue;
            }

            stats.total_lines += 1;

            // Compute normalized desired size and Tc using the same Tc-before-shrink
            // logic as the overlay path, then pass both to replace_text_opts so that
            // in-place stream replacements also benefit from size normalization and
            // character spacing compression.
            let min_fs = options.overflow.min_font_size();
            let max_fs_cap = line.line_height * 0.85;
            let base_fs = if line.normalized_font_size > 0.0 {
                line.normalized_font_size
            } else {
                global_body_fs
            };
            let desired_fs = if line.is_heading {
                (base_fs * 1.4).min(max_fs_cap)
            } else {
                base_fs.min(max_fs_cap)
            };
            let avail_w = overlay::available_width(line);
            let text_w = overlay::measure_text_width(text, &face, desired_fs);

            let (font_size_override, char_spacing_override) = if text_w > avail_w {
                let char_count = text.chars().count().max(1) as f32;
                let tc = (avail_w - text_w) / char_count;
                if tc >= -1.0 {
                    (Some(desired_fs), Some(tc))
                } else {
                    (
                        Some(overlay::fit_font_size(
                            text, &face, desired_fs, avail_w, min_fs,
                        )),
                        None,
                    )
                }
            } else {
                (Some(desired_fs), None)
            };

            let mut replace_opts = ReplaceOptions::default();
            replace_opts.font_size_override = font_size_override;
            replace_opts.char_spacing_override = char_spacing_override;

            let count = doc.page(page_num)?.replace_text_opts(
                &line.text,
                text,
                primary_font,
                replace_opts.clone(),
            )?;
            if count > 0 {
                stats.replaced += count;
                continue;
            }

            // Separator-stripping retry
            let retry = if let Some(stripped_orig) = strip_colon_prefix(&line.text) {
                if !stripped_orig.is_empty() {
                    let stripped_trans = strip_colon_prefix(text).unwrap_or(text);
                    doc.page(page_num)?.replace_text_opts(
                        stripped_orig,
                        stripped_trans,
                        primary_font,
                        replace_opts,
                    )?
                } else {
                    0
                }
            } else {
                0
            };

            if retry > 0 {
                continue;
            }

            // Overlay fallback: reuse the desired_fs / avail_w already computed above.
            stats.fallback += 1;
            let page_num_u = page_num;
            let desired = desired_fs;

            let below = line.font_size.max(global_body_fs) * descender_ratio;
            let y = line.y - below;
            let x = line.x - 1.0;
            let w = (line.right - x + 2.0).max(10.0);
            let h = line.line_height + below;
            doc.page(page_num_u)?
                .add_rect([x, y, w, h], cover_color, 1.0)?;

            // Tc-before-shrink: try character spacing compression first, then shrink.
            let base_w = overlay::measure_text_width(text, &face, desired);
            let (scaled, char_spacing) = if base_w > avail_w {
                let char_count = text.chars().count().max(1) as f32;
                let tc = (avail_w - base_w) / char_count;
                if tc >= -1.0 {
                    (desired, tc)
                } else {
                    (
                        overlay::fit_font_size(text, &face, desired, avail_w, min_fs),
                        0.0,
                    )
                }
            } else {
                (desired, 0.0)
            };
            let display_text: std::borrow::Cow<str> = match &options.overflow {
                crate::OverflowStrategy::Truncate { .. }
                    if overlay::measure_text_width(text, &face, scaled) > avail_w * 1.05 =>
                {
                    overlay::truncate_to_fit(text, &face, scaled, avail_w).into()
                }
                _ => text.into(),
            };

            let runs = overlay::split_by_font(&display_text, &all_faces);
            let bold = line.is_heading || line.is_bold;
            let mut run_x = line.x;
            for (run_text, fidx) in runs {
                if font_handles[fidx].is_none() {
                    font_handles[fidx] = Some(doc.embed_font(&options.font_fallbacks[fidx - 1])?);
                }
                let fh = font_handles[fidx].unwrap();
                let run_face = all_faces[fidx];
                doc.page(page_num_u)?.add_text_styled_with_char_spacing(
                    &run_text,
                    fh,
                    [run_x, line.y],
                    scaled,
                    [0.0f32, 0.0, 0.0],
                    bold,
                    false,
                    char_spacing,
                )?;
                let char_count_run = run_text.chars().count() as f32;
                run_x += overlay::measure_text_width(&run_text, run_face, scaled)
                    + char_spacing * char_count_run;
            }
        }
    }

    eprintln!(
        "[harumi-ai] InPlace: {} stream-replaced, {} overlay-fallback / {} total (fallback rate {:.0}%)",
        stats.replaced,
        stats.fallback,
        stats.total_lines,
        stats.fallback_rate() * 100.0,
    );

    let page_reports = overlay::compute_page_quality(
        pdf_bytes,
        &overlay_pages,
        &page_translations,
        &face,
        global_body_fs,
        options.overflow.min_font_size(),
    );

    Ok((doc.save_to_bytes()?, stats, page_reports))
}

/// Translate a PDF by rewriting content streams in-place, returning raw bytes.
///
/// Convenience wrapper around [`translate_pdf_inplace_inner`] for callers that
/// don't need placement statistics.  For the full structured result, use
/// [`translate_pdf_inplace_full`] or [`translate_pdf`].
#[allow(dead_code)]
pub(crate) async fn translate_pdf_inplace(
    pdf_bytes: &[u8],
    options: TranslateOptions,
) -> Result<Vec<u8>> {
    let (bytes, _stats, _page_reports) = translate_pdf_inplace_inner(pdf_bytes, &options).await?;
    Ok(bytes)
}

/// Full InPlace translation returning [`TranslateOutput`] (v0.2.0+).
pub async fn translate_pdf_inplace_full(
    pdf_bytes: &[u8],
    options: TranslateOptions,
) -> Result<TranslateOutput> {
    let (pdf_bytes_out, _stats, page_reports) =
        translate_pdf_inplace_inner(pdf_bytes, &options).await?;
    Ok(TranslateOutput {
        pdf_bytes: pdf_bytes_out,
        quality: TranslateQuality {
            pages: page_reports,
            overall: QualityResult::Pass,
            correction_rounds: 0,
            mode_used: crate::TranslationMode::InPlace,
            fallback_reason: None,
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
