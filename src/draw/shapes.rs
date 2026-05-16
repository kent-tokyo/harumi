/// Filled rectangle content stream fragment.
///
/// `rect` = `[x, y, width, height]` in PDF points, origin bottom-left.
/// `gs_name` = the `/ExtGState` resource name to set opacity (e.g. `"GS0"`).
pub(crate) fn rect_stream(rect: &[f32; 4], color: &[f32; 3], gs_name: &str) -> Vec<u8> {
    format!(
        "q\n/{gs} gs\n{r:.4} {g:.4} {b:.4} rg\n{x:.4} {y:.4} {w:.4} {h:.4} re\nf\nQ\n",
        gs = gs_name,
        r = color[0], g = color[1], b = color[2],
        x = rect[0], y = rect[1], w = rect[2], h = rect[3],
    )
    .into_bytes()
}

/// Stroked line content stream fragment.
///
/// `from` / `to` = endpoints in PDF points.
/// `width` = stroke width in PDF points.
pub(crate) fn line_stream(
    from: &[f32; 2],
    to: &[f32; 2],
    color: &[f32; 3],
    width: f32,
    gs_name: &str,
) -> Vec<u8> {
    format!(
        "q\n/{gs} gs\n{r:.4} {g:.4} {b:.4} RG\n{lw:.4} w\n{x1:.4} {y1:.4} m\n{x2:.4} {y2:.4} l\nS\nQ\n",
        gs = gs_name,
        r = color[0], g = color[1], b = color[2],
        lw = width,
        x1 = from[0], y1 = from[1],
        x2 = to[0], y2 = to[1],
    )
    .into_bytes()
}

/// Stroked rectangle content stream fragment (border only, no fill).
pub(crate) fn rect_stroke_stream(
    rect: &[f32; 4],
    color: &[f32; 3],
    line_width: f32,
    gs_name: &str,
) -> Vec<u8> {
    format!(
        "q\n/{gs} gs\n{r:.4} {g:.4} {b:.4} RG\n{lw:.4} w\n{x:.4} {y:.4} {w:.4} {h:.4} re\nS\nQ\n",
        gs = gs_name,
        r = color[0], g = color[1], b = color[2],
        lw = line_width,
        x = rect[0], y = rect[1], w = rect[2], h = rect[3],
    )
    .into_bytes()
}

/// Closed polygon content stream fragment.
///
/// `filled = true` → fill (`f` operator, `rg` color); `filled = false` → stroke (`S`, `RG`).
/// Returns an empty Vec if fewer than 2 points are given.
pub(crate) fn polygon_stream(
    points: &[[f32; 2]],
    color: &[f32; 3],
    gs_name: &str,
    filled: bool,
) -> Vec<u8> {
    if points.len() < 2 {
        return Vec::new();
    }
    let color_op = if filled { "rg" } else { "RG" };
    let paint_op = if filled { "f" } else { "S" };
    let mut s = format!(
        "q\n/{gs} gs\n{r:.4} {g:.4} {b:.4} {co}\n{x0:.4} {y0:.4} m\n",
        gs = gs_name,
        r = color[0], g = color[1], b = color[2],
        co = color_op,
        x0 = points[0][0], y0 = points[0][1],
    );
    for pt in &points[1..] {
        s.push_str(&format!("{:.4} {:.4} l\n", pt[0], pt[1]));
    }
    s.push_str(&format!("h\n{}\nQ\n", paint_op));
    s.into_bytes()
}

/// Returns a PDF content stream fragment that strokes an open polyline.
///
/// Unlike `polygon_stream`, the path is not closed (`h` is omitted). Returns an
/// empty Vec if fewer than 2 points are given.
pub(crate) fn polyline_stream(
    points: &[[f32; 2]],
    color: &[f32; 3],
    width: f32,
    gs_name: &str,
) -> Vec<u8> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut s = format!(
        "q\n/{gs} gs\n{r:.4} {g:.4} {b:.4} RG\n{lw:.4} w\n{x0:.4} {y0:.4} m\n",
        gs = gs_name,
        r = color[0], g = color[1], b = color[2],
        lw = width,
        x0 = points[0][0], y0 = points[0][1],
    );
    for pt in &points[1..] {
        s.push_str(&format!("{:.4} {:.4} l\n", pt[0], pt[1]));
    }
    s.push_str("S\nQ\n");
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_stream_contains_operators() {
        let bytes = rect_stream(&[10.0, 20.0, 100.0, 50.0], &[1.0, 0.0, 0.0], "GS0");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS0 gs"), "should set ExtGState");
        assert!(s.contains("re\nf"), "should fill rectangle");
        assert!(s.contains("1.0000 0.0000 0.0000 rg"), "should set fill color");
    }

    #[test]
    fn rect_stroke_stream_uses_capital_rg() {
        let bytes = rect_stroke_stream(&[10.0, 20.0, 100.0, 50.0], &[1.0, 0.0, 0.0], 2.0, "GS0");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS0 gs"));
        assert!(s.contains("re\nS"), "should stroke rectangle");
        assert!(s.contains("1.0000 0.0000 0.0000 RG"), "should use stroke color (RG)");
        assert!(s.contains("2.0000 w"), "should set line width");
        assert!(!s.contains(" rg"), "should NOT set fill color");
    }

    #[test]
    fn polygon_stream_filled_contains_rg_and_f() {
        let pts = [[0.0_f32, 0.0], [10.0, 0.0], [5.0, 10.0]];
        let bytes = polygon_stream(&pts, &[0.0, 1.0, 0.0], "GS1", true);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("0.0000 1.0000 0.0000 rg"), "fill color rg");
        assert!(s.contains(" m\n"), "moveto");
        assert!(s.contains(" l\n"), "lineto");
        assert!(s.contains("h\n"), "close path");
        assert!(s.contains("\nf\n"), "fill operator");
        assert!(!s.contains("\nS\n"), "no stroke");
    }

    #[test]
    fn polygon_stream_stroked_contains_rg_and_s() {
        let pts = [[0.0_f32, 0.0], [10.0, 0.0], [5.0, 10.0]];
        let bytes = polygon_stream(&pts, &[1.0, 0.0, 0.0], "GS2", false);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("1.0000 0.0000 0.0000 RG"), "stroke color RG");
        assert!(s.contains("\nS\n"), "stroke operator");
        assert!(!s.contains("\nf\n"), "no fill");
    }

    #[test]
    fn polygon_stream_empty_returns_empty() {
        assert!(polygon_stream(&[], &[0.0; 3], "GS0", true).is_empty());
        assert!(polygon_stream(&[[0.0, 0.0]], &[0.0; 3], "GS0", true).is_empty());
    }

    #[test]
    fn line_stream_contains_operators() {
        let bytes = line_stream(&[0.0, 0.0], &[100.0, 0.0], &[0.0, 0.0, 1.0], 2.0, "GS1");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS1 gs"), "should set ExtGState");
        assert!(s.contains("m\n"), "should have moveto");
        assert!(s.contains("l\nS"), "should stroke line");
        assert!(s.contains("2.0000 w"), "should set line width");
    }

    #[test]
    fn polyline_no_closepath() {
        let pts = [[0.0_f32, 0.0], [50.0, 0.0], [50.0, 50.0]];
        let bytes = polyline_stream(&pts, &[1.0, 0.0, 0.0], 1.5, "GS0");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("/GS0 gs"));
        assert!(s.contains("m\n"), "moveto");
        // two lineto operators
        assert_eq!(s.matches(" l\n").count(), 2);
        assert!(s.contains("\nS\n"), "stroke without close");
        assert!(!s.contains("\nh\n"), "must NOT close path");
    }

    #[test]
    fn polyline_fewer_than_2_points_is_empty() {
        assert!(polyline_stream(&[], &[0.0; 3], 1.0, "GS0").is_empty());
        assert!(polyline_stream(&[[0.0, 0.0]], &[0.0; 3], 1.0, "GS0").is_empty());
    }
}
