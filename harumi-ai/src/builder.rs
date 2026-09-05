use crate::{Error, LayoutOptions, Result};
use harumi::{Document, wrap_paragraph};
use ttf_parser::Face;

// ── Wire types ───────────────────────────────────────────────────────────────

pub(crate) struct TranslatedPage {
    pub size: (f32, f32),
    pub blocks: Vec<OutputBlock>,
}

pub(crate) struct OutputBlock {
    pub block_type: String,
    pub text: String,
}

// ── PDF builder ──────────────────────────────────────────────────────────────

/// Build a new PSPDFKit-compatible PDF from translated page content.
///
/// Uses harumi's direct API (`Document::new` + `embed_font` + `add_text`) which
/// produces CIDFontType2 with a correct ToUnicode CMap — unlike FlowDocument.
///
/// Layout: each source page starts on a new output page. If a page's content
/// overflows, additional pages of the same size are inserted automatically.
pub(crate) fn build_pdf(
    pages: &[TranslatedPage],
    font_bytes: &[u8],
    layout: &LayoutOptions,
) -> Result<Vec<u8>> {
    if pages.is_empty() {
        // Return a minimal blank PDF if source had no extractable text.
        let mut doc = Document::new((595.0, 842.0))?;
        return doc.save_to_bytes().map_err(Into::into);
    }

    let face = Face::parse(font_bytes, 0).map_err(|e| Error::FontParse(e.to_string()))?;

    let first_size = pages[0].size;
    let mut doc = Document::new(first_size)?;
    let font = doc.embed_font(font_bytes)?;

    let mut page_count = 1u32;
    let mut cur_page = 1u32;
    let mut y = first_size.1 - layout.margin;

    for (src_idx, src_page) in pages.iter().enumerate() {
        let page_size = src_page.size;
        let max_width = page_size.0 - 2.0 * layout.margin;

        for block in &src_page.blocks {
            let font_size = layout.font_size_for_type(&block.block_type);
            let lines = wrap_paragraph(&block.text, &face, font_size, max_width);

            for line in &lines {
                // Add a new page if the current line won't fit.
                if y < layout.margin + font_size {
                    doc.insert_blank_page(page_count, page_size)?;
                    page_count += 1;
                    cur_page = page_count;
                    y = page_size.1 - layout.margin;
                }
                let mut ph = doc.page(cur_page)?;
                ph.add_text(
                    line,
                    font,
                    [layout.margin, y],
                    font_size,
                    [0.0f32, 0.0, 0.0],
                )?;
                y -= font_size * layout.line_height_ratio;
            }

            if !lines.is_empty() {
                y -= font_size * layout.paragraph_gap_ratio;
            }
        }

        // Start the next source page on a fresh output page (except after the last).
        if src_idx + 1 < pages.len() {
            let next_size = pages[src_idx + 1].size;
            doc.insert_blank_page(page_count, next_size)?;
            page_count += 1;
            cur_page = page_count;
            y = next_size.1 - layout.margin;
        }
    }

    doc.save_to_bytes().map_err(Into::into)
}
