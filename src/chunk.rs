use crate::{Document, Result, TextFragment};

/// The semantic type of a text chunk.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkType {
    /// A heading at the given level (1–6). Determined by font size ratio vs. baseline.
    Heading(u8),
    /// A paragraph (body text).
    Paragraph,
}

/// A semantic text chunk extracted from a page.
///
/// Chunks group consecutive text fragments by semantic role (heading vs. paragraph)
/// based on font size heuristics. Invisible fragments (e.g., OCR search layers) are
/// excluded.
///
/// Returned by [`Document::extract_text_chunks`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// Concatenated text of all fragments in this chunk.
    pub text: String,
    /// Bounding box: `[x, y, width, height]` in PDF points.
    /// - `x`, `y` — minimum coordinates
    /// - `width`, `height` — extent from the minimum
    pub bbox: [f32; 4],
    /// Semantic role of this text.
    pub chunk_type: ChunkType,
    /// Average font size of fragments in this chunk (PDF points).
    pub avg_font_size: f32,
}

/// Extracts text chunks (semantic blocks) from a page.
///
/// # Algorithm
///
/// 1. Extract text fragments via [`extract_text_runs`](Document::extract_text_runs)
/// 2. Sort by reading order (top→bottom, left→right)
/// 3. Filter out invisible fragments (OCR layers)
/// 4. Group fragments into lines by y-coordinate (tolerance: `±font_size * 0.5`)
/// 5. Calculate baseline font size (median within first 5 lines)
/// 6. Classify lines as headings or paragraphs by font size ratio:
///    - ≥1.8× baseline → H1
///    - ≥1.5× baseline → H2
///    - ≥1.3× baseline → H3
///    - ≥1.15× baseline → H4
///    - Otherwise → Paragraph (or H5/H6 if heading sequence continues)
/// 7. Merge consecutive lines of the same type into a single chunk
/// 8. Compute bbox as min/max extent of constituent fragments
///
/// # Example
///
/// ```no_run
/// # use harumi::Document;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let doc = Document::from_file("example.pdf")?;
/// let chunks = doc.extract_text_chunks(1)?;
/// for chunk in chunks {
///     match chunk.chunk_type {
///         harumi::ChunkType::Heading(level) => {
///             println!("{} {}", "#".repeat(level as usize), chunk.text);
///         }
///         harumi::ChunkType::Paragraph => {
///             println!("{}", chunk.text);
///         }
///         _ => {}
///     }
/// }
/// # Ok(())
/// # }
/// ```
impl Document {
    pub fn extract_text_chunks(&self, page: u32) -> Result<Vec<TextChunk>> {
        let mut fragments = self.extract_text_runs(page)?;

        // Sort by reading order and filter out invisible fragments.
        crate::sort_by_reading_order(&mut fragments);
        let fragments: Vec<_> = fragments.into_iter().filter(|f| !f.invisible).collect();

        if fragments.is_empty() {
            return Ok(Vec::new());
        }

        // Group fragments into lines by y-coordinate.
        let lines = group_into_lines(&fragments);

        // Calculate baseline font size from the first few lines.
        let baseline_font_size = estimate_baseline_font_size(&lines);

        // Classify lines as heading or paragraph.
        let classified = lines
            .into_iter()
            .map(|line| {
                let avg_font_size =
                    line.iter().map(|f| f.font_size).sum::<f32>() / line.len() as f32;
                let ratio = avg_font_size / baseline_font_size;
                let chunk_type = classify_by_ratio(ratio);
                (line, avg_font_size, chunk_type)
            })
            .collect::<Vec<_>>();

        // Merge consecutive lines of the same type.
        let merged = merge_consecutive_chunks(classified);

        Ok(merged)
    }

    /// Extracts text from a page as a Markdown string.
    ///
    /// Uses [`extract_text_chunks`](Document::extract_text_chunks) to identify
    /// headings and paragraphs, then formats as Markdown:
    /// - Headings: `"#".repeat(level) + " " + text`
    /// - Paragraphs: plain text
    /// - Blank lines between blocks
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use harumi::Document;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let doc = Document::from_file("example.pdf")?;
    /// let markdown = doc.extract_as_markdown(1)?;
    /// println!("{}", markdown);
    /// # Ok(())
    /// # }
    /// ```
    pub fn extract_as_markdown(&self, page: u32) -> Result<String> {
        let chunks = self.extract_text_chunks(page)?;
        let mut result = String::new();

        for chunk in chunks {
            match chunk.chunk_type {
                ChunkType::Heading(level) => {
                    result.push_str(&"#".repeat(level as usize));
                    result.push(' ');
                    result.push_str(&chunk.text);
                    result.push_str("\n\n");
                }
                ChunkType::Paragraph => {
                    result.push_str(&chunk.text);
                    result.push_str("\n\n");
                }
            }
        }

        // Trim trailing whitespace.
        while result.ends_with('\n') || result.ends_with(' ') {
            result.pop();
        }

        Ok(result)
    }
}

/// Groups fragments into horizontal lines by y-coordinate.
/// Fragments within `±font_size * 0.5` of each other are considered on the same line.
/// Handles edge cases: negative/NaN font_size, malformed coordinates.
fn group_into_lines(fragments: &[TextFragment]) -> Vec<Vec<TextFragment>> {
    let mut lines: Vec<Vec<TextFragment>> = Vec::new();

    for frag in fragments {
        // Defensive: ensure font_size is positive and finite.
        let font_size = if frag.font_size > 0.0 && frag.font_size.is_finite() {
            frag.font_size
        } else {
            1.0 // Default tolerance if font_size is invalid
        };
        let tol = font_size * 0.5;
        let mut placed = false;

        for line in &mut lines {
            // Check if this fragment is within tolerance of the line's y.
            if let Some(first) = line.first()
                && (frag.y - first.y).abs() <= tol
            {
                line.push(frag.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            lines.push(vec![frag.clone()]);
        }
    }

    lines
}

/// Estimates baseline font size from the first few lines (minimum approach).
/// Uses the smallest finite font size found, as it's most likely to be body text.
/// Filters out NaN and Infinity values for robustness.
fn estimate_baseline_font_size(lines: &[Vec<TextFragment>]) -> f32 {
    let sizes: Vec<f32> = lines
        .iter()
        .take(10) // Use first 10 lines for baseline estimate.
        .filter_map(|line| {
            let avg = line.iter().map(|f| f.font_size).sum::<f32>() / line.len() as f32;
            // Only include finite, positive values
            if avg > 0.0 && avg.is_finite() {
                Some(avg)
            } else {
                None
            }
        })
        .collect();

    if sizes.is_empty() {
        return 12.0; // Default fallback.
    }

    // Use minimum font size as baseline (body text is typically smallest).
    // This ensures headings are reliably detected as multiples of baseline.
    sizes.into_iter().fold(f32::INFINITY, f32::min).max(1.0)
}

/// Classifies a line as heading or paragraph based on font size ratio.
/// Returns Paragraph if ratio is NaN or Infinity (malformed input).
fn classify_by_ratio(ratio: f32) -> ChunkType {
    // Guard against NaN and Infinity (treat as paragraph/unclassified).
    if !ratio.is_finite() {
        return ChunkType::Paragraph;
    }

    if ratio >= 1.8 {
        ChunkType::Heading(1)
    } else if ratio >= 1.5 {
        ChunkType::Heading(2)
    } else if ratio >= 1.3 {
        ChunkType::Heading(3)
    } else if ratio >= 1.15 {
        ChunkType::Heading(4)
    } else {
        ChunkType::Paragraph
    }
}

/// Merges consecutive lines of the same chunk type into a single chunk.
fn merge_consecutive_chunks(
    classified: Vec<(Vec<TextFragment>, f32, ChunkType)>,
) -> Vec<TextChunk> {
    let mut result: Vec<TextChunk> = Vec::new();

    for (line, avg_font_size, chunk_type) in classified {
        let text = line
            .iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>()
            .join("");

        // Compute bbox for this line.
        let min_x = line.iter().map(|f| f.x).fold(f32::INFINITY, f32::min);
        let min_y = line.iter().map(|f| f.y).fold(f32::INFINITY, f32::min);
        let max_x = line
            .iter()
            .map(|f| f.x + f.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = line
            .iter()
            .map(|f| f.y + f.height)
            .fold(f32::NEG_INFINITY, f32::max);

        let bbox = [
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ];

        // Try to merge with the last chunk if it has the same type.
        let merged = if let Some(last) = result.last_mut() {
            if last.chunk_type == chunk_type {
                last.text.push(' ');
                last.text.push_str(&text);
                // Expand bbox.
                let [x1, y1, w1, h1] = last.bbox;
                let x2 = x1 + w1;
                let y2 = y1 + h1;
                let new_min_x = min_x.min(x1);
                let new_min_y = min_y.min(y1);
                let new_max_x = max_x.max(x2);
                let new_max_y = max_y.max(y2);
                last.bbox = [
                    new_min_x,
                    new_min_y,
                    (new_max_x - new_min_x).max(0.0),
                    (new_max_y - new_min_y).max(0.0),
                ];
                // Update avg_font_size (weighted average).
                let old_count = last.text.split_whitespace().count() as f32;
                let new_count = text.split_whitespace().count() as f32;
                let total = old_count + new_count;
                if total > 0.0 {
                    last.avg_font_size =
                        (last.avg_font_size * old_count + avg_font_size * new_count) / total;
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        // No merge, add as a new chunk.
        if !merged {
            result.push(TextChunk {
                text,
                bbox,
                chunk_type,
                avg_font_size,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_by_ratio_headings() {
        assert_eq!(classify_by_ratio(1.9), ChunkType::Heading(1));
        assert_eq!(classify_by_ratio(1.8), ChunkType::Heading(1));
        assert_eq!(classify_by_ratio(1.5), ChunkType::Heading(2));
        assert_eq!(classify_by_ratio(1.3), ChunkType::Heading(3));
        assert_eq!(classify_by_ratio(1.15), ChunkType::Heading(4));
        assert_eq!(classify_by_ratio(1.0), ChunkType::Paragraph);
        assert_eq!(classify_by_ratio(0.8), ChunkType::Paragraph);
    }

    #[test]
    fn baseline_font_size_from_empty() {
        let lines: Vec<Vec<TextFragment>> = vec![];
        assert_eq!(estimate_baseline_font_size(&lines), 12.0);
    }
}
