use std::collections::BTreeMap;

/// Builds a PDF content stream fragment that renders `chars` at `(x, y)`.
///
/// - `render_mode 0` — normal visible text
/// - `render_mode 3` — invisible (selectable/searchable, no paint)
///
/// `color` is an RGB triplet in 0.0–1.0 range, applied only for `render_mode 0`.
/// `gs_name`: when `Some("GS0")`, emits `"/GS0 gs"` to apply an ExtGState (e.g. opacity).
/// `rotation_degrees`: counter-clockwise rotation in degrees. `0.0` emits `Td`; any other
/// value emits a full `Tm` text matrix (`cos sin -sin cos x y Tm`).
/// `bold`: enables render mode 2 (fill+stroke) with a thin synthetic stroke for a bold effect.
/// `italic`: adds a 12° horizontal shear via a `Tm` text matrix for a synthetic italic effect.
/// Character encoding: 2-byte big-endian GID values (Identity-H encoding).
#[allow(clippy::too_many_arguments)]
pub fn text_stream(
    font_name: &[u8],
    font_size: f32,
    x: f32,
    y: f32,
    rotation_degrees: f32,
    chars: &[char],
    char_to_gid: &BTreeMap<char, u16>,
    render_mode: u8,
    color: [f32; 3],
    gs_name: Option<&str>,
    bold: bool,
    italic: bool,
) -> Vec<u8> {
    let hex = chars_to_hex(chars, char_to_gid);
    if hex.is_empty() {
        return Vec::new();
    }

    // Bold uses render mode 2 (fill+stroke) with a thin proportional stroke.
    // Invisible text (mode 3) is unaffected by bold/italic.
    let effective_mode = if bold && render_mode == 0 { 2u8 } else { render_mode };

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
        if bold {
            // Stroke color matches fill so the bold effect is uniform.
            s.push_str(&format!(
                "{:.4} {:.4} {:.4} RG\n",
                color[0], color[1], color[2]
            ));
            // Stroke width ≈ 4% of font size (scales with font size).
            s.push_str(&format!("{:.4} w\n", font_size * 0.04));
        }
    }
    s.push_str(&format!("{} Tr\n", effective_mode));

    // Position: italic uses a shear Tm; rotation overrides Td in both cases.
    if italic {
        // Shear factor for 12° synthetic italic: tan(12°) ≈ 0.21256.
        const SHEAR: f32 = 0.21256;
        if rotation_degrees == 0.0 {
            s.push_str(&format!("1 0 {SHEAR:.5} 1 {x:.4} {y:.4} Tm\n"));
        } else {
            // Combine rotation R and italic shear S: result = [a b c d x y].
            // R = [[cos, sin], [-sin, cos]], S = [[1, 0], [shear, 1]]
            // R*S = [[cos + sin*shear, sin], [-sin + cos*shear, cos]]
            let theta = rotation_degrees.to_radians();
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            let a = cos_t + sin_t * SHEAR;
            let b = sin_t;
            let c = -sin_t + cos_t * SHEAR;
            let d = cos_t;
            s.push_str(&format!(
                "{a:.6} {b:.6} {c:.6} {d:.6} {x:.4} {y:.4} Tm\n"
            ));
        }
    } else if rotation_degrees == 0.0 {
        s.push_str(&format!("{x:.4} {y:.4} Td\n"));
    } else {
        let theta = rotation_degrees.to_radians();
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        s.push_str(&format!(
            "{cos_t:.6} {sin_t:.6} {:.6} {cos_t:.6} {x:.4} {y:.4} Tm\n",
            -sin_t
        ));
    }
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
    text_stream(font_name, font_size, x, y, 0.0, chars, char_to_gid, 3, [0.0; 3], None, false, false)
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
        let bytes = text_stream(b"F0", 12.0, 50.0, 100.0, 0.0, &['A'], &map, 0, [1.0, 0.0, 0.0], None, false, false);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("0 Tr"), "visible mode should use Tr 0");
        assert!(s.contains("1.0000 0.0000 0.0000 rg"), "should emit RGB color");
    }

    #[test]
    fn rotation_zero_uses_td() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = text_stream(b"F0", 12.0, 10.0, 20.0, 0.0, &['A'], &map, 0, [0.0; 3], None, false, false);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("10.0000 20.0000 Td"), "zero rotation should use Td");
        assert!(!s.contains("Tm"), "zero rotation must not emit Tm");
    }

    #[test]
    fn rotation_nonzero_uses_tm() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = text_stream(b"F0", 12.0, 50.0, 100.0, 45.0, &['A'], &map, 0, [0.0; 3], None, false, false);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("Tm"), "non-zero rotation should use Tm");
        assert!(!s.contains("Td"), "non-zero rotation must not emit Td");
        // cos(45°) ≈ 0.707107
        assert!(s.contains("0.707107"), "should embed cos(45)");
    }

    #[test]
    fn text_stream_with_gs_emits_gs_op() {
        let mut map = BTreeMap::new();
        map.insert('A', 1u16);
        let bytes = text_stream(b"F0", 12.0, 0.0, 0.0, 0.0, &['A'], &map, 0, [0.0; 3], Some("GS0"), false, false);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS0 gs"), "should emit gs operator when gs_name is Some");
        // Must appear after q and before BT
        let q_pos = s.find("q\n").unwrap();
        let gs_pos = s.find("/GS0 gs").unwrap();
        let bt_pos = s.find("BT").unwrap();
        assert!(q_pos < gs_pos && gs_pos < bt_pos);
    }
}
