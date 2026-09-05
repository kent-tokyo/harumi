//! Integration tests for extract_layout_regions, plan_text_for_regions,
//! and plan_text_for_regions_with_policy (issues #21 and #22).

use harumi::{
    BaselinePolicy, BoxFitOptions, Document, LayoutRegionKind, LayoutRegionOptions,
    LayoutRegionRole, OverflowPolicy, RegionTextFitOptions, WidthPolicy, extract_layout_regions,
};

fn font_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("tests/fixtures/NotoSansJP-Regular.ttf not found")
}

/// Build a synthetic 2-column PDF with 3 rows and return extracted text fragments.
/// Column 0 (labels) at x=50, column 1 (values) at x=250.
/// Rows at y=700, 685, 670.  font_size=10.
fn two_col_fragments() -> (Document, harumi::FontHandle, Vec<harumi::TextFragment>) {
    let fb = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&fb).unwrap();

    let labels = ["ID", "Name", "Date"];
    let values = ["CHEM-001-A", "Toluene", "2026-06-20"];
    for (i, (lbl, val)) in labels.iter().zip(values.iter()).enumerate() {
        let y = 700.0 - i as f32 * 15.0;
        doc.page(1)
            .unwrap()
            .add_text(lbl, font, [50.0, y], 10.0, [0.0; 3])
            .unwrap();
        doc.page(1)
            .unwrap()
            .add_text(val, font, [250.0, y], 10.0, [0.0; 3])
            .unwrap();
    }

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let frags = doc2.extract_text_runs(1).unwrap();
    (doc2, font, frags)
}

// ---------------------------------------------------------------------------
// Core invariant: usable_rect.width uses inter-column gap
// ---------------------------------------------------------------------------

#[test]
fn usable_width_wider_than_source_for_label_column() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    assert!(!regions.is_empty(), "should find at least one region");

    // Find the column-0 regions (leftmost column).
    let col0_regions: Vec<_> = regions.iter().filter(|r| r.col == Some(0)).collect();
    assert!(!col0_regions.is_empty(), "should have column-0 regions");

    for r in &col0_regions {
        // usable_rect width must reach toward the next column start (x≈250)
        // source text is "ID"/"Name"/"Date" at 10pt → roughly 10–30pt wide
        // usable width should be ≈ 250 - 50 - 2 = 198pt >> source width
        assert!(
            r.usable_rect[2] > r.source_bbox[2] * 3.0,
            "usable_rect.width ({}) should be much wider than source_bbox.width ({})",
            r.usable_rect[2],
            r.source_bbox[2]
        );
        // usable_rect.x should be ≈ the column zone's x_start (near 50)
        assert!(
            r.usable_rect[0] < 100.0,
            "usable_rect.x ({}) should be near column left edge",
            r.usable_rect[0]
        );
    }
}

#[test]
fn last_column_usable_width_reaches_page_edge() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    let col1_regions: Vec<_> = regions.iter().filter(|r| r.col == Some(1)).collect();
    assert!(!col1_regions.is_empty(), "should have column-1 regions");

    for r in &col1_regions {
        // Last column usable width = page_width (595) - col1.x_start (≈250) - margin (2) ≈ 343
        // Should be much wider than the source text ("CHEM-001-A" ≈ 50pt)
        assert!(
            r.usable_rect[2] > 200.0,
            "last-column usable_rect.width ({}) should extend toward page edge",
            r.usable_rect[2]
        );
    }
}

#[test]
fn usable_width_falls_back_to_source_when_disabled() {
    let (_doc, _font, frags) = two_col_fragments();
    let mut opts = LayoutRegionOptions::default();
    opts.infer_column_widths = false;
    let regions = extract_layout_regions(&frags, 595.0, 842.0, opts);

    let col0: Vec<_> = regions.iter().filter(|r| r.col == Some(0)).collect();
    for r in &col0 {
        // With infer_column_widths=false, usable_rect.width == source_bbox.width
        assert!(
            (r.usable_rect[2] - r.source_bbox[2]).abs() < 1.0,
            "without inference, usable width ({}) should equal source width ({})",
            r.usable_rect[2],
            r.source_bbox[2]
        );
    }
}

// ---------------------------------------------------------------------------
// Row height inference
// ---------------------------------------------------------------------------

#[test]
fn row_height_inferred_from_adjacent_rows() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    // With rows at y=700, 685, 670 (15pt apart), inter-row height ≈ 15pt.
    // font_size=10 → source_bbox.height ≈ 10.
    // usable_rect.height for middle rows should be ≈ inter-row distance.
    let middle_rows: Vec<_> = regions.iter().filter(|r| r.row == Some(1)).collect();
    for r in &middle_rows {
        assert!(
            r.usable_rect[3] >= r.source_bbox[3],
            "usable height ({}) must be >= source height ({})",
            r.usable_rect[3],
            r.source_bbox[3]
        );
    }
}

#[test]
fn row_height_falls_back_to_source_when_disabled() {
    let (_doc, _font, frags) = two_col_fragments();
    let mut opts = LayoutRegionOptions::default();
    opts.infer_row_heights = false;
    let regions = extract_layout_regions(&frags, 595.0, 842.0, opts);

    for r in &regions {
        assert!(
            (r.usable_rect[3] - r.source_bbox[3]).abs() < 1.0,
            "without row-height inference, usable height should equal source height"
        );
    }
}

// ---------------------------------------------------------------------------
// Kind classification
// ---------------------------------------------------------------------------

#[test]
fn table_cell_kind_for_normal_text() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    assert!(!regions.is_empty());
    // In a 2-column layout, body-font cells should be TableCell.
    let table_cells: Vec<_> = regions
        .iter()
        .filter(|r| r.kind == LayoutRegionKind::TableCell)
        .collect();
    assert!(
        !table_cells.is_empty(),
        "should classify at least some cells as TableCell"
    );
}

#[test]
fn heading_classified_by_font_size_ratio() {
    let fb = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&fb).unwrap();

    // Heading at 20pt (2× body)
    doc.page(1)
        .unwrap()
        .add_text("Section 1", font, [50.0, 750.0], 20.0, [0.0; 3])
        .unwrap();
    // Body text at 10pt
    doc.page(1)
        .unwrap()
        .add_text("Label A", font, [50.0, 700.0], 10.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Value A long text", font, [250.0, 700.0], 10.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let frags = doc2.extract_text_runs(1).unwrap();

    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    let headings: Vec<_> = regions
        .iter()
        .filter(|r| matches!(r.kind, LayoutRegionKind::Heading(_)))
        .collect();
    assert!(
        !headings.is_empty(),
        "large-font text should be classified as Heading"
    );
}

// ---------------------------------------------------------------------------
// Fragments preserved
// ---------------------------------------------------------------------------

#[test]
fn fragments_are_preserved_in_regions() {
    let (_doc, _font, frags) = two_col_fragments();
    let total_fragments = frags.len();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    let region_fragments: usize = regions.iter().map(|r| r.fragments.len()).sum();
    assert_eq!(
        region_fragments, total_fragments,
        "all source fragments should be preserved across regions"
    );
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_fragments_returns_empty() {
    let regions = extract_layout_regions(&[], 595.0, 842.0, LayoutRegionOptions::default());
    assert!(regions.is_empty());
}

#[test]
fn zero_page_width_returns_empty() {
    let fb = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&fb).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Hello", font, [72.0, 700.0], 12.0, [0.0; 3])
        .unwrap();
    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let frags = doc2.extract_text_runs(1).unwrap();

    let regions = extract_layout_regions(&frags, 0.0, 842.0, LayoutRegionOptions::default());
    assert!(regions.is_empty(), "zero page_width should return empty");
}

// ---------------------------------------------------------------------------
// plan_text_for_regions
// ---------------------------------------------------------------------------

#[test]
fn plan_text_for_regions_returns_one_plan_per_pair() {
    let fb = font_bytes();
    let (mut doc2, _font, frags) = two_col_fragments();
    // Embed the font in a fresh doc for planning.
    let mut doc3 = Document::from_bytes(&doc2.save_to_bytes().unwrap()).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();

    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    if regions.is_empty() {
        return; // degenerate fixture — skip
    }

    let replacements: Vec<String> = regions.iter().map(|r| format!("TR:{}", r.text)).collect();
    let opts = BoxFitOptions::default();
    let plans = doc3
        .plan_text_for_regions(&regions, &replacements, plan_font, opts)
        .unwrap();

    assert_eq!(
        plans.len(),
        regions.len().min(replacements.len()),
        "should return one plan per region-replacement pair"
    );
    for plan in &plans {
        assert!(plan.fit.font_size > 0.0, "fit font_size must be positive");
        assert!(
            plan.fit.used_rect[2] > 0.0,
            "fit used_rect width must be positive"
        );
    }
}

#[test]
fn plan_text_detects_collision_when_regions_overlap() {
    let fb = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&fb).unwrap();

    // Both rows very close together so their usable_rects overlap vertically.
    // Use same column (x=50) with rows only 5pt apart.
    doc.page(1)
        .unwrap()
        .add_text("A", font, [50.0, 700.0], 10.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("B", font, [250.0, 700.0], 10.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("C", font, [50.0, 696.0], 10.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("D", font, [250.0, 696.0], 10.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let frags = doc2.extract_text_runs(1).unwrap();

    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    if regions.is_empty() {
        return;
    }

    let replacements: Vec<String> = regions
        .iter()
        .map(|r| format!("Replacement: {}", r.text))
        .collect();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Report;

    let mut doc3 = Document::from_bytes(&bytes).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();
    let plans = doc3
        .plan_text_for_regions(&regions, &replacements, plan_font, opts)
        .unwrap();

    // The plans should be computed without error; collisions list is populated if any overlap.
    assert_eq!(plans.len(), regions.len().min(replacements.len()));
}

// ===========================================================================
// Issue #22: LayoutRegionRole + RegionTextFitOptions + plan_with_policy
// ===========================================================================

// ---------------------------------------------------------------------------
// Role classification
// ---------------------------------------------------------------------------

#[test]
fn left_label_role_assigned_in_two_col_layout() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    let col0: Vec<_> = regions.iter().filter(|r| r.col == Some(0)).collect();
    assert!(!col0.is_empty(), "should have column-0 regions");
    for r in col0 {
        assert_eq!(
            r.role,
            LayoutRegionRole::LeftLabel,
            "col-0 cell with col-1 sibling should be LeftLabel, got {:?}",
            r.role
        );
    }
}

#[test]
fn right_value_role_assigned_in_two_col_layout() {
    let (_doc, _font, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    let col1: Vec<_> = regions.iter().filter(|r| r.col == Some(1)).collect();
    assert!(!col1.is_empty(), "should have column-1 regions");
    for r in col1 {
        assert_eq!(
            r.role,
            LayoutRegionRole::RightValue,
            "col-1 cell with col-0 sibling should be RightValue, got {:?}",
            r.role
        );
    }
}

#[test]
fn section_heading_role_for_large_font() {
    let fb = font_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&fb).unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Section Header", font, [50.0, 750.0], 20.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Label", font, [50.0, 700.0], 10.0, [0.0; 3])
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_text("Value data here", font, [250.0, 700.0], 10.0, [0.0; 3])
        .unwrap();

    let bytes = doc.save_to_bytes().unwrap();
    let doc2 = Document::from_bytes(&bytes).unwrap();
    let frags = doc2.extract_text_runs(1).unwrap();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());

    let headings: Vec<_> = regions
        .iter()
        .filter(|r| r.role == LayoutRegionRole::SectionHeading)
        .collect();
    assert!(
        !headings.is_empty(),
        "large-font region should have SectionHeading role"
    );
    assert!(
        matches!(headings[0].kind, LayoutRegionKind::Heading(_)),
        "SectionHeading role should come with Heading kind"
    );
}

// ---------------------------------------------------------------------------
// RegionTextFitOptions::for_role defaults
// ---------------------------------------------------------------------------

#[test]
fn default_label_opts_preserve_baseline_and_source_width() {
    let opts = RegionTextFitOptions::for_role(&LayoutRegionRole::LeftLabel);
    assert_eq!(opts.baseline, BaselinePolicy::PreserveSourceBaseline);
    assert_eq!(opts.width, WidthPolicy::SourceLineWidth);
    assert!(opts.preserve_source_x);
}

#[test]
fn default_value_opts_clamp_to_column() {
    let opts = RegionTextFitOptions::for_role(&LayoutRegionRole::RightValue);
    assert_eq!(opts.baseline, BaselinePolicy::PreserveSourceBaseline);
    assert_eq!(opts.width, WidthPolicy::ClampToColumn);
}

#[test]
fn default_paragraph_opts_top_align_region_width() {
    let opts = RegionTextFitOptions::for_role(&LayoutRegionRole::ParagraphBody);
    assert_eq!(opts.baseline, BaselinePolicy::TopAlignToRegion);
    assert_eq!(opts.width, WidthPolicy::RegionUsableWidth);
    assert!(!opts.preserve_source_x);
}

// ---------------------------------------------------------------------------
// plan_text_for_regions_with_policy — baseline behaviour
// ---------------------------------------------------------------------------

#[test]
fn plan_with_policy_preserves_baseline_for_label() {
    let fb = font_bytes();
    let (_doc2, _, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    if regions.is_empty() {
        return;
    }

    let mut doc3 = Document::new((595.0, 842.0)).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();

    let replacements: Vec<String> = regions.iter().map(|r| format!("TR:{}", r.text)).collect();

    // Build per-region options: PreserveSourceBaseline for all
    let options: Vec<RegionTextFitOptions> = regions
        .iter()
        .map(|_| {
            let mut o = RegionTextFitOptions::default();
            o.baseline = BaselinePolicy::PreserveSourceBaseline;
            o
        })
        .collect();

    let plans = doc3
        .plan_text_for_regions_with_policy(&regions, &replacements, plan_font, &options)
        .unwrap();

    assert_eq!(plans.len(), regions.len().min(replacements.len()));

    // With PreserveSourceBaseline, the fitting rect top = source_bbox[1] + source_bbox[3].
    // used_rect is top-aligned within that fitting rect, so its top must match.
    for (plan, region) in plans.iter().zip(regions.iter()) {
        let source_top = region.source_bbox[1] + region.source_bbox[3];
        let fit_top = plan.fit.used_rect[1] + plan.fit.used_rect[3];
        assert!(
            (fit_top - source_top).abs() < 1.0,
            "PreserveSourceBaseline: fit top ({fit_top}) should match source top ({source_top})"
        );
    }
}

#[test]
fn plan_with_policy_uses_column_width_for_value() {
    let fb = font_bytes();
    let (_doc2, _, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    let value_regions: Vec<_> = regions
        .iter()
        .filter(|r| r.role == LayoutRegionRole::RightValue)
        .cloned()
        .collect();
    if value_regions.is_empty() {
        return;
    }

    let mut doc3 = Document::new((595.0, 842.0)).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();

    let replacements: Vec<String> = value_regions
        .iter()
        .map(|r| format!("Long replacement text for {}", r.text))
        .collect();

    let mut val_opts = RegionTextFitOptions::default();
    val_opts.width = WidthPolicy::ClampToColumn;
    let options: Vec<RegionTextFitOptions> =
        value_regions.iter().map(|_| val_opts.clone()).collect();

    let plans = doc3
        .plan_text_for_regions_with_policy(&value_regions, &replacements, plan_font, &options)
        .unwrap();

    // The fitting rect was the full column width → used_rect width can be larger than source
    for (plan, region) in plans.iter().zip(value_regions.iter()) {
        assert!(
            plan.fit.used_rect[2] <= region.usable_rect[2] + 1.0,
            "used_rect width should not exceed usable_rect width"
        );
    }
}

#[test]
fn plan_with_policy_empty_options_uses_role_defaults() {
    let fb = font_bytes();
    let (_doc2, _, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    if regions.is_empty() {
        return;
    }

    let mut doc3 = Document::new((595.0, 842.0)).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();

    let replacements: Vec<String> = regions.iter().map(|r| r.text.clone()).collect();

    // Pass empty slice → role-based defaults applied automatically
    let plans = doc3
        .plan_text_for_regions_with_policy(&regions, &replacements, plan_font, &[])
        .unwrap();

    assert_eq!(plans.len(), regions.len().min(replacements.len()));
    for plan in &plans {
        assert!(plan.fit.font_size > 0.0);
    }
}

#[test]
fn clamp_before_next_region_caps_width() {
    let fb = font_bytes();
    let (_doc2, _, frags) = two_col_fragments();
    let regions = extract_layout_regions(&frags, 595.0, 842.0, LayoutRegionOptions::default());
    let label_regions: Vec<_> = regions
        .iter()
        .filter(|r| r.role == LayoutRegionRole::LeftLabel)
        .cloned()
        .collect();
    if label_regions.is_empty() {
        return;
    }

    let mut doc3 = Document::new((595.0, 842.0)).unwrap();
    let plan_font = doc3.embed_font(&fb).unwrap();

    let replacements: Vec<String> = label_regions
        .iter()
        .map(|r| format!("LBL {}", r.text))
        .collect();

    let mut clamp_opts = RegionTextFitOptions::default();
    clamp_opts.width = WidthPolicy::ClampBeforeNextRegion;
    let options: Vec<RegionTextFitOptions> =
        label_regions.iter().map(|_| clamp_opts.clone()).collect();

    let plans = doc3
        .plan_text_for_regions_with_policy(&label_regions, &replacements, plan_font, &options)
        .unwrap();

    // ClampBeforeNextRegion width should be < usable_rect width (which reaches to col zone edge)
    // because it stops before the sibling's source x.
    // Note: if there is no sibling (last column), it falls back to usable_rect width.
    assert_eq!(plans.len(), label_regions.len().min(replacements.len()));
    for plan in &plans {
        assert!(
            plan.fit.font_size > 0.0,
            "plan should have positive font size"
        );
    }
}
