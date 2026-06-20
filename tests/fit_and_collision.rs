//! Integration tests for Document::fit_text_to_box, Document::measure_text,
//! detect_collisions (issue #20), classify_collisions (issue #23),
//! and layout quality primitives (issue #24).

use harumi::{
    BoxFitOptions, ClassifiedCollision, CollisionKind, Document, OverflowPolicy, PageFitSummary,
    PlacedBox, PlacementStatus, classify_collisions, detect_collisions,
};

fn noto_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/NotoSansJP-Regular.ttf")
        .expect("tests/fixtures/NotoSansJP-Regular.ttf not found")
}

fn doc_with_font() -> (Document, harumi::FontHandle) {
    let bytes = noto_bytes();
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(&bytes).unwrap();
    (doc, font)
}

// Helper: build BoxFitOptions with a given overflow policy (all other fields default).
fn opts_with_policy(policy: OverflowPolicy) -> BoxFitOptions {
    let mut opts = BoxFitOptions::default();
    opts.overflow = policy;
    opts
}

// ---------------------------------------------------------------------------
// measure_text
// ---------------------------------------------------------------------------

#[test]
fn measure_text_returns_nonzero() {
    let (doc, font) = doc_with_font();
    let w = doc.measure_text("Hello", font, 12.0).unwrap();
    assert!(w > 0.0, "expected positive width, got {w}");
}

#[test]
fn measure_text_scales_with_font_size() {
    let (doc, font) = doc_with_font();
    let w12 = doc.measure_text("Test", font, 12.0).unwrap();
    let w24 = doc.measure_text("Test", font, 24.0).unwrap();
    assert!(
        (w24 - w12 * 2.0).abs() < 0.5,
        "width should double when font_size doubles: w12={w12} w24={w24}"
    );
}

// ---------------------------------------------------------------------------
// fit_text_to_box — Report policy
// ---------------------------------------------------------------------------

#[test]
fn report_no_overflow_when_text_fits() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Report);
    let result = doc.fit_text_to_box("Hi", font, [0.0, 0.0, 200.0, 100.0], 12.0, opts).unwrap();
    assert!(!result.overflow_horizontal, "should not overflow horizontally");
    assert!(!result.overflow_vertical, "should not overflow vertically");
    assert_eq!(result.font_size, 12.0);
}

#[test]
fn report_horizontal_overflow() {
    let (doc, font) = doc_with_font();
    let mut opts = opts_with_policy(OverflowPolicy::Report);
    opts.wrap = false;
    let result = doc
        .fit_text_to_box(
            "This is a very long text that absolutely will not fit in twenty points",
            font,
            [0.0, 0.0, 20.0, 200.0],
            12.0,
            opts,
        )
        .unwrap();
    assert!(result.overflow_horizontal, "should overflow horizontally");
}

#[test]
fn report_vertical_overflow() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Report);
    let long = "word ".repeat(40);
    let result = doc
        .fit_text_to_box(long.trim(), font, [0.0, 0.0, 200.0, 10.0], 12.0, opts)
        .unwrap();
    assert!(result.overflow_vertical, "should overflow vertically");
}

// ---------------------------------------------------------------------------
// fit_text_to_box — Shrink policy
// ---------------------------------------------------------------------------

#[test]
fn shrink_policy_reduces_font_for_wide_text() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Shrink);
    let result = doc
        .fit_text_to_box("WIDE TEXT", font, [0.0, 0.0, 60.0, 100.0], 24.0, opts)
        .unwrap();
    assert!(
        result.font_size < 24.0,
        "font should be reduced; got {}",
        result.font_size
    );
    assert!(!result.overflow_horizontal, "should not overflow H after shrink");
    assert_eq!(result.lines.len(), 1, "Shrink = no wrap, single line");
}

#[test]
fn shrink_policy_respects_min_font_size() {
    let (doc, font) = doc_with_font();
    let mut opts = opts_with_policy(OverflowPolicy::Shrink);
    opts.min_font_size = 10.0;
    let result = doc
        .fit_text_to_box("OVERFLOW", font, [0.0, 0.0, 1.0, 100.0], 24.0, opts)
        .unwrap();
    assert!(
        result.font_size >= 10.0,
        "font should not go below min_font_size; got {}",
        result.font_size
    );
}

// ---------------------------------------------------------------------------
// fit_text_to_box — WrapThenShrink policy
// ---------------------------------------------------------------------------

#[test]
fn wrap_then_shrink_fits_height() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::WrapThenShrink);
    let text = "one two three four five six seven eight nine ten";
    let result = doc
        .fit_text_to_box(text, font, [0.0, 0.0, 200.0, 30.0], 12.0, opts)
        .unwrap();
    assert!(
        !result.overflow_vertical,
        "WrapThenShrink should eliminate vertical overflow; used_h={}",
        result.used_rect[3]
    );
}

// ---------------------------------------------------------------------------
// fit_text_to_box — Truncate policy
// ---------------------------------------------------------------------------

#[test]
fn truncate_drops_excess_lines() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Truncate);
    // At 12 pt, line_height = 14.4 pt. A 30 pt rect holds floor(30/14.4) = 2 lines.
    let text = "one two three four five six seven eight nine ten eleven twelve";
    let result = doc
        .fit_text_to_box(text, font, [0.0, 0.0, 100.0, 30.0], 12.0, opts)
        .unwrap();
    let max_lines = (30.0_f32 / (12.0 * 1.2)).floor() as usize;
    assert!(
        result.lines.len() <= max_lines.max(1),
        "expected at most {} lines, got {}",
        max_lines,
        result.lines.len()
    );
    assert!(!result.overflow_vertical, "truncated result should not overflow vertically");
}

#[test]
fn truncate_respects_max_lines() {
    let (doc, font) = doc_with_font();
    let mut opts = opts_with_policy(OverflowPolicy::Truncate);
    opts.max_lines = Some(1);
    let text = "one two three four five six seven";
    let result = doc
        .fit_text_to_box(text, font, [0.0, 0.0, 100.0, 200.0], 12.0, opts)
        .unwrap();
    assert_eq!(result.lines.len(), 1, "max_lines=1 should yield exactly 1 line");
}

// ---------------------------------------------------------------------------
// fit_text_to_box — CJK
// ---------------------------------------------------------------------------

#[test]
fn cjk_wraps_at_char_boundaries() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Report);
    let cjk = "東京都千代田区大手町一丁目二番地三号";
    let result = doc
        .fit_text_to_box(cjk, font, [0.0, 0.0, 100.0, 500.0], 12.0, opts)
        .unwrap();
    assert!(
        result.lines.len() > 1,
        "CJK text in narrow box should wrap into multiple lines; got {} line(s)",
        result.lines.len()
    );
    let rejoined: String = result.lines.concat();
    assert_eq!(rejoined, cjk, "no characters should be lost after wrapping");
}

// ---------------------------------------------------------------------------
// fit_text_to_box — used_rect geometry
// ---------------------------------------------------------------------------

#[test]
fn used_rect_top_aligned_in_provided_rect() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::Report);
    let rect = [10.0_f32, 20.0, 200.0, 100.0];
    let result = doc.fit_text_to_box("Hello world", font, rect, 12.0, opts).unwrap();
    let [ux, uy, uw, uh] = result.used_rect;
    assert!((ux - rect[0]).abs() < 0.1, "used_rect x should equal rect x; ux={ux}");
    assert!(
        ((uy + uh) - (rect[1] + rect[3])).abs() < 0.1,
        "top of used_rect ({}) should equal top of rect ({})",
        uy + uh,
        rect[1] + rect[3]
    );
    assert!(uw <= rect[2] + 0.1, "used_rect width ({uw}) should not exceed rect width");
}

// ---------------------------------------------------------------------------
// detect_collisions
// ---------------------------------------------------------------------------

#[test]
fn detect_collisions_finds_overlap() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
        PlacedBox::new([80.0, 0.0, 100.0, 50.0]), // overlaps first by 20 pt width
        PlacedBox::new([200.0, 0.0, 50.0, 50.0]), // no overlap
    ];
    let collisions = detect_collisions(&boxes);
    assert_eq!(collisions.len(), 1, "expected 1 collision");
    let c = &collisions[0];
    assert_eq!(c.index_a, 0);
    assert_eq!(c.index_b, 1);
    let [ox, oy, ow, oh] = c.overlap_rect;
    assert!((ox - 80.0).abs() < 0.1, "overlap x should be 80; got {ox}");
    assert!((oy - 0.0).abs() < 0.1, "overlap y should be 0; got {oy}");
    assert!((ow - 20.0).abs() < 0.1, "overlap width should be 20; got {ow}");
    assert!((oh - 50.0).abs() < 0.1, "overlap height should be 50; got {oh}");
}

#[test]
fn detect_collisions_adjacent_boxes_no_overlap() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
        PlacedBox::new([100.0, 0.0, 100.0, 50.0]),
    ];
    let collisions = detect_collisions(&boxes);
    assert!(collisions.is_empty(), "adjacent boxes should not collide");
}

#[test]
fn detect_collisions_no_boxes_returns_empty() {
    assert!(detect_collisions(&[]).is_empty());
}

#[test]
fn detect_collisions_all_overlapping() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 50.0, 50.0]),
        PlacedBox::new([0.0, 0.0, 50.0, 50.0]),
        PlacedBox::new([0.0, 0.0, 50.0, 50.0]),
    ];
    let collisions = detect_collisions(&boxes);
    assert_eq!(collisions.len(), 3, "three identical boxes should yield 3 pairwise collisions");
}

#[test]
fn detect_collisions_vertical_stack_no_overlap() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 100.0, 30.0]),
        PlacedBox::new([0.0, 30.0, 100.0, 30.0]),
        PlacedBox::new([0.0, 60.0, 100.0, 30.0]),
    ];
    let collisions = detect_collisions(&boxes);
    assert!(
        collisions.is_empty(),
        "vertically stacked non-overlapping boxes should have no collisions"
    );
}

// ---------------------------------------------------------------------------
// Collision + fit_text_to_box integration
// ---------------------------------------------------------------------------

#[test]
fn fit_results_can_be_checked_for_collision() {
    let (doc, font) = doc_with_font();
    let opts = opts_with_policy(OverflowPolicy::WrapThenShrink);

    // Identical rects: both texts land at the same position → used_rects always overlap.
    let rect = [0.0_f32, 500.0, 200.0, 50.0];

    let result_a = doc.fit_text_to_box("Label A text", font, rect, 10.0, opts.clone()).unwrap();
    let result_b = doc.fit_text_to_box("Label B text", font, rect, 10.0, opts).unwrap();

    // Both used_rects share the same top-left origin (identical input rect), so they overlap.
    let placed = vec![
        PlacedBox::new(result_a.used_rect),
        PlacedBox::new(result_b.used_rect),
    ];
    let collisions = detect_collisions(&placed);
    assert!(
        !collisions.is_empty(),
        "two texts placed in the same rect must collide; used_rects: {:?} {:?}",
        result_a.used_rect,
        result_b.used_rect
    );
}

// ---------------------------------------------------------------------------
// classify_collisions — integration smoke (public API, issue #23)
// ---------------------------------------------------------------------------

#[test]
fn classify_collisions_public_api_smoke() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
        PlacedBox::new([80.0, 0.0, 100.0, 50.0]),
    ];
    let raw = detect_collisions(&boxes);
    assert_eq!(raw.len(), 1);

    // Empty regions slice: both indices are out of range → Unknown
    let classified: Vec<ClassifiedCollision> = classify_collisions(&[], &raw);
    assert_eq!(classified.len(), 1);
    assert_eq!(classified[0].kind, CollisionKind::Unknown);
    assert!(classified[0].region_a.is_none());
    assert!(classified[0].region_b.is_none());
    // Raw collision is still accessible
    assert_eq!(classified[0].collision.index_a, 0);
    assert_eq!(classified[0].collision.index_b, 1);
}

// ---------------------------------------------------------------------------
// overlap_area (issue #24)
// ---------------------------------------------------------------------------

#[test]
fn detect_collisions_reports_overlap_area() {
    // Box A: [0, 0, 100, 50]  Box B: [80, 10, 100, 50]
    // Intersection x: [80, 100] = 20 pt  y: [10, 50] = 40 pt  area = 800 pt²
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 100.0, 50.0]),
        PlacedBox::new([80.0, 10.0, 100.0, 50.0]),
    ];
    let collisions = detect_collisions(&boxes);
    assert_eq!(collisions.len(), 1);
    let area = collisions[0].overlap_area;
    assert!((area - 800.0).abs() < 0.5, "expected ~800 pt², got {area}");
    // overlap_area must equal overlap_rect[2] * overlap_rect[3]
    let [_, _, ow, oh] = collisions[0].overlap_rect;
    assert!((area - ow * oh).abs() < 1e-4);
}

#[test]
fn no_collision_has_zero_overlap_area() {
    let boxes = vec![
        PlacedBox::new([0.0, 0.0, 50.0, 50.0]),
        PlacedBox::new([100.0, 0.0, 50.0, 50.0]),
    ];
    assert!(detect_collisions(&boxes).is_empty());
}

// ---------------------------------------------------------------------------
// PlacementStatus (issue #24)
// ---------------------------------------------------------------------------

#[test]
fn placement_status_ok_when_text_fits() {
    let (doc, font) = doc_with_font();
    let result = doc
        .fit_text_to_box("Hi", font, [0.0, 0.0, 500.0, 100.0], 12.0, BoxFitOptions::default())
        .unwrap();
    assert_eq!(result.status, PlacementStatus::Ok);
}

#[test]
fn placement_status_shrunk_when_font_reduced() {
    let (doc, font) = doc_with_font();
    // Very wide single-line text forced into a tiny box → Shrink policy should shrink
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Shrink;
    opts.min_font_size = 1.0;
    // "A" repeated enough times to force shrinking at 48pt in a 50pt wide box
    let text = "A".repeat(50);
    let result = doc.fit_text_to_box(&text, font, [0.0, 0.0, 50.0, 100.0], 48.0, opts).unwrap();
    assert!(
        matches!(result.status, PlacementStatus::Shrunk | PlacementStatus::ShrunkToMin),
        "expected Shrunk or ShrunkToMin, got {:?}",
        result.status
    );
    assert!(result.font_size < 48.0);
}

#[test]
fn placement_status_shrunk_to_min_at_floor() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Shrink;
    opts.min_font_size = 20.0;
    // Use a very wide text in a tiny box so shrinking must stop at min_font_size
    let text = "WWWWWWWWWWWWWWWWWWWWWWWW";
    let result = doc.fit_text_to_box(text, font, [0.0, 0.0, 10.0, 100.0], 48.0, opts).unwrap();
    assert_eq!(result.status, PlacementStatus::ShrunkToMin);
    assert!((result.font_size - 20.0).abs() < 0.5);
}

#[test]
fn placement_status_overflow_with_report_policy() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Report;
    opts.wrap = false;
    let text = "A".repeat(200);
    let result = doc.fit_text_to_box(&text, font, [0.0, 0.0, 50.0, 100.0], 12.0, opts).unwrap();
    assert_eq!(result.status, PlacementStatus::Overflow);
    assert!(result.overflow_horizontal);
}

#[test]
fn placement_status_ok_with_report_policy_when_fits() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Report;
    let result =
        doc.fit_text_to_box("Hi", font, [0.0, 0.0, 500.0, 100.0], 12.0, opts).unwrap();
    assert_eq!(result.status, PlacementStatus::Ok);
}

#[test]
fn placement_status_truncated_when_lines_dropped() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Truncate;
    opts.max_lines = Some(1);
    // Wide text in a narrow box forces wrapping into multiple lines; max_lines=1 truncates.
    let text = "This is a long sentence that will definitely wrap into multiple lines when placed in a narrow box.";
    let result = doc
        .fit_text_to_box(text, font, [0.0, 0.0, 80.0, 1000.0], 12.0, opts)
        .unwrap();
    assert_eq!(result.status, PlacementStatus::Truncated, "lines: {:?}", result.lines);
    assert_eq!(result.lines.len(), 1);
}

// ---------------------------------------------------------------------------
// PageFitSummary (issue #24)
// ---------------------------------------------------------------------------

#[test]
fn page_fit_summary_empty_plans() {
    let summary = PageFitSummary::from_plans(&[]);
    assert_eq!(summary.overflow_count, 0);
    assert_eq!(summary.collision_count, 0);
    assert_eq!(summary.shrunk_count, 0);
    assert_eq!(summary.worst_overlap_area, 0.0);
    assert!(summary.worst_overlap_rect.is_none());
}

// ---------------------------------------------------------------------------
// debug overlay (issue #24, draw feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "draw")]
#[test]
fn debug_overlay_no_error_on_empty_plans() {
    use harumi::DebugOverlayOptions;
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    doc.page(1).unwrap().add_fit_debug_overlay(&[], DebugOverlayOptions::default()).unwrap();
    let bytes = doc.save_to_bytes().unwrap();
    assert!(!bytes.is_empty());
}

// ---------------------------------------------------------------------------
// Bug-fix regressions (from code-review + security-review, v1.13.0)
// ---------------------------------------------------------------------------

// Bug: Report + max_lines silently dropped lines → status stayed Ok.
#[test]
fn report_policy_with_max_lines_gives_truncated_status() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Report;
    opts.max_lines = Some(1);
    // Text that wraps to many lines in a narrow box.
    let text = "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa";
    let result =
        doc.fit_text_to_box(text, font, [0.0, 0.0, 80.0, 1000.0], 12.0, opts).unwrap();
    assert_eq!(
        result.status,
        PlacementStatus::Truncated,
        "Report + max_lines must report Truncated when lines are dropped; got {:?}",
        result.status
    );
    assert_eq!(result.lines.len(), 1);
}

// Bug: WrapThenShrink when initial_font_size == min_font_size but text overflows.
#[test]
fn wrap_then_shrink_at_min_font_gives_shrunk_to_min_when_overflow() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::WrapThenShrink;
    opts.min_font_size = 12.0;
    // Very tall wrapped text in a tiny box: can't shrink below 12pt, overflow expected.
    let text = "Line A Line B Line C Line D Line E Line F Line G Line H";
    let result =
        doc.fit_text_to_box(text, font, [0.0, 0.0, 80.0, 20.0], 12.0, opts).unwrap();
    // Should NOT be Ok when text overflows and we're already at the font-size floor.
    assert_ne!(
        result.status,
        PlacementStatus::Ok,
        "status must not be Ok when at min_font_size and text overflows"
    );
    assert_eq!(result.status, PlacementStatus::ShrunkToMin);
    assert!(result.overflow_vertical);
}

// Bug: Truncate with rh == 0 kept all lines and returned Ok because else-branch
// set max_by_height = lines.len() instead of 0.
#[test]
fn truncate_zero_height_rect_gives_truncated_status() {
    let (doc, font) = doc_with_font();
    let mut opts = BoxFitOptions::default();
    opts.overflow = OverflowPolicy::Truncate;
    // Multiple lines in a zero-height rect.
    let text = "Alpha Beta Gamma Delta Epsilon Zeta";
    let result =
        doc.fit_text_to_box(text, font, [0.0, 0.0, 80.0, 0.0], 12.0, opts).unwrap();
    // Zero-height box should produce Truncated (at least 1 line kept) and overflow.
    assert_eq!(
        result.status,
        PlacementStatus::Truncated,
        "Truncate with rh=0 must report Truncated; got {:?}",
        result.status
    );
    assert!(result.overflow_vertical);
}
