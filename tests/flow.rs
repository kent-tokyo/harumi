//! Integration tests for the `flow` feature.
//! Run with: cargo test --features flow

#![cfg(feature = "flow")]

use harumi::{
    Document, FlowDocument, FlowOptions, FlowTableCell, FlowTextAlignment, HeaderFooter,
    InlineSpan, Margins, TableCellAlignment, TableColumnWidths, TableOptions,
};

const NOTO: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");
const RED_PNG: &[u8] = include_bytes!("fixtures/red_1x1.png");

#[test]
fn smoke_single_page() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_heading("Title", 1).unwrap();
    doc.push_paragraph("This is a body paragraph.").unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"), "output must be a PDF");
    assert!(bytes.len() > 100);
}

#[test]
fn mixed_paragraph_can_use_an_opt_in_fallback_font() {
    let opts = FlowOptions {
        fallback_font_bytes: Some(NOTO.to_vec()),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_paragraph("Latin 日本語 symbols ✓").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(text.contains("Latin"));
    assert!(text.contains("日本語"));
    assert!(text.contains("✓"));
}

#[test]
fn styled_mixed_paragraph_can_use_an_opt_in_fallback_font() {
    let opts = FlowOptions {
        fallback_font_bytes: Some(NOTO.to_vec()),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_paragraph_styled(&[
        InlineSpan::plain("Latin "),
        InlineSpan::bold("日本語 "),
        InlineSpan::colored("symbols ✓", [0.1, 0.2, 0.3]),
    ])
    .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(text.contains("Latin"));
    assert!(text.contains("日本語"));
    assert!(text.contains("✓"));
}

#[test]
fn body_alignment_uses_the_measured_line_width() {
    let opts = FlowOptions {
        body_alignment: FlowTextAlignment::Center,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_paragraph("A").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let run = reloaded.extract_text_runs(1).unwrap().remove(0);
    assert!(run.x > 200.0, "centered text should move inward: {run:?}");
}

#[test]
fn paragraphs_accept_explicit_trailing_spacing() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_with_spacing("First", 0.0).unwrap();
    doc.push_paragraph_styled_with_spacing(&[InlineSpan::bold("Second")], 12.0)
        .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(text.contains("First") && text.contains("Second"));
}

#[test]
fn paragraph_spacing_rejects_non_finite_or_negative_values() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    assert!(doc.push_paragraph_with_spacing("text", -1.0).is_err());
    assert!(
        doc.push_paragraph_styled_with_spacing(&[InlineSpan::plain("text")], f32::NAN)
            .is_err()
    );
}

#[test]
fn baseline_offset_is_applied_deterministically() {
    let default_bytes = {
        let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
        doc.push_paragraph("baseline").unwrap();
        doc.render().unwrap()
    };
    let shifted_bytes = {
        let mut doc = FlowDocument::new(
            NOTO,
            FlowOptions {
                baseline_offset: 3.0,
                ..FlowOptions::default()
            },
        )
        .unwrap();
        doc.push_paragraph("baseline").unwrap();
        doc.render().unwrap()
    };
    let default_run = Document::from_bytes(&default_bytes)
        .unwrap()
        .extract_text_runs(1)
        .unwrap()
        .remove(0);
    let shifted_run = Document::from_bytes(&shifted_bytes)
        .unwrap()
        .extract_text_runs(1)
        .unwrap()
        .remove(0);
    assert!((shifted_run.y - default_run.y - 3.0).abs() < 0.01);
}

#[test]
fn baseline_offset_rejects_non_finite_values() {
    let result = FlowDocument::new(
        NOTO,
        FlowOptions {
            baseline_offset: f32::INFINITY,
            ..FlowOptions::default()
        },
    );
    assert!(result.is_err());
}

#[test]
fn figure_block_renders_and_can_reserve_following_body_line() {
    let opts = FlowOptions {
        keep_figures_with_next: true,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_figure(RED_PNG, 48.0, 32.0).unwrap();
    doc.push_paragraph("Figure context").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.extract_page_images(1).unwrap().len(), 1);
}

#[test]
fn figure_block_rejects_invalid_dimensions() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    assert!(doc.push_figure(RED_PNG, 0.0, 32.0).is_err());
    assert!(doc.push_figure(RED_PNG, f32::NAN, 32.0).is_err());
    assert!(doc.push_figure(&[], 10.0, 10.0).is_err());
}

#[test]
fn auto_pagination() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    for i in 0..80 {
        doc.push_paragraph(&format!(
            "Paragraph {} with some content to fill the page.",
            i
        ))
        .unwrap();
    }
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(
        reloaded.page_count() >= 2,
        "should have paginated to at least 2 pages"
    );
}

#[test]
fn header_footer_repeat_and_page_placeholders_survive_reload() {
    let opts = FlowOptions {
        header: Some(HeaderFooter {
            left: Some("Harumi report".into()),
            right: Some("{{page}}/{{total}}".into()),
            ..HeaderFooter::default()
        }),
        footer: Some(HeaderFooter::page_number()),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_paragraph("Page one body").unwrap();
    doc.push_page_break().unwrap();
    doc.push_paragraph("Page two body").unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);

    let page_text = |page| {
        reloaded
            .extract_text_runs(page)
            .unwrap()
            .into_iter()
            .map(|run| run.text)
            .collect::<String>()
    };
    let first = page_text(1);
    let second = page_text(2);
    assert!(first.contains("Harumi report"));
    assert!(second.contains("Harumi report"));
    assert!(first.contains("1/2") && first.contains("1 / 2"));
    assert!(second.contains("2/2") && second.contains("2 / 2"));
}

#[test]
fn multi_line_paragraph_avoids_single_line_orphan() {
    let opts = FlowOptions {
        page_size: (200.0, 200.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        paragraph_spacing: 0.0,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    for _ in 0..15 {
        doc.push_paragraph("filler").unwrap();
    }
    doc.push_paragraph("first line\nsecond line").unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
    let first_page: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    let second_page: String = reloaded
        .extract_text_runs(2)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(!first_page.contains("first line"));
    assert!(second_page.contains("first line") && second_page.contains("second line"));
}

#[test]
fn paragraph_min_lines_can_raise_widow_orphan_guard() {
    let opts = FlowOptions {
        page_size: (200.0, 200.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        paragraph_spacing: 0.0,
        paragraph_min_lines: 3,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    for _ in 0..15 {
        doc.push_paragraph("filler").unwrap();
    }
    doc.push_paragraph("first line\nsecond line\nthird line")
        .unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
    let first_page: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    let second_page: String = reloaded
        .extract_text_runs(2)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(!first_page.contains("first line"));
    assert!(second_page.contains("first line"));
    assert!(second_page.contains("third line"));
}

#[test]
fn heading_can_stay_with_the_following_body_line() {
    let opts = FlowOptions {
        page_size: (200.0, 200.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        heading_size_scale: [1.0; 6],
        line_height_factor: 1.0,
        paragraph_spacing: 0.0,
        keep_headings_with_next: true,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    for _ in 0..15 {
        doc.push_paragraph("filler").unwrap();
    }
    doc.push_heading("Section", 1).unwrap();
    doc.push_paragraph("Supporting body").unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
    let first_page: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    let second_page: String = reloaded
        .extract_text_runs(2)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(!first_page.contains("Section"));
    assert!(second_page.contains("Section") && second_page.contains("Supporting body"));
}

#[test]
fn table_can_stay_with_the_following_body_line() {
    let opts = FlowOptions {
        page_size: (200.0, 200.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        paragraph_spacing: 0.0,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    for _ in 0..14 {
        doc.push_paragraph("filler").unwrap();
    }
    doc.push_table(
        &[vec!["Table marker".into()]],
        TableOptions {
            column_widths: TableColumnWidths::Fractions(vec![1.0]),
            keep_with_next: true,
            ..TableOptions::default()
        },
    )
    .unwrap();
    doc.push_paragraph("Following body").unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
    let first_page: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    let second_page: String = reloaded
        .extract_text_runs(2)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(!first_page.contains("Table marker"));
    assert!(second_page.contains("Table marker") && second_page.contains("Following body"));
}

#[test]
fn heading_levels() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    for level in 1..=6 {
        doc.push_heading(&format!("Heading {}", level), level)
            .unwrap();
        doc.push_paragraph("Supporting text.").unwrap();
    }
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn key_value_table_smoke() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_key_value_table(&[("Name", "Alice"), ("Age", "30"), ("City", "Tokyo")])
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn empty_list_no_panic() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_list(&[], false).unwrap();
    doc.push_list(&[], true).unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn ordered_and_unordered_list() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_list(&["Alpha", "Beta", "Gamma"], false).unwrap();
    doc.push_list(&["First", "Second", "Third"], true).unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn explicit_page_break() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph("Page one.").unwrap();
    doc.push_page_break().unwrap();
    doc.push_paragraph("Page two.").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
}

#[test]
fn custom_margins() {
    let opts = FlowOptions {
        margins: Margins::uniform(36.0),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_heading("Narrow Margins", 1).unwrap();
    doc.push_paragraph("Content with custom 36pt margins.")
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn flow_can_embed_distinct_heading_and_code_fonts() {
    let opts = FlowOptions {
        heading_font_bytes: Some(NOTO.to_vec()),
        code_font_bytes: Some(NOTO.to_vec()),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    doc.push_heading("Heading font", 1).unwrap();
    doc.push_code_block("code font").unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
    assert!(
        bytes
            .windows(b"/FontFile2".len())
            .any(|w| w == b"/FontFile2")
    );
}

#[test]
fn cjk_paragraph_e2e() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_heading("日本語の見出し", 1).unwrap();
    doc.push_paragraph(
        "これは日本語のサンプルテキストです。PDFに正しく出力されることを確認します。\
         長いテキストが複数行に折り返されることも検証します。",
    )
    .unwrap();
    doc.push_key_value_table(&[("名前", "田中健太郎"), ("住所", "東京都渋谷区")])
        .unwrap();
    let bytes = doc.render().unwrap();
    assert!(bytes.starts_with(b"%PDF"));

    if std::env::var("HARUMI_FLOW_OUT").is_ok() {
        std::fs::write("flow_out.pdf", &bytes).unwrap();
        eprintln!("Written to flow_out.pdf");
    }
}

#[test]
fn max_pages_limit_returns_error() {
    let opts = harumi::FlowOptions {
        max_pages: 2,
        ..harumi::FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    // Fill until we hit the limit.
    let result = (0..500).try_for_each(|i| doc.push_paragraph(&format!("Paragraph {}", i)));
    assert!(
        result.is_err(),
        "should return error when max_pages exceeded"
    );
}

#[test]
fn many_table_rows_paginate() {
    let rows: Vec<(String, String)> = (0..50)
        .map(|i| (format!("Key {}", i), format!("Value {}", i)))
        .collect();
    let rows_ref: Vec<(&str, &str)> = rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_key_value_table(&rows_ref).unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(
        reloaded.page_count() >= 2,
        "50 rows should span at least 2 pages"
    );
}

#[test]
fn oversized_table_row_splits_across_pages() {
    let opts = FlowOptions {
        page_size: (200.0, 100.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    let value = "line\n".repeat(8);
    doc.push_key_value_table(&[("key", &value)]).unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(reloaded.page_count() >= 2);
    let text: String = (1..=reloaded.page_count())
        .flat_map(|page| reloaded.extract_text_runs(page).unwrap())
        .map(|run| run.text)
        .collect();
    assert_eq!(text.matches("line").count(), 8);
}

#[test]
fn generic_table_supports_width_strategies_and_wrapping() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    let rows = vec![
        vec!["地域".into(), "売上".into(), "備考".into()],
        vec![
            "東京".into(),
            "¥12,345,678".into(),
            "CJK/Latin long cell content for deterministic wrapping".into(),
        ],
    ];
    doc.push_table(
        &rows,
        TableOptions {
            column_widths: TableColumnWidths::Fractions(vec![1.0, 1.0, 2.0]),
            ..TableOptions::default()
        },
    )
    .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let text: String = (1..=reloaded.page_count())
        .flat_map(|page| reloaded.extract_text_runs(page).unwrap())
        .map(|run| run.text)
        .collect();
    for marker in ["地域", "売上", "東京", "¥12,345,678", "CJK/Latin"] {
        assert!(text.contains(marker), "missing marker {marker:?}");
    }
}

#[test]
fn generic_table_rejects_invalid_widths() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    let rows = vec![vec!["a".into(), "b".into()]];
    assert!(
        doc.push_table(
            &rows,
            TableOptions {
                column_widths: TableColumnWidths::Fixed(vec![500.0]),
                ..TableOptions::default()
            },
        )
        .is_err()
    );
}

#[test]
fn generic_table_repeats_header_rows_after_page_break() {
    let opts = FlowOptions {
        page_size: (220.0, 150.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    let mut rows = vec![vec!["地域".into(), "売上".into()]];
    rows.extend((0..30).map(|i| vec![format!("東京{i}"), format!("¥{i}")]));
    doc.push_table(
        &rows,
        TableOptions {
            column_widths: TableColumnWidths::Fractions(vec![1.0, 1.0]),
            header_rows: 1,
            ..TableOptions::default()
        },
    )
    .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(reloaded.page_count() >= 2);
    let text: String = (1..=reloaded.page_count())
        .flat_map(|page| reloaded.extract_text_runs(page).unwrap())
        .map(|run| run.text)
        .collect();
    assert!(text.matches("地域").count() >= reloaded.page_count() as usize);
    assert!(text.matches("売上").count() >= reloaded.page_count() as usize);
}

#[test]
fn generic_table_applies_min_max_column_constraints() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    let rows = vec![vec!["left".into(), "right".into()]];
    doc.push_table(
        &rows,
        TableOptions {
            column_widths: TableColumnWidths::Fixed(vec![100.0, 100.0]),
            min_column_widths: Some(vec![120.0, 40.0]),
            max_column_widths: Some(vec![140.0, 120.0]),
            ..TableOptions::default()
        },
    )
    .unwrap();
    assert!(doc.render().unwrap().starts_with(b"%PDF"));

    let mut invalid = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    assert!(
        invalid
            .push_table(
                &rows,
                TableOptions {
                    column_widths: TableColumnWidths::Fractions(vec![1.0, 1.0]),
                    min_column_widths: Some(vec![300.0, 300.0]),
                    ..TableOptions::default()
                },
            )
            .is_err()
    );
}

#[test]
fn generic_table_exposes_resolved_width_diagnostic() {
    let doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    let rows = vec![vec!["a".into(), "b".into()]];
    let allocation = doc
        .measure_table_widths(
            &rows,
            &TableOptions {
                column_widths: TableColumnWidths::Fractions(vec![1.0, 3.0]),
                ..TableOptions::default()
            },
        )
        .unwrap();
    assert_eq!(allocation.widths.len(), 2);
    assert!((allocation.widths[0] * 3.0 - allocation.widths[1]).abs() < 0.01);
    assert!((allocation.widths.iter().sum::<f32>() - allocation.content_width).abs() < 0.01);
}

#[test]
fn generic_table_cells_support_horizontal_colspan() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    let rows = vec![
        vec![FlowTableCell::spanning("四半期レポート", 2)],
        vec![
            FlowTableCell::new("地域"),
            FlowTableCell::new("売上")
                .with_alignment(TableCellAlignment::Right)
                .with_padding(6.0),
        ],
        vec![
            FlowTableCell::new("東京").with_alignment(TableCellAlignment::Center),
            FlowTableCell::new("¥12,345"),
        ],
    ];
    doc.push_table_cells(
        &rows,
        TableOptions {
            column_widths: TableColumnWidths::Fractions(vec![1.0, 1.0]),
            ..TableOptions::default()
        },
    )
    .unwrap();
    let pdf = doc.render().unwrap();
    let reloaded = Document::from_bytes(&pdf).unwrap();
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    for marker in ["四半期レポート", "地域", "売上", "東京", "¥12,345"] {
        assert!(text.contains(marker), "missing marker {marker:?}: {text:?}");
    }
}

#[test]
fn generic_table_cells_support_vertical_rowspan() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_table_cells(
        &[
            vec![
                FlowTableCell::new("rowspan").with_rowspan(2),
                FlowTableCell::new("top"),
            ],
            vec![FlowTableCell::new("bottom")],
        ],
        TableOptions::default(),
    )
    .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(text.contains("rowspan"));
    assert!(text.contains("top"));
    assert!(text.contains("bottom"));
}

#[test]
fn oversized_code_block_splits_without_losing_lines() {
    let opts = FlowOptions {
        page_size: (200.0, 100.0),
        margins: Margins::uniform(20.0),
        body_font_size: 10.0,
        line_height_factor: 1.0,
        code_background: Some([0.95, 0.95, 0.95]),
        ..FlowOptions::default()
    };
    let mut doc = FlowDocument::new(NOTO, opts).unwrap();
    let code = (0..8)
        .map(|i| format!("code line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    doc.push_code_block(&code).unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert!(reloaded.page_count() >= 2);
    let text: String = (1..=reloaded.page_count())
        .flat_map(|page| reloaded.extract_text_runs(page).unwrap())
        .map(|run| run.text)
        .collect();
    for i in 0..8 {
        assert!(
            text.contains(&format!("code line {i}")),
            "missing line {i}: {text:?}"
        );
    }
}

/// Shared report-generation contract used when comparing harumi FlowDocument
/// with printpdf/genpdf: a CJK heading, a table, an explicit page break, and
/// extractable text must survive serialization.
#[test]
fn report_generation_fixture_contract() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_heading("四半期レポート", 1).unwrap();
    doc.push_paragraph(
        "段落組版契約: CJK/Latin mixed paragraph with enough text to exercise deterministic wrapping.",
    )
    .unwrap();
    doc.push_paragraph_styled(&[
        InlineSpan::plain("混在スタイル: "),
        InlineSpan::bold("bold"),
        InlineSpan::italic(" italic"),
    ])
    .unwrap();
    doc.push_key_value_table(&[
        ("売上", "¥12,345,678"),
        ("顧客数", "1,234"),
        ("地域", "東京・大阪・福岡"),
        ("長大セル", "CJK/Latin long cell content for wrapping"),
    ])
    .unwrap();
    doc.push_page_break().unwrap();
    doc.push_heading("明細", 2).unwrap();
    doc.push_paragraph("ページ分割後も帳票本文を抽出できることを確認します。")
        .unwrap();

    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 2);
    let text: String = (1..=reloaded.page_count())
        .flat_map(|page| reloaded.extract_text_runs(page).unwrap())
        .map(|run| run.text)
        .collect();
    let expected_markers = [
        "四半期レポート",
        "組版契約",
        "混在スタイル",
        "売上",
        "¥12,345,678",
        "顧客数",
        "1,234",
        "地域",
        "東京・大阪・福岡",
        "長大セル",
        "明細",
    ];
    let mut offset = 0;
    for expected in expected_markers {
        let relative = text[offset..]
            .find(expected)
            .unwrap_or_else(|| panic!("missing or out-of-order marker {expected:?}: {text:?}"));
        offset += relative + expected.len();
    }
    assert!(
        bytes
            .windows(b"/FontFile2".len())
            .any(|w| w == b"/FontFile2")
    );
}

// ---------------------------------------------------------------------------
// InlineSpan / push_paragraph_styled tests
// ---------------------------------------------------------------------------

#[test]
fn inline_spans_plain() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[InlineSpan::plain("Hello "), InlineSpan::plain("world")])
        .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert!(
        text.contains("Hello") && text.contains("world"),
        "text: {:?}",
        text
    );
}

#[test]
fn inline_spans_bold_italic_color() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[
        InlineSpan::bold("Bold "),
        InlineSpan::italic("Italic "),
        InlineSpan::colored("Red", [1.0, 0.0, 0.0]),
    ])
    .unwrap();
    let bytes = doc.render().unwrap();
    // Just verify it produces a valid PDF without panic.
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
}

#[test]
fn inline_spans_cjk_mixed_style() {
    let mut doc = FlowDocument::new(NOTO, FlowOptions::default()).unwrap();
    doc.push_paragraph_styled(&[InlineSpan::plain("日本語 "), InlineSpan::bold("太字")])
        .unwrap();
    let bytes = doc.render().unwrap();
    let reloaded = Document::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.page_count(), 1);
    let text: String = reloaded
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert!(text.contains("日本語"), "text: {:?}", text);
}
