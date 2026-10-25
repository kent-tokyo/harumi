//! Conversion of the self-contained SVG subset emitted by chemical renderers.

use std::collections::HashMap;

use roxmltree::{Document as XmlDocument, Node};

use crate::{Document, Error, Result};

/// Options for [`svg_to_pdf_with_options`].
#[derive(Clone, Debug)]
pub struct SvgOptions<'a> {
    /// Font bytes used for every SVG `<text>` element.
    pub font_bytes: &'a [u8],
    /// Override the SVG-derived page size when present.
    pub page_size: Option<[f32; 2]>,
}

/// Convert a self-contained SVG into one vector PDF page.
///
/// The converter intentionally accepts one documented subset: groups,
/// translations/scales, lines, polylines, polygons, rectangles, circles,
/// paths made of `M/L/H/V/Z`, text, fills, strokes, opacity, dash arrays, and
/// `marker-end` arrowheads. Text uses the supplied TrueType/OpenType font.
pub fn svg_to_pdf(svg: &[u8], font_bytes: &[u8]) -> Result<Vec<u8>> {
    svg_to_pdf_with_options(
        svg,
        SvgOptions {
            font_bytes,
            page_size: None,
        },
    )
}

/// Convert SVG using an explicit font and optional page-size override.
pub fn svg_to_pdf_with_options(svg: &[u8], options: SvgOptions<'_>) -> Result<Vec<u8>> {
    let xml = std::str::from_utf8(svg)
        .map_err(|e| Error::InvalidInput(format!("SVG is not UTF-8: {e}")))?;
    let parsed = XmlDocument::parse(xml)
        .map_err(|e| Error::InvalidInput(format!("malformed SVG XML: {e}")))?;
    let root = parsed.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Error::InvalidInput("SVG root element must be <svg>".into()));
    }
    if options.font_bytes.is_empty() {
        return Err(Error::InvalidInput(
            "SVG text conversion requires non-empty font_bytes".into(),
        ));
    }

    let view_box = parse_view_box(root.attribute("viewBox"))?;
    let width = options
        .page_size
        .map(|size| size[0])
        .or_else(|| root.attribute("width").and_then(parse_length))
        .or_else(|| view_box.map(|box_| box_[2]))
        .ok_or_else(|| Error::InvalidInput("SVG needs width or viewBox".into()))?;
    let height = options
        .page_size
        .map(|size| size[1])
        .or_else(|| root.attribute("height").and_then(parse_length))
        .or_else(|| view_box.map(|box_| box_[3]))
        .ok_or_else(|| Error::InvalidInput("SVG needs height or viewBox".into()))?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(Error::InvalidInput(
            "SVG page size must be positive and finite".into(),
        ));
    }

    let mut document = Document::new((width, height))?;
    let font = document.embed_font(options.font_bytes)?;
    let mut page = document.page(1)?;
    let source = view_box.unwrap_or([0.0, 0.0, width, height]);
    let scale_x = width / source[2];
    let scale_y = height / source[3];
    let root_transform =
        Transform::translate(-source[0], -source[1]).then(Transform::scale(scale_x, scale_y));
    let inherited = Style::default();
    {
        let mut ctx = Context {
            page: &mut page,
            font,
            height,
        };
        for child in root.children().filter(Node::is_element) {
            render_node(child, root_transform, inherited, &mut ctx)?;
        }
    }
    document.save_to_bytes()
}

struct Context<'a, 'doc> {
    page: &'a mut crate::PageHandle<'doc>,
    font: crate::FontHandle,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Transform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }
    fn scale(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }
    fn then(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }
    fn point(self, x: f32, y: f32, page_height: f32) -> [f32; 2] {
        [
            self.a * x + self.c * y + self.e,
            page_height - (self.b * x + self.d * y + self.f),
        ]
    }
    fn x_scale(self) -> f32 {
        (self.a * self.a + self.b * self.b).sqrt()
    }
    fn y_scale(self) -> f32 {
        (self.c * self.c + self.d * self.d).sqrt()
    }
    fn average_scale(self) -> f32 {
        ((self.x_scale() + self.y_scale()) / 2.0).max(f32::EPSILON)
    }
    fn rotation_degrees(self) -> f32 {
        (-self.b).atan2(self.a).to_degrees()
    }
}

#[derive(Clone, Copy)]
struct Style {
    fill: Option<[f32; 3]>,
    stroke: Option<[f32; 3]>,
    stroke_width: f32,
    opacity: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
    dash: Option<[f32; 2]>,
    font_size: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Some([0.0; 3]),
            stroke: None,
            stroke_width: 1.0,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            dash: None,
            font_size: 12.0,
        }
    }
}

fn render_node(
    node: Node<'_, '_>,
    transform: Transform,
    parent: Style,
    ctx: &mut Context<'_, '_>,
) -> Result<()> {
    let style = style_for(node, parent)?;
    let transform = transform.then(parse_transform(node.attribute("transform"))?);
    match node.tag_name().name() {
        "g" | "svg" => {
            for child in node.children().filter(Node::is_element) {
                render_node(child, transform, style, ctx)?;
            }
        }
        "line" => {
            let p1 = transform.point(attr(node, "x1")?, attr(node, "y1")?, ctx.height);
            let p2 = transform.point(attr(node, "x2")?, attr(node, "y2")?, ctx.height);
            draw_line(ctx, p1, p2, style, has_marker(node))?;
        }
        "polyline" | "polygon" => {
            let points = parse_points(
                node.attribute("points")
                    .ok_or_else(|| invalid("missing points"))?,
            )?
            .into_iter()
            .map(|p| transform.point(p[0], p[1], ctx.height))
            .collect::<Vec<_>>();
            if node.tag_name().name() == "polygon" && points.len() >= 2 {
                draw_polygon(ctx, &points, style)?;
            } else {
                draw_polyline(ctx, &points, style, has_marker(node))?;
            }
        }
        "rect" => {
            let x = attr(node, "x").unwrap_or(0.0);
            let y = attr(node, "y").unwrap_or(0.0);
            let w = attr(node, "width")?;
            let h = attr(node, "height")?;
            let points = [
                transform.point(x, y, ctx.height),
                transform.point(x + w, y, ctx.height),
                transform.point(x + w, y + h, ctx.height),
                transform.point(x, y + h, ctx.height),
            ];
            if let Some(color) = style.fill {
                ctx.page.add_path(
                    &points,
                    true,
                    color,
                    true,
                    0.0,
                    style.opacity * style.fill_opacity,
                )?;
            }
            if let Some(color) = style.stroke {
                ctx.page.add_path(
                    &points,
                    true,
                    color,
                    false,
                    scaled_stroke_width(transform, style),
                    style.opacity * style.stroke_opacity,
                )?;
            }
        }
        "circle" => {
            let cx = attr(node, "cx").unwrap_or(0.0);
            let cy = attr(node, "cy").unwrap_or(0.0);
            let r = attr(node, "r")?;
            let points = (0..32)
                .map(|index| {
                    let angle = index as f32 * std::f32::consts::TAU / 32.0;
                    transform.point(cx + r * angle.cos(), cy + r * angle.sin(), ctx.height)
                })
                .collect::<Vec<_>>();
            if let Some(color) = style.fill {
                ctx.page.add_path(
                    &points,
                    true,
                    color,
                    true,
                    0.0,
                    style.opacity * style.fill_opacity,
                )?;
            }
            if let Some(color) = style.stroke {
                ctx.page.add_path(
                    &points,
                    true,
                    color,
                    false,
                    scaled_stroke_width(transform, style),
                    style.opacity * style.stroke_opacity,
                )?;
            }
        }
        "path" => {
            let path = parse_path(
                node.attribute("d")
                    .ok_or_else(|| invalid("missing path data"))?,
            )?;
            let points = path
                .points
                .into_iter()
                .map(|p| transform.point(p[0], p[1], ctx.height))
                .collect::<Vec<_>>();
            if let Some(color) = style.fill {
                ctx.page.add_path(
                    &points,
                    path.closed,
                    color,
                    true,
                    0.0,
                    style.opacity * style.fill_opacity,
                )?;
            }
            if let Some(color) = style.stroke {
                ctx.page.add_path(
                    &points,
                    path.closed,
                    color,
                    false,
                    scaled_stroke_width(transform, style),
                    style.opacity * style.stroke_opacity,
                )?;
            }
        }
        "text" => {
            let text = node
                .descendants()
                .filter(Node::is_text)
                .map(|node| node.text().unwrap_or_default())
                .collect::<String>();
            if !text.is_empty() {
                let x = attr(node, "x").unwrap_or(0.0);
                let mut y = attr(node, "y").unwrap_or(0.0);
                let font_size = style.font_size;
                let width = ctx.page.measure_text(&text, ctx.font, font_size)?;
                let x = match node.attribute("text-anchor").unwrap_or("start") {
                    "start" => x,
                    "middle" => x - width / 2.0,
                    "end" => x - width,
                    other => return Err(invalid(format!("unsupported text-anchor {other}"))),
                };
                match node.attribute("dominant-baseline").unwrap_or("alphabetic") {
                    "auto" | "alphabetic" | "baseline" => {}
                    "central" | "middle" => y += font_size * 0.35,
                    "text-before-edge" | "before-edge" | "hanging" => y += font_size * 0.9,
                    "text-after-edge" | "after-edge" => y -= font_size * 0.1,
                    "ideographic" => y += font_size * 0.2,
                    "mathematical" => y += font_size * 0.45,
                    other => return Err(invalid(format!("unsupported dominant-baseline {other}"))),
                }
                let p = transform.point(x, y, ctx.height);
                let color = style.fill.or(style.stroke).unwrap_or([0.0; 3]);
                ctx.page.add_text_with_rotation(
                    &text,
                    ctx.font,
                    p,
                    font_size * transform.y_scale(),
                    color,
                    style.opacity * style.fill_opacity,
                    transform.rotation_degrees(),
                )?;
            }
        }
        "defs" | "marker" | "title" | "desc" | "style" => {}
        other => return Err(invalid(format!("unsupported SVG element <{other}>"))),
    }
    Ok(())
}

fn draw_line(
    ctx: &mut Context<'_, '_>,
    from: [f32; 2],
    to: [f32; 2],
    style: Style,
    marker: bool,
) -> Result<()> {
    let color = style.stroke.ok_or_else(|| invalid("line has no stroke"))?;
    if let Some(dash) = style.dash {
        ctx.page.add_line_dashed(
            from,
            to,
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
            &dash,
            0.0,
        )?;
    } else {
        ctx.page.add_line(
            from,
            to,
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
        )?;
    }
    if marker {
        arrowhead(
            ctx,
            from,
            to,
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
        )?;
    }
    Ok(())
}

fn draw_polyline(
    ctx: &mut Context<'_, '_>,
    points: &[[f32; 2]],
    style: Style,
    marker: bool,
) -> Result<()> {
    let color = style
        .stroke
        .ok_or_else(|| invalid("polyline has no stroke"))?;
    if let Some(dash) = style.dash {
        ctx.page.add_polyline_dashed(
            points,
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
            &dash,
            0.0,
        )?;
    } else {
        ctx.page.add_polyline(
            points,
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
        )?;
    }
    if marker && points.len() >= 2 {
        arrowhead(
            ctx,
            points[points.len() - 2],
            points[points.len() - 1],
            color,
            style.stroke_width,
            style.opacity * style.stroke_opacity,
        )?;
    }
    Ok(())
}

fn draw_polygon(ctx: &mut Context<'_, '_>, points: &[[f32; 2]], style: Style) -> Result<()> {
    let color = style
        .fill
        .or(style.stroke)
        .ok_or_else(|| invalid("polygon has no fill or stroke"))?;
    ctx.page.add_polygon(
        points,
        color,
        style.opacity * style.fill_opacity,
        style.fill.is_some(),
        style
            .stroke
            .filter(|_| true)
            .map(|_| style.stroke_width)
            .unwrap_or(0.0),
    )
}

fn arrowhead(
    ctx: &mut Context<'_, '_>,
    from: [f32; 2],
    to: [f32; 2],
    color: [f32; 3],
    width: f32,
    opacity: f32,
) -> Result<()> {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return Ok(());
    }
    let ux = dx / len;
    let uy = dy / len;
    let size = (width * 5.0).max(4.0);
    let base = [to[0] - ux * size, to[1] - uy * size];
    let left = [base[0] - uy * size * 0.45, base[1] + ux * size * 0.45];
    let right = [base[0] + uy * size * 0.45, base[1] - ux * size * 0.45];
    ctx.page
        .add_polygon(&[to, left, right], color, opacity, true, 0.0)
}

fn has_marker(node: Node<'_, '_>) -> bool {
    node.attribute("marker-end").is_some()
}

fn scaled_stroke_width(transform: Transform, style: Style) -> f32 {
    if style.stroke.is_none() {
        return 0.0;
    }
    style.stroke_width * transform.average_scale()
}

fn style_for(node: Node<'_, '_>, mut style: Style) -> Result<Style> {
    let mut values = HashMap::new();
    if let Some(raw) = node.attribute("style") {
        for pair in raw.split(';').filter_map(|item| item.split_once(':')) {
            values.insert(pair.0.trim(), pair.1.trim());
        }
    }
    for key in [
        "fill",
        "stroke",
        "stroke-width",
        "opacity",
        "fill-opacity",
        "stroke-opacity",
        "stroke-dasharray",
        "font-size",
    ] {
        if let Some(value) = node.attribute(key).or_else(|| values.get(key).copied()) {
            match key {
                "fill" => style.fill = parse_color(value)?,
                "stroke" => style.stroke = parse_color(value)?,
                "stroke-width" => {
                    style.stroke_width =
                        parse_length(value).ok_or_else(|| invalid("invalid stroke-width"))?
                }
                "opacity" => style.opacity = parse_number(value)?,
                "fill-opacity" => style.fill_opacity = parse_number(value)?,
                "stroke-opacity" => style.stroke_opacity = parse_number(value)?,
                "font-size" => {
                    style.font_size =
                        parse_length(value).ok_or_else(|| invalid("invalid font-size"))?
                }
                "stroke-dasharray" => {
                    let nums = parse_numbers(value)?;
                    if nums.len() != 2 {
                        return Err(invalid("stroke-dasharray currently supports two values"));
                    }
                    style.dash = Some([nums[0], nums[1]]);
                }
                _ => {}
            }
        }
    }
    Ok(style)
}

fn parse_transform(raw: Option<&str>) -> Result<Transform> {
    let Some(raw) = raw else {
        return Ok(Transform::IDENTITY);
    };
    let mut result = Transform::IDENTITY;
    for item in raw
        .split(')')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let (name, args) = item
            .split_once('(')
            .ok_or_else(|| invalid("malformed transform"))?;
        let n = parse_numbers(args)?;
        let t = match name.trim() {
            "translate" if (1..=2).contains(&n.len()) => {
                Transform::translate(n[0], *n.get(1).unwrap_or(&0.0))
            }
            "scale" if (1..=2).contains(&n.len()) => {
                Transform::scale(n[0], *n.get(1).unwrap_or(&n[0]))
            }
            "matrix" if n.len() == 6 => Transform {
                a: n[0],
                b: n[1],
                c: n[2],
                d: n[3],
                e: n[4],
                f: n[5],
            },
            _ => return Err(invalid(format!("unsupported transform {name}"))),
        };
        result = result.then(t);
    }
    Ok(result)
}

fn parse_path(raw: &str) -> Result<ParsedPath> {
    let tokens = tokenize_path(raw)?;
    let mut index = 0;
    let mut command = 'M';
    let mut current = [0.0, 0.0];
    let mut subpath_start = [0.0, 0.0];
    let mut closed = false;
    let mut points = Vec::new();

    while index < tokens.len() {
        if let PathToken::Command(next) = tokens[index] {
            command = next;
            index += 1;
            if matches!(command, 'Z' | 'z') {
                current = subpath_start;
                closed = true;
                continue;
            }
        }

        match command {
            'M' | 'm' | 'L' | 'l' => {
                let x = number_token(&tokens, &mut index)?;
                let y = number_token(&tokens, &mut index)?;
                if matches!(command, 'm' | 'l') {
                    current[0] += x;
                    current[1] += y;
                } else {
                    current = [x, y];
                }
                if command == 'M' || command == 'm' {
                    subpath_start = current;
                    command = if command == 'm' { 'l' } else { 'L' };
                }
                points.push(current);
            }
            'H' | 'h' => {
                let x = number_token(&tokens, &mut index)?;
                current[0] = if command == 'h' { current[0] + x } else { x };
                points.push(current);
            }
            'V' | 'v' => {
                let y = number_token(&tokens, &mut index)?;
                current[1] = if command == 'v' { current[1] + y } else { y };
                points.push(current);
            }
            _ => return Err(invalid("path supports only M/L/H/V/Z commands")),
        }
    }
    if points.len() < 2 {
        return Err(invalid("path needs at least two points"));
    }
    Ok(ParsedPath { points, closed })
}

#[derive(Debug)]
struct ParsedPath {
    points: Vec<[f32; 2]>,
    closed: bool,
}

#[derive(Clone, Copy, Debug)]
enum PathToken {
    Command(char),
    Number(f32),
}

fn number_token(tokens: &[PathToken], index: &mut usize) -> Result<f32> {
    match tokens.get(*index) {
        Some(PathToken::Number(value)) => {
            *index += 1;
            Ok(*value)
        }
        _ => Err(invalid("path command has incomplete arguments")),
    }
}

fn tokenize_path(raw: &str) -> Result<Vec<PathToken>> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_whitespace() || chars[index] == ',' {
            index += 1;
            continue;
        }
        if chars[index].is_ascii_alphabetic() {
            let command = chars[index];
            if !matches!(
                command,
                'M' | 'm' | 'L' | 'l' | 'H' | 'h' | 'V' | 'v' | 'Z' | 'z'
            ) {
                return Err(invalid(format!("unsupported path command {command}")));
            }
            tokens.push(PathToken::Command(command));
            index += 1;
            continue;
        }

        let start = index;
        if matches!(chars[index], '+' | '-') {
            index += 1;
        }
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        if index < chars.len() && chars[index] == '.' {
            index += 1;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
        }
        if index < chars.len() && matches!(chars[index], 'e' | 'E') {
            index += 1;
            if index < chars.len() && matches!(chars[index], '+' | '-') {
                index += 1;
            }
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
        }
        if start == index {
            return Err(invalid("invalid path number"));
        }
        let value = chars[start..index]
            .iter()
            .collect::<String>()
            .parse::<f32>()
            .map_err(|_| invalid("invalid path number"))?;
        tokens.push(PathToken::Number(value));
    }
    Ok(tokens)
}

fn parse_points(raw: &str) -> Result<Vec<[f32; 2]>> {
    let n = parse_numbers(raw)?;
    if n.len() < 4 || n.len() % 2 != 0 {
        return Err(invalid("points must contain coordinate pairs"));
    }
    Ok(n.chunks(2).map(|p| [p[0], p[1]]).collect())
}
fn parse_view_box(raw: Option<&str>) -> Result<Option<[f32; 4]>> {
    raw.map(|v| {
        let n = parse_numbers(v)?;
        if n.len() != 4 || n[2] <= 0.0 || n[3] <= 0.0 {
            Err(invalid("viewBox must be x y width height"))
        } else {
            Ok([n[0], n[1], n[2], n[3]])
        }
    })
    .transpose()
}
fn parse_numbers(raw: &str) -> Result<Vec<f32>> {
    raw.replace(',', " ")
        .split_whitespace()
        .map(parse_number)
        .collect()
}
fn parse_number(raw: &str) -> Result<f32> {
    raw.trim()
        .trim_end_matches('%')
        .parse::<f32>()
        .map_err(|_| invalid(format!("invalid number: {raw}")))
}
fn parse_length(raw: &str) -> Option<f32> {
    let raw = raw.trim();
    ["px", "pt", "mm", "cm", "in"]
        .iter()
        .find_map(|unit| {
            raw.strip_suffix(unit)
                .and_then(|v| v.parse::<f32>().ok())
                .map(|v| {
                    v * match *unit {
                        "pt" => 1.0,
                        "mm" => 72.0 / 25.4,
                        "cm" => 72.0 / 2.54,
                        "in" => 72.0,
                        "px" => 0.75,
                        _ => 1.0,
                    }
                })
        })
        .or_else(|| raw.parse().ok())
}
fn parse_color(raw: &str) -> Result<Option<[f32; 3]>> {
    let value = raw.trim();
    if value == "none" {
        return Ok(None);
    }
    if let Some(hex) = value.strip_prefix('#') {
        let hex = match hex.len() {
            3 => format!(
                "{}{}{}{}{}{}",
                &hex[0..1],
                &hex[0..1],
                &hex[1..2],
                &hex[1..2],
                &hex[2..3],
                &hex[2..3]
            ),
            6 => hex.to_owned(),
            _ => return Err(invalid("color hex must be three or six digits")),
        };
        return Ok(Some([
            u8::from_str_radix(&hex[0..2], 16).map_err(|_| invalid("invalid color"))? as f32
                / 255.0,
            u8::from_str_radix(&hex[2..4], 16).map_err(|_| invalid("invalid color"))? as f32
                / 255.0,
            u8::from_str_radix(&hex[4..6], 16).map_err(|_| invalid("invalid color"))? as f32
                / 255.0,
        ]));
    }
    let named = match value {
        "black" => [0.0, 0.0, 0.0],
        "white" => [1.0, 1.0, 1.0],
        "red" => [1.0, 0.0, 0.0],
        "green" => [0.0, 0.5, 0.0],
        "blue" => [0.0, 0.0, 1.0],
        _ => return Err(invalid(format!("unsupported color: {value}"))),
    };
    Ok(Some(named))
}
fn attr(node: Node<'_, '_>, name: &str) -> Result<f32> {
    node.attribute(name)
        .and_then(parse_length)
        .ok_or_else(|| invalid(format!("missing or invalid {name}")))
}
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("SVG: {}", message.into()))
}
