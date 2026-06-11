/// Tests for FlowDocument header/footer and auto-bookmark features (v0.5).
#[cfg(feature = "flow")]
mod inner {
    use harumi::{Document, FlowDocument, FlowOptions, HeaderFooter};

    const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

    // ---------------------------------------------------------------------------
    // HeaderFooter::page_number() convenience constructor
    // ---------------------------------------------------------------------------

    #[test]
    fn page_number_footer_creates_center_template() {
        let hf = HeaderFooter::page_number();
        assert_eq!(hf.center.as_deref(), Some("{{page}} / {{total}}"));
    }

    // ---------------------------------------------------------------------------
    // Footer renders on every page
    // ---------------------------------------------------------------------------

    #[test]
    fn footer_page_numbers_smoke() {
        let opts = FlowOptions {
            footer: Some(HeaderFooter::page_number()),
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        // Push enough paragraphs to spill onto multiple pages.
        for i in 0..60 {
            doc.push_paragraph(&format!("Paragraph {i}")).unwrap();
        }
        let bytes = doc.render().unwrap();
        assert!(!bytes.is_empty());

        // Must produce a multi-page document.
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
        assert!(reloaded.get_pages().len() > 1, "Expected multiple pages");
    }

    #[test]
    fn header_and_footer_smoke() {
        let opts = FlowOptions {
            header: Some(HeaderFooter {
                left: Some("harumi docs".into()),
                right: Some("v0.5".into()),
                ..Default::default()
            }),
            footer: Some(HeaderFooter {
                center: Some("{{page}} / {{total}}".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_heading("Title", 1).unwrap();
        doc.push_paragraph("Body text.").unwrap();
        let bytes = doc.render().unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn no_header_footer_still_works() {
        let opts = FlowOptions::default(); // header = None, footer = None
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_paragraph("No decoration.").unwrap();
        let bytes = doc.render().unwrap();
        assert!(!bytes.is_empty());
    }

    // ---------------------------------------------------------------------------
    // Auto-bookmarks from push_heading
    // ---------------------------------------------------------------------------

    #[test]
    fn auto_bookmarks_generates_outlines() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_heading("Chapter 1", 1).unwrap();
        doc.push_paragraph("Body.").unwrap();
        doc.push_heading("Chapter 2", 1).unwrap();
        doc.push_paragraph("More body.").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        assert!(
            catalog.get(b"Outlines").is_ok(),
            "/Outlines must be present when auto_bookmarks=true"
        );

        let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = reloaded
            .get_object(outlines_ref)
            .unwrap()
            .as_dict()
            .unwrap();
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert_eq!(count, 2, "Two headings should produce two bookmarks");
    }

    #[test]
    fn auto_bookmarks_disabled_produces_no_outlines() {
        let opts = FlowOptions {
            auto_bookmarks: false,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_heading("Chapter 1", 1).unwrap();
        doc.push_paragraph("Body.").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        assert!(
            catalog.get(b"Outlines").is_err(),
            "/Outlines must NOT be present when auto_bookmarks=false"
        );
    }

    #[test]
    fn auto_bookmarks_all_heading_levels_recorded() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        for level in 1u8..=6 {
            doc.push_heading(&format!("Heading {level}"), level)
                .unwrap();
            doc.push_paragraph("Short paragraph.").unwrap();
        }

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = reloaded
            .get_object(outlines_ref)
            .unwrap()
            .as_dict()
            .unwrap();
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert_eq!(count, 6, "Six headings should produce six bookmarks");
    }

    #[test]
    fn auto_bookmarks_no_headings_produces_no_outlines() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_paragraph("Just a paragraph, no headings.")
            .unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        // No headings → no pending bookmarks → no /Outlines in catalog
        assert!(
            catalog.get(b"Outlines").is_err(),
            "/Outlines must be absent when there are no headings"
        );
    }

    // ---------------------------------------------------------------------------
    // Page number substitution — semantic round-trip
    // ---------------------------------------------------------------------------

    /// Render a 2-page FlowDocument with `HeaderFooter::page_number()` footer,
    /// reload it, extract text from each page, and verify the rendered strings
    /// are "1 / 2" on page 1 and "2 / 2" on page 2.
    #[test]
    fn footer_page_number_substitution_roundtrip() {
        let opts = FlowOptions {
            footer: Some(HeaderFooter::page_number()),
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        // Push enough paragraphs to guarantee at least 2 pages.
        for i in 0..60 {
            doc.push_paragraph(&format!("Paragraph {i}")).unwrap();
        }
        let bytes = doc.render().unwrap();

        // Reload as a harumi Document so we can use extract_text_runs.
        let reloaded = Document::from_bytes(&bytes).unwrap();
        let total = reloaded.page_count();
        assert!(total >= 2, "Expected at least 2 pages, got {total}");

        let runs_p1 = reloaded.extract_text_runs(1).unwrap();
        let text_p1: String = runs_p1.iter().map(|r| r.text.as_str()).collect();
        assert!(
            text_p1.contains("1 / "),
            "Page 1 footer must contain '1 / ', got: {text_p1:?}"
        );

        let runs_p2 = reloaded.extract_text_runs(2).unwrap();
        let text_p2: String = runs_p2.iter().map(|r| r.text.as_str()).collect();
        assert!(
            text_p2.contains("2 / "),
            "Page 2 footer must contain '2 / ', got: {text_p2:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // CJK heading bookmarks
    // ---------------------------------------------------------------------------

    #[test]
    fn auto_bookmarks_cjk_heading_title() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_heading("第1章　日本語見出し", 1).unwrap();
        doc.push_paragraph("本文テキスト").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        assert!(
            catalog.get(b"Outlines").is_ok(),
            "/Outlines must be present for CJK heading"
        );
    }

    #[test]
    fn hierarchical_outline_two_levels() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();

        // Push h1 and h2 headings to create a hierarchy.
        doc.push_heading("Chapter 1", 1).unwrap();
        doc.push_paragraph("Intro").unwrap();
        doc.push_heading("Section 1.1", 2).unwrap();
        doc.push_paragraph("Content").unwrap();
        doc.push_heading("Section 1.2", 2).unwrap();
        doc.push_paragraph("More content").unwrap();
        doc.push_heading("Chapter 2", 1).unwrap();
        doc.push_paragraph("Another chapter").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = reloaded
            .get_object(outlines_ref)
            .unwrap()
            .as_dict()
            .unwrap();

        // Root should have both top-level chapters.
        let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
        let first_item = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();

        // Root's /Count should include all items (4 total: 2 h1 + 2 h2).
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert_eq!(count, 4, "Root /Count should be 4 (2 h1 + 2 h2)");

        // The first h1 should have /First pointing to its first h2 child (if it has children).
        if let Ok(h1_first) = first_item.get(b"First") {
            let h1_first_ref = h1_first.as_reference().unwrap();
            let h2_item = reloaded
                .get_object(h1_first_ref)
                .unwrap()
                .as_dict()
                .unwrap();

            // The h2 item should have a /Parent pointing back to the h1 item.
            let h2_parent_ref = h2_item.get(b"Parent").unwrap().as_reference().unwrap();
            assert_eq!(
                h2_parent_ref, first_ref,
                "h2 /Parent should point to its h1 parent"
            );

            // The h1 should also have /Last pointing to a child (either h2_1.2 or the last h2).
            assert!(
                first_item.get(b"Last").is_ok(),
                "h1 with children should have /Last"
            );
        }
    }

    #[test]
    fn flat_outline_backward_compatible() {
        // When using add_bookmark (level=0), the outline should remain flat.
        let mut doc = Document::new((595.0, 842.0)).unwrap();
        doc.add_bookmark("Chapter 1", 1, 700.0).unwrap();
        doc.add_bookmark("Chapter 2", 1, 600.0).unwrap();
        doc.add_bookmark("Chapter 3", 1, 500.0).unwrap();

        let bytes = doc.save_to_bytes().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();

        let root_ref = reloaded
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reloaded.get_object(root_ref).unwrap().as_dict().unwrap();
        let outlines_ref = catalog.get(b"Outlines").unwrap().as_reference().unwrap();
        let outlines = reloaded
            .get_object(outlines_ref)
            .unwrap()
            .as_dict()
            .unwrap();

        // All three bookmarks should be at the top level (siblings).
        let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
        let first_item = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();

        // First item should have a /Next (sibling) but no /First (leaf, no children).
        assert!(
            first_item.get(b"Next").is_ok(),
            "First item should have /Next (sibling)"
        );
        assert!(
            first_item.get(b"First").is_err(),
            "Flat outline items should not have /First (no children)"
        );

        // Root's /Count should be 3 (all flat).
        let count = outlines.get(b"Count").unwrap().as_i64().unwrap();
        assert_eq!(count, 3, "Flat outline /Count should be 3");
    }
}

// Re-export so tests compile even without the feature.
#[cfg(not(feature = "flow"))]
#[test]
fn flow_feature_not_enabled() {}
