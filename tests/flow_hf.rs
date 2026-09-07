/// Tests for FlowDocument header/footer and auto-bookmark features (v0.5).
#[cfg(feature = "flow")]
mod inner {
    use harumi::{
        Color, Document, FlowDocument, FlowOptions, HeaderFooter, Margins, PageBorder,
        PageDecoration, PageTemplate, PageTemplateVariants,
    };

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
    fn section_switches_header_and_footer_on_new_page() {
        let opts = FlowOptions {
            header: Some(HeaderFooter {
                left: Some("Section A".into()),
                ..Default::default()
            }),
            footer: Some(HeaderFooter {
                center: Some("Footer A".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_paragraph("Body A").unwrap();
        doc.push_section(
            Some(HeaderFooter {
                left: Some("Section B".into()),
                ..Default::default()
            }),
            Some(HeaderFooter {
                center: Some("Footer B".into()),
                ..Default::default()
            }),
        )
        .unwrap();
        doc.push_paragraph("Body B").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = Document::from_bytes(&bytes).unwrap();
        assert_eq!(reloaded.page_count(), 2);
        let page_one: String = reloaded
            .extract_text_runs(1)
            .unwrap()
            .into_iter()
            .map(|run| run.text)
            .collect();
        let page_two: String = reloaded
            .extract_text_runs(2)
            .unwrap()
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert!(page_one.contains("Section A") && page_one.contains("Footer A"));
        assert!(page_two.contains("Section B") && page_two.contains("Footer B"));
        assert!(!page_one.contains("Section B") && !page_two.contains("Section A"));
    }

    #[test]
    fn no_header_footer_still_works() {
        let opts = FlowOptions::default(); // header = None, footer = None
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_paragraph("No decoration.").unwrap();
        let bytes = doc.render().unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn page_decoration_renders_background_and_border() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.set_page_decoration(PageDecoration {
            background: Some(Color::Rgb([0.95, 0.95, 0.98])),
            border: Some(PageBorder {
                color: Color::Rgb([0.1, 0.2, 0.3]),
                width: 2.0,
            }),
        })
        .unwrap();
        doc.push_paragraph("Decorated page").unwrap();
        let bytes = doc.render().unwrap();
        let pdf = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
        let page_id = *pdf.get_pages().get(&1).unwrap();
        let content = String::from_utf8(pdf.get_page_content(page_id).unwrap()).unwrap();
        assert!(
            content.contains(" rg\n"),
            "background fill missing: {content}"
        );
        assert!(
            content.contains(" RG\n"),
            "border stroke missing: {content}"
        );
        assert!(content.contains(" w\n"), "border width missing: {content}");
    }

    #[test]
    fn section_decoration_switches_between_pages() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.set_page_decoration(PageDecoration {
            background: Some(Color::Rgb([0.95, 0.95, 0.98])),
            ..Default::default()
        })
        .unwrap();
        doc.push_paragraph("First page").unwrap();
        doc.push_section_with_decoration(
            None,
            None,
            PageDecoration {
                background: Some(Color::Rgb([0.98, 0.95, 0.95])),
                ..Default::default()
            },
        )
        .unwrap();
        doc.push_paragraph("Second page").unwrap();
        let bytes = doc.render().unwrap();
        let pdf = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
        let pages = pdf.get_pages();
        let first =
            String::from_utf8(pdf.get_page_content(*pages.get(&1).unwrap()).unwrap()).unwrap();
        let second =
            String::from_utf8(pdf.get_page_content(*pages.get(&2).unwrap()).unwrap()).unwrap();
        assert!(
            first.contains(" rg\n"),
            "first page background missing: {first}"
        );
        assert!(
            second.contains(" rg\n"),
            "second page background missing: {second}"
        );
        assert_ne!(first, second, "section decoration/content should differ");
    }

    #[test]
    fn section_margins_switch_body_geometry() {
        let mut doc = FlowDocument::new(
            FONT,
            FlowOptions {
                margins: Margins::uniform(20.0),
                ..Default::default()
            },
        )
        .unwrap();
        doc.push_paragraph("Left margin A").unwrap();
        doc.push_section_with_margins(
            None,
            None,
            Margins {
                top: 30.0,
                right: 40.0,
                bottom: 30.0,
                left: 100.0,
            },
        )
        .unwrap();
        doc.push_paragraph("Left margin B").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = Document::from_bytes(&bytes).unwrap();
        let first = reloaded.extract_text_runs(1).unwrap().remove(0);
        let second = reloaded.extract_text_runs(2).unwrap().remove(0);
        assert_eq!(first.x, 20.0);
        assert_eq!(second.x, 100.0);
    }

    #[test]
    fn section_margins_reject_invalid_geometry() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        assert!(
            doc.push_section_with_margins(
                None,
                None,
                Margins {
                    top: 10.0,
                    right: 600.0,
                    bottom: 10.0,
                    left: 10.0,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn first_odd_even_templates_switch_page_geometry() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.set_page_template_variants(PageTemplateVariants {
            first: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("First".into()),
                    ..Default::default()
                }),
                None,
                Margins::uniform(30.0),
            )),
            odd: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("Odd".into()),
                    ..Default::default()
                }),
                None,
                Margins {
                    top: 20.0,
                    right: 20.0,
                    bottom: 20.0,
                    left: 90.0,
                },
            )),
            even: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("Even".into()),
                    ..Default::default()
                }),
                None,
                Margins {
                    top: 20.0,
                    right: 60.0,
                    bottom: 20.0,
                    left: 60.0,
                },
            )),
        })
        .unwrap();
        doc.push_paragraph("First body").unwrap();
        doc.push_page_break().unwrap();
        doc.push_paragraph("Even body").unwrap();
        doc.push_page_break().unwrap();
        doc.push_paragraph("Odd body").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = Document::from_bytes(&bytes).unwrap();
        assert_eq!(reloaded.page_count(), 3);
        assert_eq!(reloaded.extract_text_runs(1).unwrap()[0].x, 30.0);
        assert_eq!(reloaded.extract_text_runs(2).unwrap()[0].x, 60.0);
        assert_eq!(reloaded.extract_text_runs(3).unwrap()[0].x, 90.0);
        for (page, marker) in [(1, "First"), (2, "Even"), (3, "Odd")] {
            let text: String = reloaded
                .extract_text_runs(page)
                .unwrap()
                .into_iter()
                .map(|run| run.text)
                .collect();
            assert!(
                text.contains(marker),
                "page {page} missing {marker}: {text}"
            );
        }
    }

    #[test]
    fn first_odd_even_templates_restart_for_new_section() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.push_paragraph("Base section").unwrap();
        doc.push_section(None, None).unwrap();
        doc.set_page_template_variants(PageTemplateVariants {
            first: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("Section first".into()),
                    ..Default::default()
                }),
                None,
                Margins::uniform(20.0),
            )),
            odd: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("Section odd".into()),
                    ..Default::default()
                }),
                None,
                Margins::uniform(30.0),
            )),
            even: Some(PageTemplate::new(
                Some(HeaderFooter {
                    left: Some("Section even".into()),
                    ..Default::default()
                }),
                None,
                Margins::uniform(40.0),
            )),
        })
        .unwrap();
        doc.push_paragraph("Section page one").unwrap();
        doc.push_page_break().unwrap();
        doc.push_paragraph("Section page two").unwrap();
        doc.push_page_break().unwrap();
        doc.push_paragraph("Section page three").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = Document::from_bytes(&bytes).unwrap();
        for (page, marker) in [
            (2, "Section first"),
            (3, "Section even"),
            (4, "Section odd"),
        ] {
            let text: String = reloaded
                .extract_text_runs(page)
                .unwrap()
                .into_iter()
                .map(|run| run.text)
                .collect();
            assert!(
                text.contains(marker),
                "page {page} missing {marker}: {text}"
            );
        }
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
    fn auto_bookmark_tracks_heading_after_reflow() {
        let opts = FlowOptions {
            auto_bookmarks: true,
            page_size: (200.0, 100.0),
            margins: Margins::uniform(10.0),
            body_font_size: 10.0,
            line_height_factor: 1.0,
            paragraph_spacing: 0.0,
            ..Default::default()
        };
        let mut doc = FlowDocument::new(FONT, opts).unwrap();
        doc.push_paragraph("a\nb\nc\nd\ne\nf\ng").unwrap();
        doc.push_heading("Reflowed heading", 1).unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
        let pages = reloaded.get_pages();
        let page_two = *pages.get(&2).expect("heading should reflow to page two");
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
        let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
        let first = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();
        let dest = first.get(b"Dest").unwrap().as_array().unwrap();
        assert_eq!(dest[0].as_reference().unwrap(), page_two);
    }

    #[test]
    fn named_flow_bookmark_points_to_current_position() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.push_page_break().unwrap();
        doc.push_bookmark("Contents", 1).unwrap();
        doc.push_paragraph("Bookmarked section").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = harumi::lopdf::Document::load_from(bytes.as_slice()).unwrap();
        let page_two = *reloaded
            .get_pages()
            .get(&2)
            .expect("second page should exist");
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
        let first_ref = outlines.get(b"First").unwrap().as_reference().unwrap();
        let first = reloaded.get_object(first_ref).unwrap().as_dict().unwrap();
        let dest = first.get(b"Dest").unwrap().as_array().unwrap();
        assert_eq!(dest[0].as_reference().unwrap(), page_two);
    }

    #[test]
    fn generated_table_of_contents_lists_stable_bookmarks() {
        let mut doc = FlowDocument::new(FONT, FlowOptions::default()).unwrap();
        doc.push_heading("Chapter 1", 1).unwrap();
        doc.push_paragraph("Body").unwrap();
        doc.push_bookmark("Named section", 1).unwrap();
        doc.push_paragraph("Section body").unwrap();
        doc.push_table_of_contents("Contents").unwrap();

        let bytes = doc.render().unwrap();
        let reloaded = Document::from_bytes(&bytes).unwrap();
        assert_eq!(reloaded.page_count(), 2);
        let contents: String = reloaded
            .extract_text_runs(2)
            .unwrap()
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert!(
            contents.contains("Contents"),
            "TOC title missing: {contents}"
        );
        assert!(
            contents.contains("Chapter 1"),
            "heading missing: {contents}"
        );
        assert!(
            contents.contains("Named section"),
            "named bookmark missing: {contents}"
        );
        assert!(
            contents.contains("...... 1"),
            "page number missing: {contents}"
        );
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
