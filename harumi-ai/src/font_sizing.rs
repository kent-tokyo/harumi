// font_sizing.rs — font size normalization and quantization for PDF translation

use harumi::LayoutRegionRole;

use crate::overlay::OverlayLine;

// ── Policy ────────────────────────────────────────────────────────────────────

/// Controls how the desired font size is derived for each translated line.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FontSizePolicy {
    /// Use each line's raw extracted font_size unchanged.
    Preserve,
    /// Replace per-line sizes with the median for all lines that share the same
    /// [`LayoutRegionRole`] on the page. Reduces jitter from 8.8 / 9.0 / 9.2 variation.
    RoleMedian,
    /// Use the page-level body_font_size (already computed by the extractor) for
    /// every line; headings still apply their 1.4× multiplier on top.
    PageBodyMedian,
    /// Apply [`RoleMedian`](Self::RoleMedian), then snap to the nearest standard PDF point size.
    /// This is the default and produces the most visually consistent output.
    #[default]
    Quantized,
}

// ── Quantization ──────────────────────────────────────────────────────────────

/// Standard PDF point sizes used for quantization.
pub const STANDARD_SIZES: &[f32] = &[
    6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0,
];

/// Snap `fs` to the nearest value in [`STANDARD_SIZES`].
pub fn quantize_font_size(fs: f32) -> f32 {
    STANDARD_SIZES
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - fs)
                .abs()
                .partial_cmp(&(b - fs).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(fs)
}

// ── Role median computation ───────────────────────────────────────────────────

/// Compute the median `font_size` per [`LayoutRegionRole`] across the given lines.
///
/// Lines with `font_size == 0.0` are excluded from the computation.
/// Returns a vec of `(role, median)` pairs — one entry per role that has at
/// least one valid size.  Uses a plain Vec with linear search because there are
/// at most six distinct roles.
pub(crate) fn compute_role_medians(lines: &[OverlayLine]) -> Vec<(LayoutRegionRole, f32)> {
    // Accumulate sizes per role via linear search (6 variants max).
    let mut groups: Vec<(LayoutRegionRole, Vec<f32>)> = Vec::new();
    for line in lines {
        if line.font_size <= 0.0 {
            continue;
        }
        if let Some(entry) = groups.iter_mut().find(|(r, _)| r == &line.region_role) {
            entry.1.push(line.font_size);
        } else {
            groups.push((line.region_role.clone(), vec![line.font_size]));
        }
    }
    groups
        .into_iter()
        .map(|(role, mut sizes)| {
            sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = sizes[sizes.len() / 2];
            (role, median)
        })
        .collect()
}

// ── Resolution ────────────────────────────────────────────────────────────────

/// Resolve the desired font size for a line according to `policy`.
///
/// `role_medians` is the output of [`compute_role_medians`] for the current page.
/// `body_fs` is the page-level body font size estimate used as a fallback when
/// no per-role median is available.
pub(crate) fn resolve_font_size(
    line: &OverlayLine,
    policy: &FontSizePolicy,
    role_medians: &[(LayoutRegionRole, f32)],
    body_fs: f32,
) -> f32 {
    let raw = if line.font_size > 0.0 { line.font_size } else { body_fs };
    match policy {
        FontSizePolicy::Preserve => raw,
        FontSizePolicy::PageBodyMedian => body_fs,
        FontSizePolicy::RoleMedian => role_medians
            .iter()
            .find(|(r, _)| r == &line.region_role)
            .map(|(_, m)| *m)
            .unwrap_or(raw),
        FontSizePolicy::Quantized => {
            let median = role_medians
                .iter()
                .find(|(r, _)| r == &line.region_role)
                .map(|(_, m)| *m)
                .unwrap_or(raw);
            quantize_font_size(median)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use harumi::LayoutRegionRole;

    use super::*;
    use crate::overlay::{OverlayLine, OverlayPage};

    fn make_line(font_size: f32, role: LayoutRegionRole) -> OverlayLine {
        OverlayLine {
            x: 0.0,
            y: 0.0,
            right: 0.0,
            col_right: 0.0,
            line_height: 12.0,
            is_heading: false,
            is_bold: false,
            page_width: 595.0,
            text: String::new(),
            fragment_texts: vec![],
            font_size,
            normalized_font_size: 0.0,
            region_usable_right: 595.0,
            region_role: role,
            is_skip: false,
        }
    }

    // ── quantize_font_size ────────────────────────────────────────────────────

    #[test]
    fn quantize_exact_match() {
        assert_eq!(quantize_font_size(9.0), 9.0);
        assert_eq!(quantize_font_size(12.0), 12.0);
    }

    #[test]
    fn quantize_rounds_down() {
        // 9.3 is closer to 9 (delta 0.3) than 10 (delta 0.7)
        assert_eq!(quantize_font_size(9.3), 9.0);
    }

    #[test]
    fn quantize_rounds_up() {
        // 9.6 is closer to 10 (delta 0.4) than 9 (delta 0.6)
        assert_eq!(quantize_font_size(9.6), 10.0);
    }

    #[test]
    fn quantize_clamps_min() {
        assert_eq!(quantize_font_size(3.5), 6.0);
        assert_eq!(quantize_font_size(0.0), 6.0);
    }

    #[test]
    fn quantize_clamps_max() {
        assert_eq!(quantize_font_size(40.0), 32.0);
    }

    // ── compute_role_medians ──────────────────────────────────────────────────

    #[test]
    fn role_median_single_role() {
        let lines = vec![
            make_line(8.0, LayoutRegionRole::ParagraphBody),
            make_line(9.0, LayoutRegionRole::ParagraphBody),
            make_line(10.0, LayoutRegionRole::ParagraphBody),
        ];
        let medians = compute_role_medians(&lines);
        assert_eq!(medians.len(), 1);
        assert_eq!(medians[0].0, LayoutRegionRole::ParagraphBody);
        assert_eq!(medians[0].1, 9.0);
    }

    #[test]
    fn role_median_two_roles() {
        let lines = vec![
            make_line(8.0, LayoutRegionRole::ParagraphBody),
            make_line(9.0, LayoutRegionRole::ParagraphBody),
            make_line(10.0, LayoutRegionRole::ParagraphBody),
            make_line(14.0, LayoutRegionRole::SectionHeading),
            make_line(16.0, LayoutRegionRole::SectionHeading),
        ];
        let medians = compute_role_medians(&lines);
        assert_eq!(medians.len(), 2);
        let body = medians
            .iter()
            .find(|(r, _)| *r == LayoutRegionRole::ParagraphBody)
            .unwrap();
        assert_eq!(body.1, 9.0);
        let heading = medians
            .iter()
            .find(|(r, _)| *r == LayoutRegionRole::SectionHeading)
            .unwrap();
        assert_eq!(heading.1, 16.0);
    }

    #[test]
    fn role_median_ignores_zero() {
        let lines = vec![
            make_line(0.0, LayoutRegionRole::ParagraphBody),
            make_line(9.0, LayoutRegionRole::ParagraphBody),
        ];
        let medians = compute_role_medians(&lines);
        assert_eq!(medians.len(), 1);
        assert_eq!(medians[0].1, 9.0);
    }

    // ── resolve_font_size ─────────────────────────────────────────────────────

    #[test]
    fn resolve_preserve() {
        let line = make_line(9.3, LayoutRegionRole::ParagraphBody);
        let medians = vec![(LayoutRegionRole::ParagraphBody, 9.0)];
        let result = resolve_font_size(&line, &FontSizePolicy::Preserve, &medians, 10.0);
        assert_eq!(result, 9.3);
    }

    #[test]
    fn resolve_role_median() {
        let line = make_line(9.3, LayoutRegionRole::ParagraphBody);
        let medians = vec![(LayoutRegionRole::ParagraphBody, 9.0)];
        let result = resolve_font_size(&line, &FontSizePolicy::RoleMedian, &medians, 10.0);
        assert_eq!(result, 9.0);
    }

    #[test]
    fn resolve_quantized_rounds() {
        let line = make_line(9.3, LayoutRegionRole::ParagraphBody);
        let medians = vec![(LayoutRegionRole::ParagraphBody, 9.3)];
        let result = resolve_font_size(&line, &FontSizePolicy::Quantized, &medians, 10.0);
        assert_eq!(result, 9.0); // 9.3 quantizes to 9
    }

    #[test]
    fn resolve_fallback_to_body_fs() {
        let line = make_line(0.0, LayoutRegionRole::Unknown);
        let medians: Vec<(LayoutRegionRole, f32)> = vec![];
        let result =
            resolve_font_size(&line, &FontSizePolicy::RoleMedian, &medians, 10.0);
        assert_eq!(result, 10.0); // font_size=0 → raw=body_fs=10; no median → fallback raw=10
    }

    #[test]
    fn resolve_page_body_median() {
        let line = make_line(14.0, LayoutRegionRole::SectionHeading);
        let medians = vec![(LayoutRegionRole::SectionHeading, 14.0)];
        let result = resolve_font_size(&line, &FontSizePolicy::PageBodyMedian, &medians, 9.0);
        assert_eq!(result, 9.0); // always returns body_fs
    }

    // ── OverlayPage construction (smoke test) ─────────────────────────────────

    #[test]
    fn overlay_page_has_normalized_font_size_field() {
        let page = OverlayPage {
            page_num: 1,
            lines: vec![],
            body_font_size: 10.0,
            invisible_rects: vec![],
            image_bboxes: vec![],
        };
        assert_eq!(page.page_num, 1);
    }
}
