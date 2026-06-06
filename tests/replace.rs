use harumi::Document;
use lopdf;

const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

// ---------------------------------------------------------------------------
// Cross-operator replace test helpers
// ---------------------------------------------------------------------------

/// Modify the first content stream on page 1 of a PDF, splitting the first
/// `<HEX> Tj` into two consecutive Tj operators at `split_at_char` CID chars.
/// Returns the modified PDF bytes. Used to simulate text split across multiple
/// Tj operators in the same BT/ET block (the cross-operator match scenario).
fn split_first_tj(pdf_bytes: &[u8], split_at_char: usize) -> Vec<u8> {
    let mut ldoc = lopdf::Document::load_from(pdf_bytes).unwrap();
    let page_id = *ldoc.get_pages().values().next().unwrap();

    let contents_val = {
        let obj = ldoc.get_object(page_id).unwrap();
        obj.as_dict().unwrap().get(b"Contents").unwrap().clone()
    };
    let stream_ids: Vec<lopdf::ObjectId> = match contents_val {
        lopdf::Object::Reference(id) => vec![id],
        lopdf::Object::Array(arr) => arr
            .into_iter()
            .filter_map(|o| if let lopdf::Object::Reference(id) = o { Some(id) } else { None })
            .collect(),
        _ => panic!("unexpected Contents type"),
    };

    for stream_id in stream_ids {
        let stream_obj = ldoc.get_object(stream_id).unwrap().clone();
        let Ok(stream) = stream_obj.as_stream() else { continue };
        let mut owned = stream.clone();
        if owned.dict.get(b"Filter").is_ok() {
            owned.decompress().ok();
        }
        let Ok(content_str) = std::str::from_utf8(&owned.content) else { continue };
        if let Some(new_content) = try_split_hex_tj(content_str, split_at_char) {
            ldoc.objects.insert(
                stream_id,
                lopdf::Object::Stream(lopdf::Stream::new(
                    lopdf::Dictionary::new(),
                    new_content.into_bytes(),
                )),
            );
            break;
        }
    }

    let mut out = Vec::new();
    ldoc.save_to(&mut out).unwrap();
    out
}

/// Find the first `<HEX> Tj` in `content` and split it at `split_at_char`
/// CID characters (each CID char = 4 hex digits). Returns `None` if no
/// suitable Tj is found or the split point is out of range.
fn try_split_hex_tj(content: &str, split_at_char: usize) -> Option<String> {
    let tj_idx = content.find("> Tj")?;
    let lt_idx = content[..tj_idx].rfind('<')?;
    let hex_str = &content[lt_idx + 1..tj_idx];
    if hex_str.len() % 4 != 0 || hex_str.is_empty() {
        return None;
    }
    let split_pos = split_at_char * 4;
    if split_pos == 0 || split_pos >= hex_str.len() {
        return None;
    }
    Some(format!(
        "{}<{}> Tj\n<{}> Tj{}",
        &content[..lt_idx],
        &hex_str[..split_pos],
        &hex_str[split_pos..],
        &content[tj_idx + 4..], // skip "> Tj"
    ))
}

/// Helper: create a minimal PDF with a known text run, save to bytes.
fn pdf_with_text(text: &str) -> Vec<u8> {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text(text, font, [72.0, 700.0], 14.0).unwrap();
    doc.save_to_bytes().unwrap()
}

#[test]
fn replace_text_latin_present_in_output() {
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("Hello", "World", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    // Replacement text must appear
    assert!(
        texts.iter().any(|&t| t.contains("World")),
        "expected 'World' in extracted text, got: {:?}",
        texts
    );
    // Original text must be gone
    assert!(
        !texts.iter().any(|&t| t.contains("Hello")),
        "expected 'Hello' to be gone, got: {:?}",
        texts
    );
}

#[test]
fn replace_text_no_match_is_noop() {
    let bytes = pdf_with_text("Alpha");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("Beta", "Gamma", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    assert!(
        texts.iter().any(|&t| t.contains("Alpha")),
        "expected 'Alpha' to remain, got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|&t| t.contains("Gamma")),
        "expected 'Gamma' to be absent, got: {:?}",
        texts
    );
}

#[test]
fn replace_text_cjk() {
    let bytes = pdf_with_text("日本語");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().replace_text("日本語", "英語", font).unwrap();
    let out = doc.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();

    assert!(
        texts.iter().any(|&t| t.contains("英語")),
        "expected '英語' in extracted text, got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|&t| t.contains("日本語")),
        "expected '日本語' to be gone, got: {:?}",
        texts
    );
}

#[test]
fn replace_preserve_font_success() {
    // Two runs ensure both glyph sets land in the same font subset.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("Alpha", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("Beta", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    doc2.page(1).unwrap().replace_text_preserve_font("Alpha", "Beta").unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
    assert!(all.contains("Beta"), "expected 'Beta' in: {:?}", all);
    assert!(!all.contains("Alpha"), "expected 'Alpha' gone from: {:?}", all);
}

#[test]
fn replace_preserve_font_missing_glyph_returns_err() {
    // Only "Hello" glyphs (H,e,l,o) are in the subset; "World" needs W,r,d which are absent.
    // Error is now returned eagerly at replace_text_preserve_font() call time, not at save().
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let result = doc.page(1).unwrap().replace_text_preserve_font("Hello", "World");
    assert!(result.is_err(), "expected Err for char not in font subset, got Ok");
}

#[test]
fn replace_preserve_font_cjk() {
    // Include both strings so all glyphs are in the subset.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("日本語", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("英語", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    doc2.page(1).unwrap().replace_text_preserve_font("日本語", "英語").unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
    assert!(all.contains("英語"), "expected '英語' in: {:?}", all);
    assert!(!all.contains("日本語"), "expected '日本語' gone from: {:?}", all);
}

#[test]
fn replace_text_returns_match_count() {
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    let count = doc.page(1).unwrap().replace_text("Hello", "World", font).unwrap();
    assert_eq!(count, 1, "expected 1 match, got {count}");
}

#[test]
fn replace_text_no_match_returns_zero() {
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    let count = doc.page(1).unwrap().replace_text("NoSuchText", "World", font).unwrap();
    assert_eq!(count, 0, "expected 0 matches, got {count}");
}

#[test]
fn replace_preserve_font_returns_count() {
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("Alpha", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("Beta", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    let count = doc2.page(1).unwrap().replace_text_preserve_font("Alpha", "Beta").unwrap();
    assert_eq!(count, 1, "expected 1 match, got {count}");
    // no match → 0
    let count2 = doc2.page(1).unwrap().replace_text_preserve_font("NoSuchText", "Beta").unwrap();
    assert_eq!(count2, 0, "expected 0 matches, got {count2}");
}

#[test]
fn can_replace_text_counts_without_mutating() {
    let bytes = pdf_with_text("Hello");

    // page() requires &mut self even for read-only ops (PageHandle design)
    let mut doc = Document::from_bytes(&bytes).unwrap();
    let count = doc.page(1).unwrap().can_replace_text("Hello", "Hello").unwrap();
    assert_eq!(count, 1, "expected 1 match, got {count}");

    // Document was not mutated: original text is still there after save
    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let texts: Vec<&str> = frags.iter().map(|f| f.text.as_str()).collect();
    assert!(
        texts.iter().any(|&t| t.contains("Hello")),
        "can_replace_text should not modify the document; 'Hello' should still be present"
    );
}

#[test]
fn can_replace_text_missing_glyph_returns_err() {
    let bytes = pdf_with_text("Hello");

    let mut doc = Document::from_bytes(&bytes).unwrap();
    let result = doc.page(1).unwrap().can_replace_text("Hello", "World");
    assert!(result.is_err(), "expected Err for char not in font subset");
}

#[test]
fn replace_multiple_on_same_page() {
    // Two distinct runs on the same page, both get replaced
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap().add_invisible_text("Foo", font, [72.0, 700.0], 14.0).unwrap();
    doc.page(1).unwrap().add_invisible_text("Bar", font, [72.0, 680.0], 14.0).unwrap();
    let bytes = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&bytes).unwrap();
    let font2 = doc2.embed_font(FONT).unwrap();
    doc2.page(1).unwrap().replace_text("Foo", "Baz", font2).unwrap();
    doc2.page(1).unwrap().replace_text("Bar", "Qux", font2).unwrap();
    let out = doc2.save_to_bytes().unwrap();

    let check = Document::from_bytes(&out).unwrap();
    let frags = check.extract_text_runs(1).unwrap();
    let all_text: String = frags.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");

    assert!(all_text.contains("Baz"), "expected Baz in: {:?}", all_text);
    assert!(all_text.contains("Qux"), "expected Qux in: {:?}", all_text);
    assert!(!all_text.contains("Foo"), "expected Foo gone: {:?}", all_text);
    assert!(!all_text.contains("Bar"), "expected Bar gone: {:?}", all_text);
}

// ---------------------------------------------------------------------------
// Cross-operator replace tests
// ---------------------------------------------------------------------------

#[test]
fn replace_text_cross_operator() {
    // "Hello" is split into two consecutive Tj ops in the same BT/ET block:
    //   <Hel_gids> Tj   <lo_gids> Tj
    // replace_text("Hello", "World") must span the operator boundary.
    let split_bytes = split_first_tj(&pdf_with_text("Hello"), 3);

    let mut doc = Document::from_bytes(&split_bytes).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    let count = doc.page(1).unwrap().replace_text("Hello", "World", font).unwrap();
    assert_eq!(count, 1, "cross-op replace must find exactly 1 match");

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("World"), "expected 'World' in: {:?}", all);
    assert!(!all.contains("Hello"), "expected 'Hello' gone from: {:?}", all);
}

#[test]
fn can_replace_text_cross_operator_count() {
    // can_replace_text must count matches that span multiple Tj operators.
    let split_bytes = split_first_tj(&pdf_with_text("Hello"), 3);

    let mut doc = Document::from_bytes(&split_bytes).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .can_replace_text("Hello", "Hello")
        .unwrap();
    assert_eq!(count, 1, "can_replace_text must count cross-op matches");
}

#[test]
fn replace_preserve_font_cross_operator() {
    // Both "Hello" and "World" must be in the font subset so that
    // replace_text_preserve_font can encode "World" with the existing font.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("World", font, [72.0, 680.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    // Split "Hello"'s Tj — both BT/ET blocks share one lopdf stream object.
    let split_bytes = split_first_tj(&initial, 3);

    let mut doc2 = Document::from_bytes(&split_bytes).unwrap();
    let count = doc2
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "World")
        .unwrap();
    assert_eq!(count, 1, "preserve_font cross-op must find exactly 1 match");

    let out = doc2.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("World"), "expected 'World' in: {:?}", all);
    assert!(!all.contains("Hello"), "expected 'Hello' gone from: {:?}", all);
}

// ---------------------------------------------------------------------------
// replace_text_resubset tests
// ---------------------------------------------------------------------------

#[test]
fn replace_text_resubset_no_match() {
    // old_text not in PDF → returns 0, document unchanged.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&initial).unwrap();
    let count = doc2
        .page(1)
        .unwrap()
        .replace_text_resubset("NotPresent", "Whatever", FONT)
        .unwrap();
    assert_eq!(count, 0);

    let out = doc2.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("Hello"), "text should be unchanged: {:?}", all);
}

#[test]
fn replace_text_resubset_same_chars() {
    // Replacement text already in the subset — basic round-trip.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    // Embed "World" too so it is in the subset.
    doc.page(1)
        .unwrap()
        .add_invisible_text("World", font, [72.0, 680.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    let mut doc2 = Document::from_bytes(&initial).unwrap();
    let count = doc2
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "World", FONT)
        .unwrap();
    assert!(count >= 1, "expected at least 1 match, got {count}");

    let out = doc2.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("World"), "expected 'World' in output: {:?}", all);
}

#[test]
fn replace_text_resubset_new_char_expanded() {
    // new_text contains a character NOT in the original subset.
    // Without resubset this would fail with FontCharNotMapped;
    // with resubset it must succeed and produce readable output.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    // Only embed "Hello" so the subset has no other Latin chars beyond these.
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    // Verify that preserve_font would fail.
    let mut doc_pf = Document::from_bytes(&initial).unwrap();
    let err = doc_pf
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "日本語")
        .unwrap_err();
    assert!(
        matches!(err, harumi::Error::FontCharNotMapped { .. }),
        "expected FontCharNotMapped but got: {:?}", err
    );

    // Now use resubset — it must succeed.
    let mut doc3 = Document::from_bytes(&initial).unwrap();
    let count = doc3
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "日本語", FONT)
        .unwrap();
    assert_eq!(count, 1, "expected 1 match");

    let out = doc3.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        all.contains("日本語"),
        "expected '日本語' in output: {:?}", all
    );
}

#[test]
fn replace_text_resubset_chinese() {
    // '中', '文', '字', '你', '好' are present in NotoSansJP (shared CJK Unified Ideographs).
    // '汉', '语' are NOT (simplified Chinese only) — we avoid them here.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1).unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    // Verify preserve_font fails (Chinese chars not in the "Hello"-only subset).
    let mut doc_pf = Document::from_bytes(&initial).unwrap();
    let err = doc_pf.page(1).unwrap()
        .replace_text_preserve_font("Hello", "中文字")
        .unwrap_err();
    assert!(matches!(err, harumi::Error::FontCharNotMapped { .. }));

    // resubset with NotoSansJP (which contains these CJK ideographs) must succeed.
    let mut doc2 = Document::from_bytes(&initial).unwrap();
    let count = doc2.page(1).unwrap()
        .replace_text_resubset("Hello", "中文字", FONT)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc2.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check.extract_text_runs(1).unwrap()
        .iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("");
    assert!(all.contains("中文字"), "expected '中文字' in output: {:?}", all);
}
