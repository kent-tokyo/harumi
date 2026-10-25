#![cfg(feature = "svg")]

use harumi::{Document, SvgOptions, svg_to_pdf, svg_to_pdf_with_options};

const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

#[test]
fn converts_vector_chemistry_subset_to_one_page() {
    let svg = br##"<svg width="240" height="120" viewBox="0 0 240 120">
      <g transform="translate(10 10)" stroke="#111111" fill="none" stroke-width="2">
        <line x1="10" y1="40" x2="80" y2="40" stroke-dasharray="4 2"/>
        <polygon points="80,40 70,34 70,46" fill="#111111"/>
        <circle cx="40" cy="80" r="12" fill="#ffcc00" stroke="#111111"/>
      </g>
      <text x="100" y="70" font-size="16" fill="#0000ff">NH4+</text>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("SVG should convert");
    let document = Document::from_bytes(&bytes).expect("output should be a PDF");
    assert_eq!(document.page_count(), 1);
    assert!(bytes.windows(2).any(|window| window == b" m"));
    assert!(bytes.windows(2).any(|window| window == b" l"));
}

#[test]
fn rejects_malformed_and_unsupported_svg() {
    let malformed = svg_to_pdf(b"<svg>", FONT).expect_err("malformed SVG must fail");
    assert!(malformed.to_string().contains("malformed SVG"));

    let unsupported = svg_to_pdf(
        br#"<svg width="10" height="10"><image href="x"/></svg>"#,
        FONT,
    )
    .expect_err("unsupported SVG element must fail");
    assert!(unsupported.to_string().contains("unsupported SVG element"));
}

#[test]
fn ignores_metadata_subtrees_during_rendering() {
    let svg = br#"<svg width="80" height="40">
      <metadata><smiles>CCO</smiles><unsupported data="anything"/></metadata>
      <line x1="10" y1="20" x2="70" y2="20" stroke="black"/>
    </svg>"#;
    let bytes = svg_to_pdf(svg, FONT).expect("metadata must not affect conversion");
    assert_eq!(
        Document::from_bytes(&bytes)
            .expect("output should be a PDF")
            .page_count(),
        1
    );
}

#[test]
fn geometry_only_svg_does_not_require_or_embed_a_font() {
    let svg = br#"<svg width="120" height="80">
      <metadata><text>metadata is not rendered</text></metadata>
      <path d="M 10 40 Q 30 10 50 40 A 12 8 0 0 1 80 40" fill="none" stroke="black"/>
      <ellipse cx="100" cy="40" rx="12" ry="8" fill="blue"/>
    </svg>"#;
    let bytes = svg_to_pdf_with_options(
        svg,
        SvgOptions {
            font_bytes: &[],
            page_size: None,
        },
    )
    .expect("geometry-only SVG should not require a font");
    assert!(!bytes.windows(5).any(|window| window == b"/Font"));
    assert!(Document::from_bytes(&bytes).is_ok());
}

#[test]
fn rendered_svg_text_still_requires_a_font() {
    let svg = br#"<svg width="80" height="40"><text x="5" y="20">text</text></svg>"#;
    let error = svg_to_pdf_with_options(
        svg,
        SvgOptions {
            font_bytes: &[],
            page_size: None,
        },
    )
    .expect_err("rendered text must require a font");
    assert!(error.to_string().contains("requires non-empty font_bytes"));
}

#[test]
fn supports_horizontal_vertical_closed_paths_and_relative_numbers() {
    let svg = br##"<svg width="100" height="100">
      <path d="M 10 10 H 90 V 90 h -80 z" fill="#00aa00"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("H/V/Z path should convert");
    assert!(bytes.windows(2).any(|window| window == b"h\n"));
}

#[test]
fn supports_curves_arcs_and_ellipses() {
    let svg = br##"<svg width="200" height="120">
      <path d="M 10 60 Q 30 10 50 60 T 90 60 C 110 10 140 10 160 60 S 180 90 190 60" fill="none" stroke="#000"/>
      <path d="M 20 90 A 20 12 25 0 1 70 90" fill="none" stroke="#f00"/>
      <ellipse cx="130" cy="90" rx="24" ry="12" fill="#0a0" stroke="#00f" stroke-width="2"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("curves, arcs, and ellipses should convert");
    let document = lopdf::Document::load_mem(&bytes).expect("output should be a PDF");
    let page_id = *document.get_pages().get(&1).expect("page 1 should exist");
    let content_bytes = document.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content_bytes);
    assert!(content.matches(" l\n").count() > 20);
    assert!(content.contains("0.0000 0.6667 0.0000 rg"));
}

#[test]
fn rejects_empty_transform_arguments_without_panicking() {
    let translate = br#"<svg width="10" height="10"><g transform="translate()"/></svg>"#;
    let scale = br#"<svg width="10" height="10"><g transform="scale()"/></svg>"#;
    assert!(svg_to_pdf(translate, FONT).is_err());
    assert!(svg_to_pdf(scale, FONT).is_err());
}

#[test]
fn applies_affine_transforms_to_shapes_and_text() {
    let svg = br##"<svg width="120" height="120">
      <g transform="matrix(0 1 -1 0 100 0)" fill="none" stroke="#000000">
        <rect x="10" y="10" width="30" height="20"/>
        <circle cx="60" cy="30" r="10"/>
        <text x="20" y="80" fill="#000000">A</text>
      </g>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("affine transform should convert");
    let document = Document::from_bytes(&bytes).expect("output should be a PDF");
    assert_eq!(document.page_count(), 1);
}

#[test]
fn honors_text_anchor_and_dominant_baseline() {
    let svg = br##"<svg width="80" height="80" viewBox="-20 -20 80 80">
      <text x="0" y="0" font-size="12" text-anchor="middle" dominant-baseline="central" fill="#3050F8">H4N+</text>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("anchored text should convert");
    let document = Document::from_bytes(&bytes).expect("output should be a PDF");
    assert_eq!(document.page_count(), 1);
}

#[test]
fn accepts_three_digit_hex_colors_for_fill_and_stroke() {
    let svg = br##"<svg width="80" height="40">
      <rect x="5" y="5" width="20" height="20" fill="#333" stroke="#abc"/>
      <line x1="0" y1="30" x2="80" y2="30" stroke="#f0a"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("short SVG colors should convert");
    assert!(Document::from_bytes(&bytes).is_ok());
}

#[test]
fn accepts_css_named_and_rgb_colors() {
    let svg = br#"<svg width="80" height="40">
      <rect x="0" y="0" width="20" height="20" fill="yellow"/>
      <rect x="20" y="0" width="20" height="20" fill="rgb(255, 165, 0)"/>
      <line x1="0" y1="30" x2="80" y2="30" stroke="rgb(0%, 50%, 100%)"/>
    </svg>"#;
    let bytes = svg_to_pdf(svg, FONT).expect("CSS colors should convert");
    let document = lopdf::Document::load_mem(&bytes).expect("output should be a PDF");
    let page_id = *document.get_pages().get(&1).expect("page 1 should exist");
    let content_bytes = document.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content_bytes);
    assert!(content.contains("1.0000 1.0000 0.0000 rg"));
    assert!(content.contains("1.0000 0.6471 0.0000 rg"));
    assert!(content.contains("0.0000 0.5000 1.0000 RG"));
}

#[test]
fn inline_style_overrides_presentation_attributes_and_supports_current_color() {
    let svg = br##"<svg width="80" height="40">
      <g color="#00ff00">
        <rect x="0" y="0" width="20" height="20" fill="#ff0000" style="fill: currentColor"/>
      </g>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("currentColor should convert");
    let document = lopdf::Document::load_mem(&bytes).expect("output should be a PDF");
    let page_id = *document.get_pages().get(&1).expect("page 1 should exist");
    let content_bytes = document.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content_bytes);
    assert!(content.contains("0.0000 1.0000 0.0000 rg"));
    assert!(!content.contains("1.0000 0.0000 0.0000 rg"));
}

#[test]
fn renders_referenced_markers_and_honors_none() {
    let svg = br##"<svg width="120" height="40">
      <defs><marker id="tip" refX="5" refY="2">
        <polygon points="0 0, 5 2, 0 4" fill="#f00"/>
      </marker></defs>
      <line x1="10" y1="10" x2="100" y2="10" stroke="#000" marker-end="url(#tip)"/>
      <line x1="10" y1="30" x2="100" y2="30" stroke="#000" marker-end="none"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("marker definitions should convert");
    let without_marker = br##"<svg width="120" height="40">
      <line x1="10" y1="10" x2="100" y2="10" stroke="#000"/>
      <line x1="10" y1="30" x2="100" y2="30" stroke="#000" marker-end="none"/>
    </svg>"##;
    let plain = svg_to_pdf(without_marker, FONT).expect("unmarked lines should convert");
    assert_ne!(bytes, plain);
}

#[test]
fn keeps_path_subpaths_separate_after_move_to() {
    let svg = br##"<svg width="100" height="40">
      <path d="M 10 20 L 30 20 M 70 20 L 90 20" fill="none" stroke="#000"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("multiple path subpaths should convert");
    let document = lopdf::Document::load_mem(&bytes).expect("output should be a PDF");
    let page_id = *document.get_pages().get(&1).expect("page 1 should exist");
    let content_bytes = document.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content_bytes);
    assert_eq!(content.matches(" m\n").count(), 2);
    assert_eq!(content.matches("\nS\n").count(), 2);
}

#[test]
fn preserves_distinct_polygon_fill_and_stroke_colors() {
    let svg = br##"<svg width="100" height="100">
      <polygon points="20,20 80,20 50,80" fill="#ff0000" stroke="#0000ff" stroke-width="8"/>
    </svg>"##;
    let bytes = svg_to_pdf(svg, FONT).expect("polygon colors should convert");
    let document = lopdf::Document::load_mem(&bytes).expect("output should be a PDF");
    let page_id = *document.get_pages().get(&1).expect("page 1 should exist");
    let content_bytes = document.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content_bytes);

    assert!(content.contains("1.0000 0.0000 0.0000 rg"));
    assert!(content.contains("0.0000 0.0000 1.0000 RG"));
    assert!(content.contains("8.0000 w"));
}
