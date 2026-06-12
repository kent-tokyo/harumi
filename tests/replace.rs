use harumi::Document;

const FONT: &[u8] = include_bytes!("fixtures/NotoSansJP-Regular.ttf");

// ---------------------------------------------------------------------------
// Cross-operator replace test helpers
// ---------------------------------------------------------------------------

/// Modify the first content stream on page 1 of a PDF, splitting the first
/// `<HEX> Tj` into two consecutive Tj operators at `split_at_char` CID chars.
/// Returns the modified PDF bytes. Used to simulate text split across multiple
/// Tj operators in the same BT/ET block (the cross-operator match scenario).
#[allow(dead_code)]
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
            .filter_map(|o| {
                if let lopdf::Object::Reference(id) = o {
                    Some(id)
                } else {
                    None
                }
            })
            .collect(),
        _ => panic!("unexpected Contents type"),
    };

    for stream_id in stream_ids {
        let stream_obj = ldoc.get_object(stream_id).unwrap().clone();
        let Ok(stream) = stream_obj.as_stream() else {
            continue;
        };
        let mut owned = stream.clone();
        if owned.dict.get(b"Filter").is_ok() {
            owned.decompress().ok();
        }
        let Ok(content_str) = std::str::from_utf8(&owned.content) else {
            continue;
        };
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
#[allow(dead_code)]
fn try_split_hex_tj(content: &str, split_at_char: usize) -> Option<String> {
    let tj_idx = content.find("> Tj")?;
    let lt_idx = content[..tj_idx].rfind('<')?;
    let hex_str = &content[lt_idx + 1..tj_idx];
    if !hex_str.len().is_multiple_of(4) || hex_str.is_empty() {
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
    doc.page(1)
        .unwrap()
        .add_invisible_text(text, font, [72.0, 700.0], 14.0)
        .unwrap();
    doc.save_to_bytes().unwrap()
}

// ============================================================================
// Test Suite
// ============================================================================

#[test]
fn replace_text_resubset_basic() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "世界", FONT)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("世界"), "expected '世界' in output: {}", all);
}

#[test]
fn replace_text_resubset_no_match() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset("Goodbye", "世界", FONT)
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn replace_text_resubset_empty_replacement() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "", FONT)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !all.contains("Hello"),
        "expected 'Hello' to be removed: {}",
        all
    );
}

#[test]
fn replace_text_preserve_basic() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    // Use "Hello" → "Helo" (remove one 'l'), using only characters in the original text
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "Helo")
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("Helo"), "expected 'Helo' in output: {}", all);
}

#[test]
fn replace_text_preserve_no_match() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Goodbye", "Hi")
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn replace_text_preserve_empty_replacement() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "")
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !all.contains("Hello"),
        "expected 'Hello' to be removed: {}",
        all
    );
}

#[test]
fn replace_text_preserve_char_not_in_font() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let err = doc
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "Привет")
        .unwrap_err();
    assert!(matches!(err, harumi::Error::FontCharNotMapped { .. }));
}

#[test]
fn replace_text_resubset_japanese() {
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "日本語テスト", FONT)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
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
        "expected '日本語' in output: {:?}",
        all
    );
}

#[test]
fn replace_text_resubset_chinese() {
    // '中', '文', '字', '你', '好' are present in NotoSansJP (shared CJK Unified Ideographs).
    // '汉', '语' are NOT (simplified Chinese only) — we avoid them here.
    let mut doc = Document::new((595.0, 842.0)).unwrap();
    let font = doc.embed_font(FONT).unwrap();
    doc.page(1)
        .unwrap()
        .add_invisible_text("Hello", font, [72.0, 700.0], 14.0)
        .unwrap();
    let initial = doc.save_to_bytes().unwrap();

    // Verify preserve_font fails (Chinese chars not in the "Hello"-only subset).
    let mut doc_pf = Document::from_bytes(&initial).unwrap();
    let err = doc_pf
        .page(1)
        .unwrap()
        .replace_text_preserve_font("Hello", "中文字")
        .unwrap_err();
    assert!(matches!(err, harumi::Error::FontCharNotMapped { .. }));

    // resubset with NotoSansJP (which contains these CJK ideographs) must succeed.
    let mut doc2 = Document::from_bytes(&initial).unwrap();
    let count = doc2
        .page(1)
        .unwrap()
        .replace_text_resubset("Hello", "中文字", FONT)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc2.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        all.contains("中文字"),
        "expected '中文字' in output: {:?}",
        all
    );
}

// ============================================================================
// Wrap Mode Tests (P3)
// ============================================================================

#[test]
fn replace_text_resubset_with_wrap_simple() {
    // Test simple text wrapping: short text → longer text that wraps to 2 lines
    let initial = pdf_with_text("Hi");
    let mut doc = Document::from_bytes(&initial).unwrap();

    // Replace "Hi" with a longer string that will wrap given page constraints
    // Page width ~595pt, margins ~144pt = ~451pt available for text
    let replacement = "This is a much longer replacement text";
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("Hi", replacement, FONT, 14.4)
        .unwrap();
    assert_eq!(count, 1, "Expected 1 match for 'Hi'");

    // Save and verify output
    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    // The replacement text should be in the output (possibly across multiple fragments)
    assert!(
        all.contains("This") && all.contains("longer"),
        "expected wrapped text components in output: {}",
        all
    );
}

#[test]
fn replace_text_resubset_with_wrap_cjk() {
    // Test wrapping with CJK characters
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();

    // Replace with Japanese that will wrap
    let replacement = "日本語テスト文字列は複数行に折り返されるはずです";
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("Hello", replacement, FONT, 14.4)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    assert!(
        all.contains("日本語") && all.contains("文字"),
        "expected Japanese text components in output: {}",
        all
    );
}

#[test]
fn replace_text_resubset_with_wrap_custom_line_height() {
    // Test wrapping with custom line height
    let initial = pdf_with_text("X");
    let mut doc = Document::from_bytes(&initial).unwrap();

    // Use custom line height of 20.0 (larger spacing)
    let replacement = "A B C D E F G H I J K L M N O P";
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("X", replacement, FONT, 20.0)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    // Verify multiple components of the replacement are present
    assert!(
        all.contains("A") && all.contains("P"),
        "expected wrapped text in output: {}",
        all
    );
}

#[test]
fn replace_text_resubset_with_wrap_invalid_line_height_nan() {
    // Test that NaN line_height is rejected
    let initial = pdf_with_text("Hi");
    let mut doc = Document::from_bytes(&initial).unwrap();

    let result =
        doc.page(1)
            .unwrap()
            .replace_text_resubset_with_wrap("Hi", "Replacement", FONT, f32::NAN);
    assert!(
        result.is_err(),
        "Expected error for NaN line_height, got {:?}",
        result
    );
}

#[test]
fn replace_text_resubset_with_wrap_invalid_line_height_negative() {
    // Test that negative line_height is rejected
    let initial = pdf_with_text("Hi");
    let mut doc = Document::from_bytes(&initial).unwrap();

    let result =
        doc.page(1)
            .unwrap()
            .replace_text_resubset_with_wrap("Hi", "Replacement", FONT, -5.0);
    assert!(
        result.is_err(),
        "Expected error for negative line_height, got {:?}",
        result
    );
}

#[test]
fn replace_text_resubset_with_wrap_zero_line_height_defaults_to_14_4() {
    // Test that line_height of 0 defaults to 14.4 (12pt × 1.2)
    let initial = pdf_with_text("Hi");
    let mut doc = Document::from_bytes(&initial).unwrap();

    let replacement = "A longer replacement text";
    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("Hi", replacement, FONT, 0.0)
        .unwrap();
    assert_eq!(count, 1, "Expected wrap with default line_height=14.4");

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        all.contains("longer"),
        "expected replacement text in output: {}",
        all
    );
}

#[test]
fn replace_text_resubset_with_wrap_no_match_returns_zero() {
    // Test that wrapping with no matching text returns 0
    let initial = pdf_with_text("Hello");
    let mut doc = Document::from_bytes(&initial).unwrap();

    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("NotPresent", "Replacement", FONT, 14.4)
        .unwrap();
    assert_eq!(count, 0, "Expected no matches");
}

#[test]
fn replace_text_resubset_with_wrap_single_line_fits() {
    // Test wrap mode when replacement text fits on single line (should still work)
    let initial = pdf_with_text("Hi");
    let mut doc = Document::from_bytes(&initial).unwrap();

    let count = doc
        .page(1)
        .unwrap()
        .replace_text_resubset_with_wrap("Hi", "OK", FONT, 14.4)
        .unwrap();
    assert_eq!(count, 1);

    let out = doc.save_to_bytes().unwrap();
    let check = Document::from_bytes(&out).unwrap();
    let all: String = check
        .extract_text_runs(1)
        .unwrap()
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(all.contains("OK"), "expected 'OK' in output: {}", all);
}
