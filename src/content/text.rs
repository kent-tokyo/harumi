use std::collections::BTreeMap;

/// Builds a PDF content stream fragment that renders `chars` at `(x, y)`.
///
/// - `render_mode 0` — normal visible text
/// - `render_mode 3` — invisible (selectable/searchable, no paint)
///
/// `color` is an RGB triplet in 0.0–1.0 range, applied only for `render_mode 0`.
/// `gs_name`: when `Some("GS0")`, emits `"/GS0 gs"` to apply an ExtGState (e.g. opacity).
/// Character encoding: 2-byte big-endian GID values (Identity-H encoding).
#[allow(clippy::too_many_arguments)]
pub fn text_stream(
    font_name: &[u8],
    font_size: f32,
    x: f32,
    y: f32,
    chars: &[char],
    char_to_gid: &BTreeMap<char, u16>,
    render_mode: u8,
    color: [f32; 3],
    gs_name: Option<&str>,
) -> Vec<u8> {
    let hex = chars_to_hex(chars, char_to_gid);
    if hex.is_empty() {
        return Vec::new();
    }

    let mut s = String::new();
    s.push_str("q\n");
    if let Some(gs) = gs_name {
        s.push_str(&format!("/{gs} gs\n"));
    }
    s.push_str("BT\n");
    s.push_str(&format!(
        "/{} {} Tf\n",
        String::from_utf8_lossy(font_name),
        font_size
    ));
    if render_mode == 0 {
        s.push_str(&format!(
            "{:.4} {:.4} {:.4} rg\n",
            color[0], color[1], color[2]
        ));
    }
    s.push_str(&format!("{} Tr\n", render_mode));
    s.push_str(&format!("{:.4} {:.4} Td\n", x, y));
    s.push_str(&format!("<{}> Tj\n", hex));
    s.push_str("ET\n");
    s.push_str("Q\n");

    s.into_bytes()
}

/// Convenience wrapper: invisible text (render mode 3).
#[allow(dead_code)]
pub fn invisible_text_stream(
    font_name: &[u8],
    font_size: f32,
    x: f32,
    y: f32,
    chars: &[char],
    char_to_gid: &BTreeMap<char, u16>,
) -> Vec<u8> {
    text_stream(font_name, font_size, x, y, chars, char_to_gid, 3, [0.0; 3], None)
}

/// Converts chars to a hex string of 2-byte GID values for Identity-H encoding.
fn chars_to_hex(chars: &[char], char_to_gid: &BTreeMap<char, u16>) -> String {
    chars
        .iter()
        .filter_map(|ch| char_to_gid.get(ch).map(|gid| format!("{:04X}", gid)))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encoding() {
        let mut map = BTreeMap::new();
        map.insert('日', 1u16);
        map.insert('本', 2u16);
        map.insert('語', 3u16);
        let hex = chars_to_hex(&['日', '本', '語'], &map);
        assert_eq!(hex, "000100020003");
    }

    #[test]
    fn stream_contains_invisible_mode() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = invisible_text_stream(b"F0", 12.0, 100.0, 200.0, &['A'], &map);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("3 Tr"));
        assert!(s.contains("BT"));
        assert!(s.contains("ET"));
        assert!(!s.contains("rg"), "invisible mode should not emit color");
    }

    #[test]
    fn stream_visible_mode_has_color() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = text_stream(b"F0", 12.0, 50.0, 100.0, &['A'], &map, 0, [1.0, 0.0, 0.0], None);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("0 Tr"), "visible mode should use Tr 0");
        assert!(s.contains("1.0000 0.0000 0.0000 rg"), "should emit RGB color");
    }

    #[test]
    fn text_stream_with_gs_emits_gs_op() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = text_stream(b"F0", 12.0, 0.0, 0.0, &['A'], &map, 0, [0.0; 3], Some("GS0"));
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS0 gs"), "should emit gs operator when gs_name is Some");
        // Must appear after q and before BT
        let q_pos = s.find("q\n").unwrap();
        let gs_pos = s.find("/GS0 gs").unwrap();
        let bt_pos = s.find("BT").unwrap();
        assert!(q_pos < gs_pos && gs_pos < bt_pos);
    }
}
