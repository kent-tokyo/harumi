use std::collections::BTreeMap;

/// Generates the ToUnicode CMap stream content for a Type0 (CID) font.
///
/// `gid_to_char` maps each glyph ID to its Unicode character.
/// GIDs are encoded as 2-byte Identity-H codes in the Tj operator,
/// so bfchar entries are <GGGG> → <UUUU> (hex GID → UTF-16BE codepoint).
pub fn generate_to_unicode(gid_to_char: &BTreeMap<u16, char>) -> Vec<u8> {
    let mut out = String::new();

    out.push_str("/CIDInit /ProcSet findresource begin\n");
    out.push_str("12 dict begin\n");
    out.push_str("begincmap\n");
    out.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    out.push_str("/CMapName /Adobe-Identity-UCS def\n");
    out.push_str("/CMapType 2 def\n");

    // PDF spec: max 100 entries per beginbfchar block.
    let entries: Vec<(u16, char)> = gid_to_char.iter().map(|(&g, &c)| (g, c)).collect();
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, ch) in chunk {
            let utf16 = encode_utf16be(*ch);
            out.push_str(&format!("<{:04X}> <{}>\n", gid, utf16));
        }
        out.push_str("endbfchar\n");
    }

    out.push_str("endcmap\n");
    out.push_str("CMapName currentdict /CMap defineresource pop\n");
    out.push_str("end\nend\n");

    out.into_bytes()
}

/// Encodes a char as a hex string of its UTF-16BE bytes.
fn encode_utf16be(ch: char) -> String {
    let mut buf = [0u16; 2];
    let encoded = ch.encode_utf16(&mut buf);
    encoded
        .iter()
        .map(|unit| format!("{:04X}", unit))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_cjk_char() {
        // U+65E5 '日' → UTF-16BE = 65E5, no surrogate
        let s = encode_utf16be('日');
        assert_eq!(s, "65E5");
    }

    #[test]
    fn supplementary_plane_char() {
        // U+20000 (CJK Extension B) → UTF-16BE surrogate pair D840 DC00
        let s = encode_utf16be('\u{20000}');
        assert_eq!(s, "D840DC00");
    }

    #[test]
    fn to_unicode_structure() {
        let mut map = BTreeMap::new();
        map.insert(1u16, '日');
        map.insert(2u16, '本');
        map.insert(3u16, '語');
        let bytes = generate_to_unicode(&map);
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("begincmap"));
        assert!(text.contains("<0001> <65E5>"));
        assert!(text.contains("<0002> <672C>"));
        assert!(text.contains("<0003> <8A9E>"));
    }
}
