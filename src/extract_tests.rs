use super::*;
use lopdf::Object;

#[test]
fn parse_to_unicode_cmap_basic() {
    let cmap = b"/CIDInit /ProcSet findresource begin\n\
                 12 dict begin\n\
                 begincmap\n\
                 1 beginbfchar\n\
                 <0001> <65E5>\n\
                 endbfchar\n\
                 endcmap\n\
                 end\nend\n";
    let map = parse_to_unicode_cmap(cmap);
    assert_eq!(map.get(&1u16), Some(&'日'));
}

#[test]
fn parse_to_unicode_cmap_surrogate() {
    let cmap = b"1 beginbfchar\n<0001> <D840DC00>\nendbfchar\n";
    let map = parse_to_unicode_cmap(cmap);
    assert_eq!(map.get(&1u16), Some(&'\u{20000}'));
}

#[test]
fn parse_bfrange_contiguous() {
    let cmap = b"1 beginbfrange\n<20> <7E> <0020>\nendbfrange\n";
    let map = parse_to_unicode_cmap(cmap);
    assert_eq!(map.get(&0x20), Some(&' '));
    assert_eq!(map.get(&0x41), Some(&'A'));
    assert_eq!(map.get(&0x7E), Some(&'~'));
}

#[test]
fn parse_bfrange_explicit_array() {
    let cmap = b"1 beginbfrange\n<20> <21> [<0048> <0069>]\nendbfrange\n";
    let map = parse_to_unicode_cmap(cmap);
    assert_eq!(map.get(&0x20), Some(&'H'));
    assert_eq!(map.get(&0x21), Some(&'i'));
}

#[test]
fn decode_hex_bytes_roundtrip() {
    let hex = b"00010002";
    let bytes = decode_hex_bytes(hex);
    assert_eq!(bytes, vec![0x00, 0x01, 0x00, 0x02]);
}

#[test]
fn litstr_tokenizer_basic() {
    let stream = b"(Hello)";
    let tokens = tokenize(stream);
    assert!(matches!(&tokens[0].0, Token::LitStr(b) if b == b"Hello"));
}

#[test]
fn litstr_escapes() {
    let stream = b"(He\\nllo\\041)"; // \n and \041 = '!'
    let tokens = tokenize(stream);
    match &tokens[0].0 {
        Token::LitStr(b) => {
            assert_eq!(b[0], b'H');
            assert_eq!(b[1], b'e');
            assert_eq!(b[2], b'\n');
            assert_eq!(b[3], b'l');
            assert_eq!(b[6], b'!');
        }
        _ => panic!("expected LitStr"),
    }
}

#[test]
fn litstr_in_array() {
    let stream = b"[(Hel) -50 (lo)]";
    let tokens = tokenize(stream);
    if let Token::Array(items) = &tokens[0].0 {
        assert!(matches!(&items[0], Token::LitStr(b) if b == b"Hel"));
        assert!(matches!(&items[1], Token::Number(n) if (*n + 50.0).abs() < 0.1));
        assert!(matches!(&items[2], Token::LitStr(b) if b == b"lo"));
    } else {
        panic!("expected Array");
    }
}

#[test]
fn tokenizer_smoke() {
    let stream = b"BT\n/F0 12 Tf\n100 200 Td\n<0001> Tj\nET\n";
    let tokens = tokenize(stream);
    let keywords: Vec<&[u8]> = tokens
        .iter()
        .filter_map(|(t, _)| {
            if let Token::Keyword(k) = t {
                Some(k.as_slice())
            } else {
                None
            }
        })
        .collect();
    assert!(keywords.contains(&b"BT".as_slice()));
    assert!(keywords.contains(&b"Tf".as_slice()));
    assert!(keywords.contains(&b"Td".as_slice()));
    assert!(keywords.contains(&b"Tj".as_slice()));
    assert!(keywords.contains(&b"ET".as_slice()));
}

#[test]
fn parse_w_array_run_format() {
    let arr = vec![
        Object::Integer(0),
        Object::Array(vec![
            Object::Integer(500),
            Object::Integer(600),
            Object::Integer(700),
        ]),
    ];
    let runs = parse_w_array(&arr);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].start_gid, 0);
    assert_eq!(runs[0].widths, vec![500, 600, 700]);
}

#[test]
fn font_info_advance_width_fallback() {
    let info = FontInfo {
        to_unicode: BTreeMap::new(),
        to_unicode_text: BTreeMap::new(),
        dw: 1000,
        w_runs: vec![WidthRun {
            start_gid: 5,
            widths: vec![600],
        }],
        bytes_per_char: 2,
        identity_fallback: false,
        base_font: String::new(),
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
    };
    assert_eq!(info.advance_width(5), 600);
    assert_eq!(info.advance_width(0), 1000);
    assert_eq!(info.advance_width(99), 1000);
}

#[test]
fn win_ansi_spot_checks() {
    assert_eq!(WIN_ANSI_ENCODING[0x20], Some(' '));
    assert_eq!(WIN_ANSI_ENCODING[0x41], Some('A'));
    assert_eq!(WIN_ANSI_ENCODING[0x80], Some('€'));
    assert_eq!(WIN_ANSI_ENCODING[0xE9], Some('é'));
    assert_eq!(WIN_ANSI_ENCODING[0x7F], None);
}

#[test]
fn parse_to_unicode_preserves_multiple_scalars() {
    let cmap = br#"
        1 beginbfchar
        <0001> <00410301>
        endbfchar
    "#;
    let parsed = parse_to_unicode_cmap_text(cmap);
    assert_eq!(parsed.get(&1), Some(&"A\u{301}".to_owned()));
}

#[test]
fn agl_table_sorted() {
    for i in 1..AGL_TABLE.len() {
        assert!(
            AGL_TABLE[i - 1].0 < AGL_TABLE[i].0,
            "AGL_TABLE not sorted at index {i}: {:?} >= {:?}",
            AGL_TABLE[i - 1].0,
            AGL_TABLE[i].0
        );
    }
}

#[test]
fn glyph_name_lookup_spot_checks() {
    assert_eq!(glyph_name_to_char(b"space"), Some(' '));
    assert_eq!(glyph_name_to_char(b"eacute"), Some('é'));
    assert_eq!(glyph_name_to_char(b"euro"), Some('€'));
    assert_eq!(glyph_name_to_char(b"Euro"), Some('€'));
    assert_eq!(glyph_name_to_char(b"fi"), Some('\u{FB01}'));
    assert_eq!(glyph_name_to_char(b"nonexistent"), None);
}

#[test]
fn encoding_table_to_btree_basic() {
    let map = encoding_table_to_btree(&WIN_ANSI_ENCODING);
    assert_eq!(map.get(&0x41), Some(&'A'));
    assert_eq!(map.get(&0x80), Some(&'€'));
    assert!(!map.contains_key(&0x7F)); // undefined slot not included
}

#[test]
fn parse_font_attributes_cases() {
    // Plain family
    let (name, bold, italic, family) = parse_font_attributes("Helvetica");
    assert_eq!(name, "Helvetica");
    assert!(!bold);
    assert!(!italic);
    assert_eq!(family, "Helvetica");

    // Bold + subset prefix
    let (name, bold, italic, family) = parse_font_attributes("ABCDEF+Helvetica-Bold");
    assert_eq!(name, "Helvetica-Bold");
    assert!(bold);
    assert!(!italic);
    assert_eq!(family, "Helvetica");

    // BoldItalic
    let (name, bold, italic, family) = parse_font_attributes("TimesNewRoman-BoldItalic");
    assert_eq!(name, "TimesNewRoman-BoldItalic");
    assert!(bold);
    assert!(italic);
    assert_eq!(family, "TimesNewRoman");

    // Oblique style
    let (_name, bold, italic, _family) = parse_font_attributes("Arial-Oblique");
    assert!(!bold);
    assert!(italic);

    // Heavy weight
    let (_name, bold, _italic, _family) = parse_font_attributes("Futura-Heavy");
    assert!(bold);
}

#[test]
fn detect_text_columns_single() {
    // Single-column page: all text on the left half
    let frags = vec![TextFragment {
        text: "Hello".into(),
        x: 50.0,
        y: 700.0,
        width: 100.0,
        height: 12.0,
        rotation_degrees: 0.0,
        font_size: 12.0,
        font_name: "F1".into(),
        color: [0.0; 3],
        opacity: 1.0,
        invisible: false,
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
        base_font: String::new(),
        space_advance: 0.0,
        tf_font_size: 12.0,
        tm_y_scale: 1.0,
        source_stream: None,
        source_op_start: None,
        source_op_end: None,
        source_xobject: None,
        tm_origin_x: None,
        tm_origin_y: None,
        tm_x_scale: None,
        tm_lm_x: None,
        tm_lm_y: None,
    }];
    let zones = detect_text_columns(&frags, 595.0);
    assert_eq!(zones.len(), 1);

    // Empty input → empty
    assert!(detect_text_columns(&[], 595.0).is_empty());
}

#[test]
fn detect_text_columns_two_columns() {
    // Two fragments with a 100pt gap between them
    let left = TextFragment {
        text: "Left".into(),
        x: 50.0,
        y: 700.0,
        width: 150.0,
        height: 12.0,
        rotation_degrees: 0.0,
        font_size: 12.0,
        font_name: "F1".into(),
        color: [0.0; 3],
        opacity: 1.0,
        invisible: false,
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
        base_font: String::new(),
        space_advance: 0.0,
        tf_font_size: 12.0,
        tm_y_scale: 1.0,
        source_stream: None,
        source_op_start: None,
        source_op_end: None,
        source_xobject: None,
        tm_origin_x: None,
        tm_origin_y: None,
        tm_x_scale: None,
        tm_lm_x: None,
        tm_lm_y: None,
    };
    let right = TextFragment {
        text: "Right".into(),
        x: 350.0,
        y: 700.0,
        width: 150.0,
        height: 12.0,
        rotation_degrees: 0.0,
        font_size: 12.0,
        font_name: "F1".into(),
        color: [0.0; 3],
        opacity: 1.0,
        invisible: false,
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
        base_font: String::new(),
        space_advance: 0.0,
        tf_font_size: 12.0,
        tm_y_scale: 1.0,
        source_stream: None,
        source_op_start: None,
        source_op_end: None,
        source_xobject: None,
        tm_origin_x: None,
        tm_origin_y: None,
        tm_x_scale: None,
        tm_lm_x: None,
        tm_lm_y: None,
    };
    let zones = detect_text_columns(&[left, right], 595.0);
    assert_eq!(zones.len(), 2, "expected two columns, got {:?}", zones);
    assert!(zones[0].x_start < zones[1].x_start);
}

fn make_frag(text: &str, x: f32, y: f32, w: f32, fs: f32) -> TextFragment {
    TextFragment {
        text: text.into(),
        x,
        y,
        width: w,
        height: fs,
        rotation_degrees: 0.0,
        font_size: fs,
        font_name: "F1".into(),
        color: [0.0; 3],
        opacity: 1.0,
        invisible: false,
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
        base_font: String::new(),
        space_advance: 0.0,
        tf_font_size: fs,
        tm_y_scale: 1.0,
        source_stream: None,
        source_op_start: None,
        source_op_end: None,
        source_xobject: None,
        tm_origin_x: None,
        tm_origin_y: None,
        tm_x_scale: None,
        tm_lm_x: None,
        tm_lm_y: None,
    }
}

#[test]
fn extract_table_cells_single_column() {
    // Three rows, one column.
    let frags = vec![
        make_frag("Header", 50.0, 700.0, 80.0, 12.0),
        make_frag("Row 1", 50.0, 680.0, 60.0, 12.0),
        make_frag("Row 2", 50.0, 660.0, 60.0, 12.0),
    ];
    let cells = extract_table_cells(&frags, 595.0, 842.0);
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[0].row, 0);
    assert_eq!(cells[0].col, 0);
    assert_eq!(cells[1].row, 1);
    assert_eq!(cells[2].row, 2);
    assert_eq!(cells[0].text, "Header");
}

#[test]
fn extract_table_cells_two_columns() {
    // Two rows × two columns (100 pt gap between columns).
    let frags = vec![
        make_frag("A1", 50.0, 700.0, 80.0, 12.0),
        make_frag("B1", 300.0, 700.0, 80.0, 12.0),
        make_frag("A2", 50.0, 680.0, 80.0, 12.0),
        make_frag("B2", 300.0, 680.0, 80.0, 12.0),
    ];
    let cells = extract_table_cells(&frags, 595.0, 842.0);
    assert_eq!(cells.len(), 4);
    // Row 0, col 0 should be "A1"
    let a1 = cells.iter().find(|c| c.row == 0 && c.col == 0).unwrap();
    assert_eq!(a1.text, "A1");
    // Row 0, col 1 should be "B1"
    let b1 = cells.iter().find(|c| c.row == 0 && c.col == 1).unwrap();
    assert_eq!(b1.text, "B1");
}

#[test]
fn extract_table_cells_merges_same_cell_fragments() {
    // Two fragments on the same line, same column → merged into one cell.
    let frags = vec![
        make_frag("Hello", 50.0, 700.0, 30.0, 12.0),
        make_frag("World", 85.0, 700.0, 30.0, 12.0),
    ];
    let cells = extract_table_cells(&frags, 595.0, 842.0);
    assert_eq!(cells.len(), 1);
    assert!(cells[0].text.contains("Hello"));
    assert!(cells[0].text.contains("World"));
}

#[test]
fn extract_table_cells_empty_returns_empty() {
    assert!(extract_table_cells(&[], 595.0, 842.0).is_empty());
    assert!(extract_table_cells(&[], 0.0, 842.0).is_empty());
}

#[test]
fn group_text_fragments_raw() {
    let frags = vec![
        make_frag("A", 50.0, 700.0, 20.0, 12.0),
        make_frag("B", 80.0, 700.0, 20.0, 12.0),
    ];
    let groups = group_text_fragments(&frags, GroupingStrategy::Raw);
    assert_eq!(groups.len(), 2);
}

#[test]
fn group_text_fragments_line() {
    let frags = vec![
        make_frag("A", 50.0, 700.0, 20.0, 12.0),
        make_frag("B", 80.0, 700.0, 20.0, 12.0), // same line
        make_frag("C", 50.0, 680.0, 20.0, 12.0), // new line (gap > 6pt)
    ];
    let groups = group_text_fragments(&frags, GroupingStrategy::Line);
    assert_eq!(groups.len(), 2, "expected 2 lines, got {}", groups.len());
    assert!(groups[0].text.contains('A') && groups[0].text.contains('B'));
}

#[test]
fn group_text_fragments_paragraph() {
    // Three lines: first two close together (same paragraph), third far below.
    let frags = vec![
        make_frag("L1", 50.0, 700.0, 20.0, 12.0),
        make_frag("L2", 50.0, 686.0, 20.0, 12.0), // gap=14, < 1.5×12=18 → same paragraph
        make_frag("L3", 50.0, 630.0, 20.0, 12.0), // gap=56, > 18 → new paragraph
    ];
    let groups = group_text_fragments(&frags, GroupingStrategy::Paragraph);
    assert_eq!(
        groups.len(),
        2,
        "expected 2 paragraphs, got {}",
        groups.len()
    );
    assert!(groups[0].text.contains("L1") && groups[0].text.contains("L2"));
    assert!(groups[1].text.contains("L3"));
}

// Chrome/Skia PDFs put /Resources on a parent /Pages node rather than on each
// page dict.  extract_text_from_xobjects() must walk up the /Parent chain to find
// /Resources/XObject; without the fix it returns early and produces zero fragments.
#[test]
fn extract_xobjects_from_inherited_resources() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();

    // Type1 font with no explicit /Encoding → StandardEncoding fallback.
    // ASCII bytes H(72) e(101) l(108) l(108) o(111) map to "Hello".
    let mut font_d = Dictionary::new();
    font_d.set("Type", Object::Name(b"Font".to_vec()));
    font_d.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font_d));

    // Form XObject with its own /Resources/Font and a text content stream.
    let mut xobj_font_d = Dictionary::new();
    xobj_font_d.set("F1", Object::Reference(font_id));
    let mut xobj_res = Dictionary::new();
    xobj_res.set("Font", Object::Dictionary(xobj_font_d));
    let mut xobj_d = Dictionary::new();
    xobj_d.set("Type", Object::Name(b"XObject".to_vec()));
    xobj_d.set("Subtype", Object::Name(b"Form".to_vec()));
    xobj_d.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    xobj_d.set("Resources", Object::Dictionary(xobj_res));
    let xobj_id = doc.add_object(Object::Stream(Stream::new(
        xobj_d,
        b"BT /F1 12 Tf (Hello) Tj ET".to_vec(),
    )));

    // Minimal page content stream — text lives in the XObject, not here.
    let content_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"q Q".to_vec(),
    )));

    // Page node with NO /Resources — Chrome/Skia style (inherits from Pages).
    let mut page_d = Dictionary::new();
    page_d.set("Type", Object::Name(b"Page".to_vec()));
    page_d.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_d.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(Object::Dictionary(page_d));

    // Pages node: /Resources/XObject here (NOT on the page dict).
    let mut xobj_dict = Dictionary::new();
    xobj_dict.set("X1", Object::Reference(xobj_id));
    let mut pages_res = Dictionary::new();
    pages_res.set("XObject", Object::Dictionary(xobj_dict));
    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_d.set("Count", Object::Integer(1));
    pages_d.set("Resources", Object::Dictionary(pages_res));
    let pages_id = doc.add_object(Object::Dictionary(pages_d));

    // Wire up /Parent.
    if let Ok(obj) = doc.get_object_mut(page_id) {
        if let Ok(d) = obj.as_dict_mut() {
            d.set("Parent", Object::Reference(pages_id));
        }
    }

    // Catalog.
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    let text: String = frags
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !frags.is_empty(),
        "expected text from XObject with inherited /Resources, got none"
    );
    assert!(
        text.contains("Hello"),
        "expected 'Hello' in extracted text, got: {text:?}"
    );
}

// Validates the *real* Chrome/Skia decode path: Type0/CID font with Identity-H
// encoding, ToUnicode CMap, and 2-byte hex glyph IDs (<XXXX> Tj), all inside a
// Form XObject discovered via an inherited /Resources on the parent /Pages node.
//
// This is the path that matters for P1 (InPlace replace_text on Chrome/Skia PDFs).
// The previous test used Type1/literal strings — a completely different decode branch.
#[test]
fn extract_cid_xobject_inherited_resources() {
    use lopdf::{Document, Stream};

    // GID→Unicode mapping used in this test:
    //   GID 0x0048 → 'H',  GID 0x0069 → 'i'
    // Content stream will be <00480069> Tj  (2 CIDs, 4 hex bytes).
    let cmap_bytes = b"/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-H def\n\
         /CMapType 1 def\n\
         2 beginbfchar\n\
         <0048> <0048>\n\
         <0069> <0069>\n\
         endbfchar\n\
         endcmap\n\
         end end\n"
        .to_vec();

    let mut doc = Document::new();

    // ToUnicode CMap stream.
    let cmap_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), cmap_bytes)));

    // CIDFontType2 (descendant font).
    let mut cidfont_d = Dictionary::new();
    cidfont_d.set("Type", Object::Name(b"Font".to_vec()));
    cidfont_d.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
    cidfont_d.set("BaseFont", Object::Name(b"TestCIDFont".to_vec()));
    {
        let mut cidsys = Dictionary::new();
        cidsys.set(
            "Registry",
            Object::String(b"Adobe".to_vec(), lopdf::StringFormat::Literal),
        );
        cidsys.set(
            "Ordering",
            Object::String(b"Identity".to_vec(), lopdf::StringFormat::Literal),
        );
        cidsys.set("Supplement", Object::Integer(0));
        cidfont_d.set("CIDSystemInfo", Object::Dictionary(cidsys));
    }
    cidfont_d.set("DW", Object::Integer(1000));
    let cidfont_id = doc.add_object(Object::Dictionary(cidfont_d));

    // Type0 font dict.
    let mut font_d = Dictionary::new();
    font_d.set("Type", Object::Name(b"Font".to_vec()));
    font_d.set("Subtype", Object::Name(b"Type0".to_vec()));
    font_d.set("BaseFont", Object::Name(b"TestCIDFont".to_vec()));
    font_d.set("Encoding", Object::Name(b"Identity-H".to_vec()));
    font_d.set(
        "DescendantFonts",
        Object::Array(vec![Object::Reference(cidfont_id)]),
    );
    font_d.set("ToUnicode", Object::Reference(cmap_id));
    let font_id = doc.add_object(Object::Dictionary(font_d));

    // Form XObject: /Resources/Font has F1, content stream uses 2-byte CID hex.
    // <00480069> encodes GID 0x0048 ('H') and GID 0x0069 ('i').
    let mut xobj_font_d = Dictionary::new();
    xobj_font_d.set("F1", Object::Reference(font_id));
    let mut xobj_res = Dictionary::new();
    xobj_res.set("Font", Object::Dictionary(xobj_font_d));
    let mut xobj_d = Dictionary::new();
    xobj_d.set("Type", Object::Name(b"XObject".to_vec()));
    xobj_d.set("Subtype", Object::Name(b"Form".to_vec()));
    xobj_d.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    xobj_d.set("Resources", Object::Dictionary(xobj_res));
    let xobj_id = doc.add_object(Object::Stream(Stream::new(
        xobj_d,
        b"BT /F1 12 Tf <00480069> Tj ET".to_vec(),
    )));

    // Minimal page content stream.
    let content_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"q Q".to_vec(),
    )));

    // Page node with NO /Resources (inherits from Pages).
    let mut page_d = Dictionary::new();
    page_d.set("Type", Object::Name(b"Page".to_vec()));
    page_d.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_d.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(Object::Dictionary(page_d));

    // Pages node: /Resources/XObject here, NOT on page dict.
    let mut xobj_dict = Dictionary::new();
    xobj_dict.set("X1", Object::Reference(xobj_id));
    let mut pages_res = Dictionary::new();
    pages_res.set("XObject", Object::Dictionary(xobj_dict));
    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_d.set("Count", Object::Integer(1));
    pages_d.set("Resources", Object::Dictionary(pages_res));
    let pages_id = doc.add_object(Object::Dictionary(pages_d));

    if let Ok(obj) = doc.get_object_mut(page_id) {
        if let Ok(d) = obj.as_dict_mut() {
            d.set("Parent", Object::Reference(pages_id));
        }
    }

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    let text: String = frags
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !frags.is_empty(),
        "expected CID text from XObject with inherited /Resources, got none"
    );
    assert!(
        text.contains("Hi"),
        "expected 'Hi' from CID+hex decode, got: {text:?}"
    );

    // Without /ToUnicode, the Identity-H path remains best-effort for the
    // compatibility API, but verbose extraction must report that inference.
    if let Ok(obj) = doc.get_object_mut(font_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.remove(b"ToUnicode");
    }
    let (_, warnings) = extract_text_runs_from_page_verbose(&doc, page_id).unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::MissingToUnicodeCMap)),
        "expected MissingToUnicodeCMap warning, got: {warnings:?}"
    );
}

#[test]
fn verbose_extraction_reports_unsupported_font_subtype() {
    let mut doc = lopdf::Document::new();
    let mut font = lopdf::Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type9".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font));

    let mut resources = lopdf::Dictionary::new();
    resources.set("F9", Object::Reference(font_id));
    let mut fonts = HashMap::new();
    let mut warnings = Vec::new();
    collect_font_dict_entries_verbose(&doc, &resources, &mut fonts, &mut warnings);

    assert!(fonts.is_empty());
    assert!(warnings.iter().any(|warning| {
        matches!(warning.kind, WarningKind::UnsupportedFontSubtype)
            && warning.message.contains("/F9")
            && warning.message.contains("Type9")
    }));
}

#[test]
fn verbose_extraction_reports_identity_v_boundary() {
    let mut doc = lopdf::Document::new();
    let mut font = lopdf::Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type0".to_vec()));
    font.set("Encoding", Object::Name(b"Identity-V".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font));

    let mut resources = lopdf::Dictionary::new();
    resources.set("FV", Object::Reference(font_id));
    let mut fonts = HashMap::new();
    let mut warnings = Vec::new();
    collect_font_dict_entries_verbose(&doc, &resources, &mut fonts, &mut warnings);

    assert!(warnings.iter().any(|warning| {
        matches!(warning.kind, WarningKind::UnsupportedVerticalWriting)
            && warning.message.contains("/FV")
            && warning.message.contains("Identity-V")
    }));
}

// Verify that a non-identity CTM established by q/cm/Q is correctly applied to
// TextFragment coordinates.  Chrome/Skia PDFs use this pattern:
//
//   q
//   0.24 0 0 -0.24 0 841 cm   ← scale + Y-flip
//   BT /F1 100 Tf 100 200 Td (A) Tj ET
//   Q
//
// Expected page coords: x_page = 0.24*100 = 24, y_page = -0.24*200 + 841 = 793.
// Expected font_size in page space: 100 * 0.24 = 24.
#[test]
fn ctm_transforms_coordinates_to_page_space() {
    // Minimal font map: "F1" is a Type1 font with a single-byte 0x41 → 'A' mapping.
    let mut to_unicode = BTreeMap::new();
    to_unicode.insert(0x41u16, 'A');
    let mut fonts = HashMap::new();
    fonts.insert(
        b"F1".to_vec(),
        FontInfo {
            to_unicode,
            to_unicode_text: BTreeMap::new(),
            dw: 1000,
            w_runs: vec![WidthRun {
                start_gid: 0x41,
                widths: vec![600],
            }],
            bytes_per_char: 1,
            identity_fallback: false,
            is_bold: false,
            is_italic: false,
            font_family: String::new(),
            base_font: String::new(),
        },
    );

    // Content stream: q → cm (0.24 scale + Y-flip) → BT/Tj/ET → Q
    let stream = b"q\n\
        0.24 0 0 -0.24 0 841 cm\n\
        BT\n\
        /F1 100 Tf\n\
        100 200 Td\n\
        (A) Tj\n\
        ET\n\
        Q\n";

    let mut state = ParseCarryState::default();
    let mut frags: Vec<TextFragment> = Vec::new();
    parse_content_stream(
        stream,
        &fonts,
        &HashMap::new(),
        &mut state,
        &mut frags,
        Some(0),
        None,
    );

    assert_eq!(frags.len(), 1, "expected one TextFragment");
    let f = &frags[0];
    let eps = 0.5;
    assert!(
        (f.x - 24.0).abs() < eps,
        "x should be ~24 (0.24*100), got {}",
        f.x
    );
    assert!(
        (f.y - 793.0).abs() < eps,
        "y should be ~793 (-0.24*200 + 841), got {}",
        f.y
    );
    assert!(
        (f.font_size - 24.0).abs() < eps,
        "font_size should be ~24 (100*0.24), got {}",
        f.font_size
    );
}

// Verify that the CTM captured at Do time is forwarded to state.ctm so that
// extract_text_from_xobjects() can use it.  Chrome/Skia pattern:
//   q
//   0.24 0 0 -0.24 0 841 cm
//   /Fm0 Do
//   Q
// After parse_content_stream, state.ctm must be the scaled+flipped matrix.
#[test]
fn ctm_at_do_captured_in_state() {
    let fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
    let stream = b"q\n0.24 0 0 -0.24 0 841 cm\n/Fm0 Do\nQ\n";
    let mut state = ParseCarryState::default();
    let mut frags: Vec<TextFragment> = Vec::new();
    parse_content_stream(
        stream,
        &fonts,
        &HashMap::new(),
        &mut state,
        &mut frags,
        Some(0),
        None,
    );

    let eps = 1e-5f32;
    assert!(
        (state.ctm[0] - 0.24).abs() < eps,
        "ctm[0] should be 0.24, got {}",
        state.ctm[0]
    );
    assert!(
        (state.ctm[3] - -0.24).abs() < eps,
        "ctm[3] should be -0.24, got {}",
        state.ctm[3]
    );
    assert!(
        (state.ctm[5] - 841.0).abs() < eps,
        "ctm[5] should be 841, got {}",
        state.ctm[5]
    );
}

// Regression test for the PScript5.dll/Distiller pattern where a Form XObject
// has its own /Resources dict (e.g. /ProcSet only) but no /Font sub-entry.
// The XObject's content stream references fonts by name expecting them to be
// resolved from the parent page's /Resources/Font dictionary.
//
// Before the fix, xobject_fonts() returned an empty map (the /Resources dict
// existed so unwrap_or_else never fired), causing all text to be silently dropped.
#[test]
fn extract_xobject_font_from_page_resources() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();

    // Font lives on the PAGE, not inside the XObject.
    let mut font_d = Dictionary::new();
    font_d.set("Type", Object::Name(b"Font".to_vec()));
    font_d.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font_d));

    // Form XObject: has /Resources dict but NO /Font entry (only /ProcSet).
    // Its content stream uses /F1 which is defined on the page, not here.
    let mut xobj_res = Dictionary::new();
    xobj_res.set(
        "ProcSet",
        Object::Array(vec![
            Object::Name(b"PDF".to_vec()),
            Object::Name(b"Text".to_vec()),
        ]),
    );
    let mut xobj_d = Dictionary::new();
    xobj_d.set("Type", Object::Name(b"XObject".to_vec()));
    xobj_d.set("Subtype", Object::Name(b"Form".to_vec()));
    xobj_d.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    xobj_d.set("Resources", Object::Dictionary(xobj_res));
    let xobj_id = doc.add_object(Object::Stream(Stream::new(
        xobj_d,
        b"BT /F1 12 Tf (Hello) Tj ET".to_vec(),
    )));

    // Page content: just invokes the XObject via Do.
    let content_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"/X1 Do".to_vec(),
    )));

    // Page has /Resources/Font with F1 and /Resources/XObject with X1.
    let mut font_dict = Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut xobj_dict = Dictionary::new();
    xobj_dict.set("X1", Object::Reference(xobj_id));
    let mut page_res = Dictionary::new();
    page_res.set("Font", Object::Dictionary(font_dict));
    page_res.set("XObject", Object::Dictionary(xobj_dict));

    let mut page_d = Dictionary::new();
    page_d.set("Type", Object::Name(b"Page".to_vec()));
    page_d.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_d.set("Resources", Object::Dictionary(page_res));
    page_d.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(Object::Dictionary(page_d));

    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_d.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(pages_d));

    if let Ok(obj) = doc.get_object_mut(page_id) {
        if let Ok(d) = obj.as_dict_mut() {
            d.set("Parent", Object::Reference(pages_id));
        }
    }

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    let text: String = frags
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("Hello"),
        "XObject referencing page-level font must extract text; got: {text:?}"
    );
}

// Regression test for Distiller/PScript5 PDFs that split a single BT…ET block
// across multiple content streams.  Stream A opens BT (no ET), stream B continues
// with Tj operators.  Before the fix, `in_bt` was reset to false at the start of
// each parse_content_stream call, so all Tj in stream B were silently dropped.
#[test]
fn extract_cross_stream_bt_tj() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();

    // Font on the page.
    let mut font_d = Dictionary::new();
    font_d.set("Type", Object::Name(b"Font".to_vec()));
    font_d.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font_d));

    // Stream A: opens BT, sets font, but does NOT close with ET.
    let stream_a_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"BT /F1 12 Tf 100 700 Td".to_vec(),
    )));

    // Stream B: continues inside the unclosed BT; emits text, then closes ET.
    let stream_b_id = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        b"(Hello) Tj ET".to_vec(),
    )));

    // Page with /Contents as an array of two streams.
    let mut font_dict = Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut page_res = Dictionary::new();
    page_res.set("Font", Object::Dictionary(font_dict));

    let mut page_d = Dictionary::new();
    page_d.set("Type", Object::Name(b"Page".to_vec()));
    page_d.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_d.set("Resources", Object::Dictionary(page_res));
    page_d.set(
        "Contents",
        Object::Array(vec![
            Object::Reference(stream_a_id),
            Object::Reference(stream_b_id),
        ]),
    );
    let page_id = doc.add_object(Object::Dictionary(page_d));

    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_d.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(pages_d));

    if let Ok(obj) = doc.get_object_mut(page_id) {
        if let Ok(d) = obj.as_dict_mut() {
            d.set("Parent", Object::Reference(pages_id));
        }
    }

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    let text: String = frags
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("Hello"),
        "text inside BT split across streams must be extracted; got: {text:?}"
    );
}

#[test]
fn text_fragment_bounds_empty() {
    assert!(text_fragment_bounds(&[]).is_none());
}

#[test]
fn text_fragment_bounds_single() {
    let frag = make_frag("A", 100.0, 700.0, 50.0, 12.0);
    let [x, y, w, h] = text_fragment_bounds(&[frag]).unwrap();
    let eps = 0.01;
    assert!((x - 100.0).abs() < eps, "x={x}");
    assert!((y - (700.0 - 12.0 * 0.25)).abs() < eps, "y={y}");
    assert!((w - 50.0).abs() < eps, "w={w}");
    assert!((h - 12.0).abs() < eps, "h={h}"); // 0.75 + 0.25 = 1.0 × font_size
}

#[test]
fn text_fragment_bounds_multiple() {
    // Two fragments at different positions.
    let a = make_frag("A", 50.0, 700.0, 40.0, 12.0);
    let b = make_frag("B", 200.0, 680.0, 60.0, 14.0);
    let [x, y, w, h] = text_fragment_bounds(&[a, b]).unwrap();
    let eps = 0.01;
    // x_min = 50, x_max = 260 → width = 210
    assert!((x - 50.0).abs() < eps, "x={x}");
    assert!((w - 210.0).abs() < eps, "w={w}");
    // y_min = min(700-3, 680-3.5) = 676.5
    let expected_y_min = f32::min(700.0 - 12.0 * 0.25, 680.0 - 14.0 * 0.25);
    assert!((y - expected_y_min).abs() < eps, "y={y}");
    // y_max = max(700+9, 680+10.5) = 709
    let expected_y_max = f32::max(700.0 + 12.0 * 0.75, 680.0 + 14.0 * 0.75);
    assert!((h - (expected_y_max - expected_y_min)).abs() < eps, "h={h}");
}

// Regression: in a single BT block, Tm sets the column anchor (tm_origin_x).
// Subsequent Td+Tj rows drift in x but ALL share the same tm_origin_x.
// This models PDFs like GHS SDS where vertically-aligned labels are produced by
//   Tm → Tj → Td → Tj → Td → …
// within one BT block (no fresh Tm per row).
#[test]
fn tm_origin_preserves_column_anchor() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();

    // Type1 font (ASCII, StandardEncoding).
    let mut font_d = Dictionary::new();
    font_d.set("Type", Object::Name(b"Font".to_vec()));
    font_d.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font_d));

    // Single BT block: Tm sets x=100, then Td moves to next line (x accumulates).
    // Row 1: "AB" at x=100; after Tj, x ≈ 100 + advance
    // Td 0 -14 keeps x; Row 2: "C" starts at x ≈ 100 + advance (drifted)
    // All fragments must have tm_origin_x = 100.
    let stream_bytes = b"BT /F1 12 Tf 100 700 Td (AB) Tj 0 -14 Td (C) Tj ET".to_vec();

    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), stream_bytes)));

    let mut font_dict = Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut page_res = Dictionary::new();
    page_res.set("Font", Object::Dictionary(font_dict));

    let mut page_d = Dictionary::new();
    page_d.set("Type", Object::Name(b"Page".to_vec()));
    page_d.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_d.set("Resources", Object::Dictionary(page_res));
    page_d.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(Object::Dictionary(page_d));

    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_d.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(pages_d));

    if let Ok(obj) = doc.get_object_mut(page_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id));
    }

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    assert!(!frags.is_empty(), "expected fragments");

    // All fragments must have tm_origin_x set to 100.0 (the Td-set x in this case,
    // because Td with absolute operands acts as initial positioning before Tm).
    // Actually: "100 700 Td" is NOT a Tm — it's a Td with tx=100, ty=700.
    // Td doesn't set tm_origin. tm_origin_x is only set by Tm.
    // So tm_origin_x should be None for all fragments (no Tm was used).
    for f in &frags {
        assert!(
            f.tm_origin_x.is_none(),
            "no Tm in stream → tm_origin_x should be None, got {:?}",
            f.tm_origin_x
        );
    }

    // Now test with actual Tm operator.
    let mut doc2 = Document::new();
    let font_id2 = doc2.add_object(Object::Dictionary({
        let mut d = lopdf::Dictionary::new();
        d.set("Type", Object::Name(b"Font".to_vec()));
        d.set("Subtype", Object::Name(b"Type1".to_vec()));
        d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        d
    }));

    // BT block with Tm → Tj → Td → Tj
    // After Tm, tm_origin_x = 100 (in page space).
    // After first Tj, x advances; Td keeps x; second Tj starts at drifted x.
    // Both fragments should have tm_origin_x = 100.
    let stream2 = b"BT /F1 12 Tf 1 0 0 1 100 700 Tm (AB) Tj 0 -14 Td (C) Tj ET".to_vec();
    let cid2 = doc2.add_object(Object::Stream(Stream::new(Dictionary::new(), stream2)));

    let mut fd2 = lopdf::Dictionary::new();
    fd2.set("F1", Object::Reference(font_id2));
    let mut pr2 = lopdf::Dictionary::new();
    pr2.set("Font", Object::Dictionary(fd2));

    let mut pg2 = lopdf::Dictionary::new();
    pg2.set("Type", Object::Name(b"Page".to_vec()));
    pg2.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    pg2.set("Resources", Object::Dictionary(pr2));
    pg2.set("Contents", Object::Reference(cid2));
    let page_id2 = doc2.add_object(Object::Dictionary(pg2));

    let mut ps2 = lopdf::Dictionary::new();
    ps2.set("Type", Object::Name(b"Pages".to_vec()));
    ps2.set("Kids", Object::Array(vec![Object::Reference(page_id2)]));
    ps2.set("Count", Object::Integer(1));
    let pages_id2 = doc2.add_object(Object::Dictionary(ps2));

    if let Ok(obj) = doc2.get_object_mut(page_id2)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id2));
    }

    let mut cat2 = lopdf::Dictionary::new();
    cat2.set("Type", Object::Name(b"Catalog".to_vec()));
    cat2.set("Pages", Object::Reference(pages_id2));
    let cat_id2 = doc2.add_object(Object::Dictionary(cat2));
    doc2.trailer.set("Root", Object::Reference(cat_id2));

    let frags2 = extract_text_runs_from_page(&doc2, page_id2).unwrap();
    assert!(!frags2.is_empty(), "expected fragments with Tm");

    // All fragments from same BT block with same Tm must share tm_origin_x = 100.
    for f in &frags2 {
        let ox = f.tm_origin_x.unwrap_or(f32::NAN);
        assert!(
            (ox - 100.0).abs() < 0.5,
            "tm_origin_x should be ~100, got {ox} for {:?}",
            f.text
        );
    }

    // Per PDF spec, Td resets T_m to T_lm_new (no glyph-advance drift carries over).
    // After "0 -14 Td" with Tm scale=1 and T_lm_x=100: new T_lm_x = 0*1 + 100 = 100.
    // The second row must start at x=100, not at first_row_end_x.
    let rows: Vec<_> = frags2.iter().collect();
    if rows.len() >= 2 {
        let second_row_x = rows[rows.len() - 1].x;
        assert!(
            (second_row_x - 100.0).abs() < 1.0,
            "after Td(0,-14) x should reset to T_lm_x=100, got {second_row_x}"
        );
        // tm_lm_x for the second row should also be 100
        let second_lm_x = rows[rows.len() - 1].tm_lm_x.unwrap_or(f32::NAN);
        assert!(
            (second_lm_x - 100.0).abs() < 0.5,
            "second row tm_lm_x should be 100, got {second_lm_x}"
        );
    }
}

// Regression: tm_x_scale is exposed correctly and Td offsets are scaled by Tm.
//
// Test A: uniform Tm scale → tm_x_scale = Some(scale), and a horizontal Td jump
//         of tx units results in x advancing by tx * scale in page space.
// Test B: non-uniform Tm (x_scale ≠ y_scale) → tm_x_scale ≠ tm_y_scale and
//         width uses x_scale while height uses y_scale.
// Test C: Td-only BT block (no Tm) → tm_x_scale = None.
#[test]
fn tm_x_scale_and_td_scaling() {
    use lopdf::{Document, Stream};

    fn make_doc(stream_bytes: Vec<u8>) -> (Document, lopdf::ObjectId) {
        let mut doc = Document::new();
        let mut fd = lopdf::Dictionary::new();
        fd.set("Type", Object::Name(b"Font".to_vec()));
        fd.set("Subtype", Object::Name(b"Type1".to_vec()));
        fd.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        let font_id = doc.add_object(Object::Dictionary(fd));

        let cid = doc.add_object(Object::Stream(Stream::new(
            lopdf::Dictionary::new(),
            stream_bytes,
        )));

        let mut font_dict = lopdf::Dictionary::new();
        font_dict.set("F1", Object::Reference(font_id));
        let mut res = lopdf::Dictionary::new();
        res.set("Font", Object::Dictionary(font_dict));

        let mut pg = lopdf::Dictionary::new();
        pg.set("Type", Object::Name(b"Page".to_vec()));
        pg.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(595),
                Object::Integer(842),
            ]),
        );
        pg.set("Resources", Object::Dictionary(res));
        pg.set("Contents", Object::Reference(cid));
        let page_id = doc.add_object(Object::Dictionary(pg));

        let mut ps = lopdf::Dictionary::new();
        ps.set("Type", Object::Name(b"Pages".to_vec()));
        ps.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        ps.set("Count", Object::Integer(1));
        let pages_id = doc.add_object(Object::Dictionary(ps));

        if let Ok(obj) = doc.get_object_mut(page_id)
            && let Ok(d) = obj.as_dict_mut()
        {
            d.set("Parent", Object::Reference(pages_id));
        }

        let mut cat = lopdf::Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        let cat_id = doc.add_object(Object::Dictionary(cat));
        doc.trailer.set("Root", Object::Reference(cat_id));

        (doc, page_id)
    }

    let eps = 1.0f32;

    // Test A: uniform Tm scale=10, then Td(5,0) → second fragment x = first_x + width_A + 5*10
    {
        // BT /F1 1 Tf  10 0 0 10 50 700 Tm  (A) Tj  5 0 Td  (B) Tj ET
        let stream = b"BT /F1 1 Tf 10 0 0 10 50 700 Tm (A) Tj 5 0 Td (B) Tj ET".to_vec();
        let (doc, page_id) = make_doc(stream);
        let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
        assert_eq!(frags.len(), 2, "expected 2 fragments (A and B)");

        // tm_x_scale should be Some(10.0) for both fragments
        for f in &frags {
            let xs = f.tm_x_scale.unwrap_or(f32::NAN);
            assert!(
                (xs - 10.0).abs() < 0.01,
                "tm_x_scale should be 10, got {xs}"
            );
        }

        // Per PDF spec, Td resets T_m to T_lm_new (glyph-advance of A does NOT carry).
        // After "5 0 Td" with T_lm_x=50 and tm_x_scale=10: T_lm_x_new = 5*10 + 50 = 100.
        // B must start at x=100, independent of A's advance.
        let x_b = frags[1].x;
        let expected_x_b = 50.0 + 5.0 * 10.0; // = 100.0
        assert!(
            (x_b - expected_x_b).abs() < eps,
            "Td(5,0) resets T_m to T_lm_new=100; got x_B={x_b}, expected={expected_x_b}"
        );
        // tm_lm_x for both fragments should equal T_lm at the time of each Tj:
        // A: T_lm_x=50 (from Tm); B: T_lm_x=100 (after Td)
        assert!(
            (frags[0].tm_lm_x.unwrap_or(f32::NAN) - 50.0).abs() < 0.5,
            "A.tm_lm_x should be 50 (Tm anchor), got {:?}",
            frags[0].tm_lm_x
        );
        assert!(
            (frags[1].tm_lm_x.unwrap_or(f32::NAN) - 100.0).abs() < 0.5,
            "B.tm_lm_x should be 100 (after Td), got {:?}",
            frags[1].tm_lm_x
        );
    }

    // Test B: non-uniform Tm (x_scale=5, y_scale=2) → width uses x_scale, height uses y_scale
    {
        // BT /F1 1 Tf  5 0 0 2 100 700 Tm  (A) Tj ET
        let stream = b"BT /F1 1 Tf 5 0 0 2 100 700 Tm (A) Tj ET".to_vec();
        let (doc, page_id) = make_doc(stream);
        let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
        assert!(!frags.is_empty());
        let f = &frags[0];

        // tm_x_scale = 5, tm_y_scale = 2
        let xs = f.tm_x_scale.unwrap_or(f32::NAN);
        assert!((xs - 5.0).abs() < 0.01, "tm_x_scale should be 5, got {xs}");
        assert!(
            (f.tm_y_scale - 2.0).abs() < 0.01,
            "tm_y_scale should be 2, got {}",
            f.tm_y_scale
        );

        // height (font_size) uses y_scale=2
        assert!(
            (f.height - 2.0).abs() < 0.01,
            "height should be ≈2 (y_scale), got {}",
            f.height
        );

        // width uses x_scale=5, so width > height
        assert!(
            f.width > f.height,
            "width (x_scale=5 based) should exceed height (y_scale=2 based); w={} h={}",
            f.width,
            f.height
        );
    }

    // Test C: no Tm → tm_x_scale = None
    {
        let stream = b"BT /F1 12 Tf 100 700 Td (A) Tj ET".to_vec();
        let (doc, page_id) = make_doc(stream);
        let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
        assert!(!frags.is_empty());
        for f in &frags {
            assert!(
                f.tm_x_scale.is_none(),
                "no Tm → tm_x_scale should be None, got {:?}",
                f.tm_x_scale
            );
        }
    }
}

// Regression: form/table PDF with Tm scale + Td column jumps.
//
// Pattern: single BT block, Tm sets scale, then alternating Td moves
// jump between label column (left) and value column (right).
// PDF spec says Td resets T_m to T_lm_new, so glyph-advance drift from
// the previous Tj does NOT carry into the next row's x position.
//
// Stream: Tm(10,50,700) → (Label1) Tj → 200 0 Td → (Value1) Tj
//                       → -200 -14 Td → (Label2) Tj → 200 0 Td → (Value2) Tj
//
// Expected column positions:
//   Label x ≈ 50           (T_lm_x after Tm or after -200 Td)
//   Value x ≈ 50+200*10=2050  (T_lm_x after 200 Td)
#[test]
fn form_pdf_column_stability() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();
    let mut fd = lopdf::Dictionary::new();
    fd.set("Type", Object::Name(b"Font".to_vec()));
    fd.set("Subtype", Object::Name(b"Type1".to_vec()));
    fd.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(fd));

    // BT /F1 1 Tf
    //   10 0 0 10 50 700 Tm   (scale=10, origin=(50,700))
    //   (Label1) Tj
    //   200 0 Td              (jump to value column: x = 200*10 + 50 = 2050)
    //   (Value1) Tj
    //   -200 -14 Td           (back to label, next row: x = -200*10 + 2050 = 50)
    //   (Label2) Tj
    //   200 0 Td              (value column again: x = 200*10 + 50 = 2050)
    //   (Value2) Tj
    // ET
    let stream_bytes = b"BT /F1 1 Tf 10 0 0 10 50 700 Tm \
          (A) Tj 200 0 Td (B) Tj -200 -14 Td (C) Tj 200 0 Td (D) Tj ET"
        .to_vec();

    let cid = doc.add_object(Object::Stream(Stream::new(
        lopdf::Dictionary::new(),
        stream_bytes,
    )));
    let mut font_dict = lopdf::Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut res = lopdf::Dictionary::new();
    res.set("Font", Object::Dictionary(font_dict));
    let mut pg = lopdf::Dictionary::new();
    pg.set("Type", Object::Name(b"Page".to_vec()));
    pg.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    pg.set("Resources", Object::Dictionary(res));
    pg.set("Contents", Object::Reference(cid));
    let page_id = doc.add_object(Object::Dictionary(pg));
    let mut ps = lopdf::Dictionary::new();
    ps.set("Type", Object::Name(b"Pages".to_vec()));
    ps.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    ps.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(ps));
    if let Ok(obj) = doc.get_object_mut(page_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id));
    }
    let mut cat = lopdf::Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(Object::Dictionary(cat));
    doc.trailer.set("Root", Object::Reference(cat_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    assert_eq!(frags.len(), 4, "expected 4 fragments A/B/C/D");

    // A = Label1, B = Value1, C = Label2, D = Value2
    let (a, b, c, d) = (&frags[0], &frags[1], &frags[2], &frags[3]);
    let eps = 1.0f32;

    // All fragments share the same Tm origin (50, 700).
    for f in &frags {
        let ox = f.tm_origin_x.unwrap_or(f32::NAN);
        assert!(
            (ox - 50.0).abs() < eps,
            "tm_origin_x should be 50, got {ox} for {:?}",
            f.text
        );
    }

    // Label column (A and C): x ≈ 50 (T_lm reset by Td or Tm)
    assert!((a.x - 50.0).abs() < eps, "A.x should be ≈50, got {}", a.x);
    assert!(
        (c.x - 50.0).abs() < eps,
        "C.x should be ≈50 after -200 Td, got {}",
        c.x
    );

    // Value column (B and D): x ≈ 2050 (= 200*10 + 50)
    let value_x = 200.0 * 10.0 + 50.0;
    assert!(
        (b.x - value_x).abs() < eps,
        "B.x should be ≈{value_x}, got {}",
        b.x
    );
    assert!(
        (d.x - value_x).abs() < eps,
        "D.x should be ≈{value_x}, got {}",
        d.x
    );

    // tm_lm_x: A=50, B=2050, C=50, D=2050
    let a_lm = a.tm_lm_x.unwrap_or(f32::NAN);
    let b_lm = b.tm_lm_x.unwrap_or(f32::NAN);
    let c_lm = c.tm_lm_x.unwrap_or(f32::NAN);
    let d_lm = d.tm_lm_x.unwrap_or(f32::NAN);
    assert!(
        (a_lm - 50.0).abs() < eps,
        "A.tm_lm_x should be 50, got {a_lm}"
    );
    assert!(
        (b_lm - value_x).abs() < eps,
        "B.tm_lm_x should be {value_x}, got {b_lm}"
    );
    assert!(
        (c_lm - 50.0).abs() < eps,
        "C.tm_lm_x should be 50 (row reset), got {c_lm}"
    );
    assert!(
        (d_lm - value_x).abs() < eps,
        "D.tm_lm_x should be {value_x}, got {d_lm}"
    );
}

// Test: extract_table_cells returns non-empty fragments and uses tm_lm_x
// for column assignment on form PDFs.
#[test]
fn extract_table_cells_has_fragments_and_tm_lm_cols() {
    use lopdf::{Document, Stream};

    // Build a minimal form-PDF with 2 columns via Tm + Td jumps.
    // Scale=10, label col at x=50, value col at x=50+200*10=2050.
    let stream_bytes = b"BT /F1 1 Tf 10 0 0 10 50 700 Tm \
          (Label) Tj 200 0 Td (Value) Tj ET"
        .to_vec();

    let mut doc = Document::new();
    let mut fd = lopdf::Dictionary::new();
    fd.set("Type", Object::Name(b"Font".to_vec()));
    fd.set("Subtype", Object::Name(b"Type1".to_vec()));
    fd.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(fd));
    let cid = doc.add_object(Object::Stream(Stream::new(
        lopdf::Dictionary::new(),
        stream_bytes,
    )));
    let mut font_dict = lopdf::Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut res = lopdf::Dictionary::new();
    res.set("Font", Object::Dictionary(font_dict));
    let mut pg = lopdf::Dictionary::new();
    pg.set("Type", Object::Name(b"Page".to_vec()));
    pg.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    pg.set("Resources", Object::Dictionary(res));
    pg.set("Contents", Object::Reference(cid));
    let page_id = doc.add_object(Object::Dictionary(pg));
    let mut ps = lopdf::Dictionary::new();
    ps.set("Type", Object::Name(b"Pages".to_vec()));
    ps.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    ps.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(ps));
    if let Ok(obj) = doc.get_object_mut(page_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id));
    }
    let mut cat = lopdf::Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(Object::Dictionary(cat));
    doc.trailer.set("Root", Object::Reference(cat_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();

    // extract_table_cells should use tm_lm_x mode and split into 2 columns.
    let cells = extract_table_cells(&frags, 595.0, 842.0);
    assert_eq!(
        cells.len(),
        2,
        "expected 2 cells (label col + value col), got {}",
        cells.len()
    );

    // Each cell must carry its source fragments.
    for c in &cells {
        assert!(
            !c.fragments.is_empty(),
            "cell ({},{}) has no fragments",
            c.row,
            c.col
        );
    }

    // Column 0 should be the label (x ≈ 50), column 1 the value (x ≈ 2050).
    let col0 = cells.iter().find(|c| c.col == 0).expect("col 0 missing");
    let col1 = cells.iter().find(|c| c.col == 1).expect("col 1 missing");
    assert!(
        (col0.x - 50.0).abs() < 5.0,
        "col0.x should be ≈50, got {}",
        col0.x
    );
    let value_x = 50.0 + 200.0 * 10.0;
    assert!(
        (col1.x - value_x).abs() < 5.0,
        "col1.x should be ≈{value_x}, got {}",
        col1.x
    );
    assert_eq!(col0.text.trim(), "Label");
    assert_eq!(col1.text.trim(), "Value");

    // bbox() convenience method.
    let b = col0.bbox();
    assert!((b[0] - col0.x).abs() < 0.01);
    assert!((b[1] - col0.y).abs() < 0.01);
    assert!((b[2] - col0.width).abs() < 0.01);
    assert!((b[3] - col0.height).abs() < 0.01);
}

// Regression #16: Tf operator after Tm must keep the Tm y-scale.
// Pattern: /F1 1 Tf  →  10 0 0 10 x y Tm  →  (A) Tj  →  /F1 1 Tf  →  (B) Tj
// Both fragments must report font_size ≈ 10, not 1.
#[test]
fn tf_after_tm_preserves_scale() {
    use lopdf::{Document, Stream};

    let mut doc = Document::new();
    let mut fd = lopdf::Dictionary::new();
    fd.set("Type", Object::Name(b"Font".to_vec()));
    fd.set("Subtype", Object::Name(b"Type1".to_vec()));
    fd.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(fd));

    // BT /F1 1 Tf  10 0 0 10 50 700 Tm  (A) Tj  /F1 1 Tf  (B) Tj ET
    let stream_bytes = b"BT /F1 1 Tf 10 0 0 10 50 700 Tm (A) Tj /F1 1 Tf (B) Tj ET".to_vec();

    let cid = doc.add_object(Object::Stream(Stream::new(
        lopdf::Dictionary::new(),
        stream_bytes,
    )));
    let mut font_dict = lopdf::Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut res = lopdf::Dictionary::new();
    res.set("Font", Object::Dictionary(font_dict));
    let mut pg = lopdf::Dictionary::new();
    pg.set("Type", Object::Name(b"Page".to_vec()));
    pg.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    pg.set("Resources", Object::Dictionary(res));
    pg.set("Contents", Object::Reference(cid));
    let page_id = doc.add_object(Object::Dictionary(pg));
    let mut ps = lopdf::Dictionary::new();
    ps.set("Type", Object::Name(b"Pages".to_vec()));
    ps.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    ps.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(ps));
    if let Ok(obj) = doc.get_object_mut(page_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id));
    }
    let mut cat = lopdf::Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(Object::Dictionary(cat));
    doc.trailer.set("Root", Object::Reference(cat_id));

    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    assert_eq!(frags.len(), 2, "expected 2 fragments");

    for f in &frags {
        assert!(
            (f.font_size - 10.0).abs() < 0.5,
            "font_size should be ≈10 (Tm scale persists after Tf), got {} for {:?}",
            f.font_size,
            f.text
        );
    }
}

// Enhancement #17: merge_short_cjk_tails joins short fragments.
#[test]
fn merge_short_cjk_tails_basic() {
    let make = |text: &str, x: f32, y: f32, w: f32, fs: f32| TextFragment {
        text: text.into(),
        x,
        y,
        width: w,
        height: fs,
        rotation_degrees: 0.0,
        font_size: fs,
        font_name: "F1".into(),
        color: [0.0; 3],
        opacity: 1.0,
        invisible: false,
        is_bold: false,
        is_italic: false,
        font_family: String::new(),
        base_font: String::new(),
        space_advance: 0.0,
        tf_font_size: fs,
        tm_y_scale: 1.0,
        tm_x_scale: None,
        source_stream: None,
        source_op_start: None,
        source_op_end: None,
        source_xobject: None,
        tm_origin_x: None,
        tm_origin_y: None,
        tm_lm_x: None,
        tm_lm_y: None,
    };

    // Three fragments: "保護眼鏡やゴーグルを着用する" (long), "る。" (2 chars, tail), "次の行" (long)
    let frags = vec![
        make("保護眼鏡やゴーグルを着用す", 50.0, 700.0, 100.0, 10.0),
        make("る。", 150.0, 700.0, 15.0, 10.0), // short tail, same y
        make("次の行テキスト", 50.0, 686.0, 80.0, 10.0), // long, new line
    ];

    let merged = merge_short_cjk_tails(&frags, 4, 1.7);

    assert_eq!(merged.len(), 2, "tail should be merged into predecessor");
    assert!(
        merged[0].text.contains("る。"),
        "merged text should contain tail"
    );
    assert_eq!(merged[1].text, "次の行テキスト");

    // With max_chars=0, no merging occurs.
    let no_merge = merge_short_cjk_tails(&frags, 0, 1.7);
    assert_eq!(no_merge.len(), 3);
}

// Helper: build a minimal one-page lopdf::Document with the given content stream bytes
// and a single Type1 font /F1.
fn make_test_page(content: &[u8]) -> (lopdf::Document, lopdf::ObjectId) {
    use lopdf::{Dictionary, Document, Stream};
    let mut doc = Document::new();
    let mut fd = Dictionary::new();
    fd.set("Type", Object::Name(b"Font".to_vec()));
    fd.set("Subtype", Object::Name(b"Type1".to_vec()));
    fd.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(fd));
    let cid = doc.add_object(Object::Stream(Stream::new(
        Dictionary::new(),
        content.to_vec(),
    )));
    let mut font_dict = Dictionary::new();
    font_dict.set("F1", Object::Reference(font_id));
    let mut res = Dictionary::new();
    res.set("Font", Object::Dictionary(font_dict));
    let mut pg = Dictionary::new();
    pg.set("Type", Object::Name(b"Page".to_vec()));
    pg.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    pg.set("Resources", Object::Dictionary(res));
    pg.set("Contents", Object::Reference(cid));
    let page_id = doc.add_object(Object::Dictionary(pg));
    let mut ps = Dictionary::new();
    ps.set("Type", Object::Name(b"Pages".to_vec()));
    ps.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    ps.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(Object::Dictionary(ps));
    if let Ok(obj) = doc.get_object_mut(page_id)
        && let Ok(d) = obj.as_dict_mut()
    {
        d.set("Parent", Object::Reference(pages_id));
    }
    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(Object::Dictionary(cat));
    doc.trailer.set("Root", Object::Reference(cat_id));
    (doc, page_id)
}

// T* operator moves y by -TL when TL is set via TL operator.
#[test]
fn t_star_moves_y_by_text_leading() {
    // BT  /F1 12 Tf  1 0 0 1 50 700 Tm  12 TL  (A) Tj  T*  (B) Tj  ET
    // Fragment A at y=700, fragment B at y = 700 - 12 = 688.
    let content = b"BT /F1 12 Tf 1 0 0 1 50 700 Tm 12 TL (A) Tj T* (B) Tj ET";
    let (doc, page_id) = make_test_page(content);
    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    assert_eq!(frags.len(), 2, "expected 2 fragments");
    assert!(
        (frags[0].y - 700.0).abs() < 1.0,
        "A.y should be ≈700, got {}",
        frags[0].y
    );
    assert!(
        (frags[1].y - 688.0).abs() < 1.0,
        "B.y should be ≈688 (700 - 12), got {}",
        frags[1].y
    );
}

// TD operator moves position AND sets TL = -ty, so subsequent T* uses the new leading.
#[test]
fn td_uppercase_sets_text_leading_for_t_star() {
    // BT  /F1 12 Tf  1 0 0 1 50 700 Tm  (A) Tj  0 -14 TD  (B) Tj  T*  (C) Tj  ET
    // A at 700, B at 686, C at 672.
    let content = b"BT /F1 12 Tf 1 0 0 1 50 700 Tm (A) Tj 0 -14 TD (B) Tj T* (C) Tj ET";
    let (doc, page_id) = make_test_page(content);
    let frags = extract_text_runs_from_page(&doc, page_id).unwrap();
    assert_eq!(frags.len(), 3, "expected 3 fragments");
    assert!(
        (frags[0].y - 700.0).abs() < 1.0,
        "A.y≈700, got {}",
        frags[0].y
    );
    assert!(
        (frags[1].y - 686.0).abs() < 1.0,
        "B.y≈686, got {}",
        frags[1].y
    );
    assert!(
        (frags[2].y - 672.0).abs() < 1.0,
        "C.y≈672, got {}",
        frags[2].y
    );
}

// Tc character spacing shifts the x cursor forward after each glyph.
// Two separate Tj operators: with Tc=5, the second fragment's x should be
// at least 5 pt further right than the advance alone would place it.
#[test]
fn tc_char_spacing_shifts_x_cursor() {
    // Without Tc: (A) Tj (B) Tj — B.x = 50 + advance(A)
    // With Tc=10: (A) Tj (B) Tj — B.x = 50 + advance(A) + 10
    let no_tc = b"BT /F1 12 Tf 1 0 0 1 50 700 Tm (A) Tj (B) Tj ET";
    let with_tc = b"BT /F1 12 Tf 1 0 0 1 50 700 Tm 10 Tc (A) Tj (B) Tj ET";

    let (doc0, pid0) = make_test_page(no_tc);
    let (doc1, pid1) = make_test_page(with_tc);

    let frags0 = extract_text_runs_from_page(&doc0, pid0).unwrap();
    let frags1 = extract_text_runs_from_page(&doc1, pid1).unwrap();

    assert_eq!(frags0.len(), 2);
    assert_eq!(frags1.len(), 2);

    let b_x_no_tc = frags0[1].x;
    let b_x_with_tc = frags1[1].x;
    assert!(
        b_x_with_tc > b_x_no_tc + 9.0,
        "Tc=10 should push B.x at least 10 pt further right; no_tc={b_x_no_tc}, with_tc={b_x_with_tc}"
    );
}

// ---------------------------------------------------------------------------
// classify_collisions — unit tests
// (in-crate: struct literals are legal here via `use super::*`)
// ---------------------------------------------------------------------------

fn make_region_for_classify(
    row: Option<usize>,
    col: Option<usize>,
    role: LayoutRegionRole,
) -> LayoutRegion {
    LayoutRegion {
        kind: LayoutRegionKind::TableCell,
        role,
        row,
        col,
        text: String::new(),
        source_bbox: [0.0, 0.0, 50.0, 10.0],
        usable_rect: [0.0, 0.0, 50.0, 10.0],
        fragments: vec![],
    }
}

fn make_collision_for_classify(index_a: usize, index_b: usize) -> Collision {
    Collision {
        index_a,
        index_b,
        overlap_rect: [0.0, 0.0, 10.0, 10.0],
        overlap_area: 100.0,
    }
}

#[test]
fn classify_same_region() {
    let regions = vec![
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::LeftLabel),
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::RightValue),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].kind, CollisionKind::SameRegion);
}

#[test]
fn classify_same_row() {
    let regions = vec![
        make_region_for_classify(Some(2), Some(0), LayoutRegionRole::LeftLabel),
        make_region_for_classify(Some(2), Some(1), LayoutRegionRole::RightValue),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::SameRow);
}

#[test]
fn classify_adjacent_row() {
    let regions = vec![
        make_region_for_classify(Some(3), Some(0), LayoutRegionRole::Unknown),
        make_region_for_classify(Some(4), Some(0), LayoutRegionRole::Unknown),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::AdjacentRow);
}

#[test]
fn classify_adjacent_row_reversed_indices() {
    let regions = vec![
        make_region_for_classify(Some(5), Some(0), LayoutRegionRole::Unknown),
        make_region_for_classify(Some(4), Some(0), LayoutRegionRole::Unknown),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::AdjacentRow);
}

#[test]
fn classify_same_column() {
    let regions = vec![
        make_region_for_classify(Some(1), Some(1), LayoutRegionRole::Unknown),
        make_region_for_classify(Some(3), Some(1), LayoutRegionRole::Unknown),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::SameColumn);
}

#[test]
fn classify_header_footer_trumps_same_row() {
    let regions = vec![
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::HeaderFooter),
        make_region_for_classify(Some(0), Some(1), LayoutRegionRole::RightValue),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::HeaderFooter);
}

#[test]
fn classify_header_footer_on_index_b() {
    let regions = vec![
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::Unknown),
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::HeaderFooter),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::HeaderFooter);
}

#[test]
fn classify_unknown_no_row_col() {
    let regions = vec![
        make_region_for_classify(None, None, LayoutRegionRole::ParagraphBody),
        make_region_for_classify(None, None, LayoutRegionRole::ParagraphBody),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].kind, CollisionKind::Unknown);
}

#[test]
fn classify_out_of_bounds_index_yields_unknown() {
    let regions = vec![make_region_for_classify(
        Some(0),
        Some(0),
        LayoutRegionRole::Unknown,
    )];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 99)]);
    assert_eq!(result[0].kind, CollisionKind::Unknown);
    assert!(result[0].region_b.is_none());
}

#[test]
fn classify_region_roles_propagated() {
    let regions = vec![
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::LeftLabel),
        make_region_for_classify(Some(1), Some(0), LayoutRegionRole::RightValue),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].region_a, Some(LayoutRegionRole::LeftLabel));
    assert_eq!(result[0].region_b, Some(LayoutRegionRole::RightValue));
}

#[test]
fn classify_empty_collisions_returns_empty() {
    let regions = vec![make_region_for_classify(
        Some(0),
        Some(0),
        LayoutRegionRole::Unknown,
    )];
    let result = classify_collisions(&regions, &[]);
    assert!(result.is_empty());
}

// ---------------------------------------------------------------------------
// CollisionSeverity + collision_severity (v1.14.0)
// ---------------------------------------------------------------------------

#[test]
fn collision_severity_minor_below_5_percent() {
    // overlap_area = 4 pt², box = 200 pt² → ratio = 0.02 → Minor
    assert_eq!(
        crate::extract::collision_severity(4.0, 200.0, 400.0),
        crate::extract::CollisionSeverity::Minor
    );
}

#[test]
fn collision_severity_moderate_between_5_and_20_percent() {
    // overlap_area = 50 pt², box = 500 pt² → ratio = 0.10 → Moderate
    assert_eq!(
        crate::extract::collision_severity(50.0, 500.0, 1000.0),
        crate::extract::CollisionSeverity::Moderate
    );
}

#[test]
fn collision_severity_major_above_20_percent() {
    // overlap_area = 110 pt², box = 500 pt² → ratio = 0.22 → Major
    assert_eq!(
        crate::extract::collision_severity(110.0, 500.0, 1000.0),
        crate::extract::CollisionSeverity::Major
    );
}

#[test]
fn collision_severity_absolute_fallback_when_area_zero() {
    // No box area → absolute thresholds
    assert_eq!(
        crate::extract::collision_severity(10.0, 0.0, 0.0),
        crate::extract::CollisionSeverity::Minor, // < 50 pt²
    );
    assert_eq!(
        crate::extract::collision_severity(200.0, 0.0, 0.0),
        crate::extract::CollisionSeverity::Moderate, // 50–400 pt²
    );
    assert_eq!(
        crate::extract::collision_severity(500.0, 0.0, 0.0),
        crate::extract::CollisionSeverity::Major, // ≥ 400 pt²
    );
}

#[test]
fn classify_collisions_sets_severity_field() {
    // make_region_for_classify gives source_bbox [0,0,50,10] → area = 500 pt²
    // make_collision_for_classify gives overlap_area = 100 pt²
    // ratio = 100 / 500 = 0.20 → exactly at boundary → Major (ratio < 0.20 is false)
    let regions = vec![
        make_region_for_classify(Some(0), Some(0), LayoutRegionRole::LeftLabel),
        make_region_for_classify(Some(0), Some(1), LayoutRegionRole::RightValue),
    ];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 1)]);
    assert_eq!(result[0].severity, crate::extract::CollisionSeverity::Major);
}

#[test]
fn classify_collisions_out_of_bounds_uses_absolute_fallback() {
    // One region in-range (area 500 pt²), one out of bounds → box_b_area = 0
    // overlap_area = 100 → uses min(500, 0) = 0 → absolute: 100 < 400 → Moderate
    let regions = vec![make_region_for_classify(
        Some(0),
        Some(0),
        LayoutRegionRole::Unknown,
    )];
    let result = classify_collisions(&regions, &[make_collision_for_classify(0, 99)]);
    assert_eq!(
        result[0].severity,
        crate::extract::CollisionSeverity::Moderate
    );
}

// ---------------------------------------------------------------------------
// extract_label_value_pairs (v1.14.0)
// ---------------------------------------------------------------------------

fn make_region_with_role_and_row(
    row: usize,
    col: usize,
    role: LayoutRegionRole,
    text: &str,
) -> LayoutRegion {
    LayoutRegion {
        kind: LayoutRegionKind::TableCell,
        role,
        row: Some(row),
        col: Some(col),
        text: text.to_owned(),
        source_bbox: [0.0, 0.0, 50.0, 10.0],
        usable_rect: [0.0, 0.0, 50.0, 10.0],
        fragments: vec![],
    }
}

#[test]
fn extract_label_value_pairs_basic() {
    let regions = vec![
        make_region_with_role_and_row(0, 0, LayoutRegionRole::LeftLabel, "Name"),
        make_region_with_role_and_row(0, 1, LayoutRegionRole::RightValue, "Value A"),
        make_region_with_role_and_row(1, 0, LayoutRegionRole::LeftLabel, "Address"),
        make_region_with_role_and_row(1, 1, LayoutRegionRole::RightValue, "Value B"),
    ];
    let pairs = crate::extract::extract_label_value_pairs(&regions);
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].label.text, "Name");
    assert_eq!(pairs[0].values.len(), 1);
    assert_eq!(pairs[0].values[0].text, "Value A");
    assert_eq!(pairs[1].label.text, "Address");
    assert_eq!(pairs[1].values[0].text, "Value B");
}

#[test]
fn extract_label_value_pairs_multiple_values_per_label() {
    let regions = vec![
        make_region_with_role_and_row(0, 0, LayoutRegionRole::LeftLabel, "Label"),
        make_region_with_role_and_row(0, 1, LayoutRegionRole::RightValue, "Val 1"),
        make_region_with_role_and_row(0, 2, LayoutRegionRole::RightValue, "Val 2"),
    ];
    let pairs = crate::extract::extract_label_value_pairs(&regions);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].values.len(), 2);
    // Values sorted by column
    assert_eq!(pairs[0].values[0].text, "Val 1");
    assert_eq!(pairs[0].values[1].text, "Val 2");
}

#[test]
fn extract_label_value_pairs_lone_value_skipped() {
    // RightValue with no LeftLabel on the same row → not in output
    let regions = vec![make_region_with_role_and_row(
        0,
        1,
        LayoutRegionRole::RightValue,
        "Orphan",
    )];
    let pairs = crate::extract::extract_label_value_pairs(&regions);
    assert!(pairs.is_empty());
}

#[test]
fn extract_label_value_pairs_skips_regions_without_row() {
    let mut region = make_region_with_role_and_row(0, 0, LayoutRegionRole::LeftLabel, "X");
    region.row = None; // No row → skip
    let regions = vec![region];
    let pairs = crate::extract::extract_label_value_pairs(&regions);
    assert!(pairs.is_empty());
}

#[test]
fn extract_label_value_pairs_empty_values_list_ok() {
    // Label with no value siblings is still returned (values is empty Vec)
    let regions = vec![make_region_with_role_and_row(
        0,
        0,
        LayoutRegionRole::LeftLabel,
        "Solo",
    )];
    let pairs = crate::extract::extract_label_value_pairs(&regions);
    assert_eq!(pairs.len(), 1);
    assert!(pairs[0].values.is_empty());
}

// ---------------------------------------------------------------------------
// PageLayoutQuality
// ---------------------------------------------------------------------------

fn make_fit_plan_for_quality(status: crate::PlacementStatus, used_rect: [f32; 4]) -> RegionFitPlan {
    let mut region = make_region_with_role_and_row(0, 0, LayoutRegionRole::RightValue, "Source");
    region.source_bbox = [10.0, 10.0, 40.0, 10.0];
    region.usable_rect = [10.0, 10.0, 40.0, 10.0];
    RegionFitPlan {
        region,
        fit: crate::FitResult {
            lines: vec!["Translated".to_owned()],
            font_size: 8.0,
            used_rect,
            overflow_horizontal: matches!(status, crate::PlacementStatus::Overflow),
            overflow_vertical: false,
            status,
        },
        collisions: vec![],
    }
}

#[test]
fn page_layout_quality_reports_overflow_and_image_overlap() {
    let plans = vec![make_fit_plan_for_quality(
        crate::PlacementStatus::Overflow,
        [10.0, 10.0, 80.0, 10.0],
    )];
    let quality = PageLayoutQuality::from_plans(1, &plans, &[[50.0, 8.0, 20.0, 20.0]]);
    assert_eq!(quality.page_num, 1);
    assert_eq!(quality.overflow_count, 1);
    assert_eq!(quality.image_overlap_count, 1);
    assert!(
        quality
            .issues
            .iter()
            .any(|issue| issue.kind == LayoutIssueKind::TextOverflow)
    );
    assert!(
        quality
            .issues
            .iter()
            .any(|issue| issue.kind == LayoutIssueKind::ImageOverlap)
    );
    assert_eq!(quality.worst_severity, Some(LayoutIssueSeverity::Major));
}

#[test]
fn page_layout_quality_reports_accepted_shrink_as_minor() {
    let plans = vec![make_fit_plan_for_quality(
        crate::PlacementStatus::Shrunk,
        [10.0, 10.0, 35.0, 10.0],
    )];
    let quality = PageLayoutQuality::from_plans(1, &plans, &[]);
    assert_eq!(quality.summary.shrunk_count, 1);
    assert!(
        quality
            .issues
            .iter()
            .any(|issue| issue.kind == LayoutIssueKind::AcceptedShrink
                && issue.severity == LayoutIssueSeverity::Minor)
    );
}
