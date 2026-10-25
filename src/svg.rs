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
/// paths made of `M/L/H/V/C/S/Q/T/A/Z`, ellipses, text, fills, strokes,
/// opacity, dash arrays, and `marker-end` arrowheads. Curves and arcs are
/// flattened to vector line segments; text uses the supplied
/// TrueType/OpenType font.
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
    let markers = collect_markers(root)?;
    {
        let mut ctx = Context {
            page: &mut page,
            font,
            height,
            markers,
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
    markers: HashMap<String, Marker>,
}

#[derive(Clone, Debug)]
struct Marker {
    ref_x: f32,
    ref_y: f32,
    shape: MarkerShape,
}

#[derive(Clone, Debug)]
enum MarkerShape {
    Polygon {
        points: Vec<[f32; 2]>,
        style: Style,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: Style,
    },
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

#[derive(Clone, Copy, Debug)]
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
            draw_line(ctx, p1, p2, style, node.attribute("marker-end"))?;
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
                draw_polyline(ctx, &points, style, node.attribute("marker-end"))?;
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
        "ellipse" => {
            let cx = attr(node, "cx").unwrap_or(0.0);
            let cy = attr(node, "cy").unwrap_or(0.0);
            let rx = attr(node, "rx")?;
            let ry = attr(node, "ry")?;
            if rx <= 0.0 || ry <= 0.0 {
                return Err(invalid("ellipse radii must be positive"));
            }
            let points = (0..32)
                .map(|index| {
                    let angle = index as f32 * std::f32::consts::TAU / 32.0;
                    transform.point(cx + rx * angle.cos(), cy + ry * angle.sin(), ctx.height)
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
            for subpath in path.subpaths {
                let points = subpath
                    .points
                    .into_iter()
                    .map(|p| transform.point(p[0], p[1], ctx.height))
                    .collect::<Vec<_>>();
                if let Some(color) = style.fill {
                    ctx.page.add_path(
                        &points,
                        subpath.closed,
                        color,
                        true,
                        0.0,
                        style.opacity * style.fill_opacity,
                    )?;
                }
                if let Some(color) = style.stroke {
                    ctx.page.add_path(
                        &points,
                        subpath.closed,
                        color,
                        false,
                        scaled_stroke_width(transform, style),
                        style.opacity * style.stroke_opacity,
                    )?;
                }
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
    marker: Option<&str>,
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
    if let Some(marker) = marker {
        draw_marker(
            ctx,
            marker,
            from,
            to,
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
    marker: Option<&str>,
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
    if let Some(marker) = marker
        && points.len() >= 2
    {
        draw_marker(
            ctx,
            marker,
            points[points.len() - 2],
            points[points.len() - 1],
            style.stroke_width,
            style.opacity * style.stroke_opacity,
        )?;
    }
    Ok(())
}

fn draw_polygon(ctx: &mut Context<'_, '_>, points: &[[f32; 2]], style: Style) -> Result<()> {
    if let Some(color) = style.fill {
        ctx.page
            .add_polygon(points, color, style.opacity * style.fill_opacity, true, 0.0)?;
    }
    if let Some(color) = style.stroke {
        ctx.page.add_polygon(
            points,
            color,
            style.opacity * style.stroke_opacity,
            false,
            style.stroke_width,
        )?;
    }
    if style.fill.is_none() && style.stroke.is_none() {
        return Err(invalid("polygon has no fill or stroke"));
    }
    Ok(())
}

fn draw_marker(
    ctx: &mut Context<'_, '_>,
    reference: &str,
    from: [f32; 2],
    to: [f32; 2],
    stroke_width: f32,
    opacity: f32,
) -> Result<()> {
    if reference == "none" {
        return Ok(());
    }
    let Some(id) = reference
        .strip_prefix("url(#")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Err(invalid("marker-end must be none or url(#id)"));
    };
    let Some(marker) = ctx.markers.get(id) else {
        return Err(invalid(format!("marker definition not found: {id}")));
    };
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return Ok(());
    }
    let ux = dx / len;
    let uy = dy / len;
    let scale = stroke_width.max(1.0);
    let origin = [
        to[0] - ux * marker.ref_x * scale + uy * marker.ref_y * scale,
        to[1] - uy * marker.ref_x * scale - ux * marker.ref_y * scale,
    ];
    let map = |x: f32, y: f32| {
        [
            origin[0] + (ux * x - uy * y) * scale,
            origin[1] + (uy * x + ux * y) * scale,
        ]
    };
    let (points, style) = match &marker.shape {
        MarkerShape::Polygon { points, style } => (
            points.iter().map(|p| map(p[0], p[1])).collect::<Vec<_>>(),
            *style,
        ),
        MarkerShape::Rect {
            x,
            y,
            width,
            height,
            style,
        } => (
            vec![
                map(*x, *y),
                map(*x + *width, *y),
                map(*x + *width, *y + *height),
                map(*x, *y + *height),
            ],
            *style,
        ),
    };
    let color = style
        .fill
        .or(style.stroke)
        .ok_or_else(|| invalid("marker has no fill or stroke"))?;
    ctx.page.add_path(
        &points,
        true,
        color,
        style.fill.is_some(),
        style
            .stroke
            .filter(|_| true)
            .map(|_| style.stroke_width * scale)
            .unwrap_or(0.0),
        opacity
            * style.opacity
            * if style.fill.is_some() {
                style.fill_opacity
            } else {
                style.stroke_opacity
            },
    )
}

fn collect_markers(root: Node<'_, '_>) -> Result<HashMap<String, Marker>> {
    let mut markers = HashMap::new();
    for node in root
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "marker")
    {
        let Some(id) = node.attribute("id") else {
            continue;
        };
        let marker = Marker {
            ref_x: parse_length(node.attribute("refX").unwrap_or("0")).unwrap_or(0.0),
            ref_y: parse_length(node.attribute("refY").unwrap_or("0")).unwrap_or(0.0),
            shape: node
                .children()
                .find(Node::is_element)
                .map(|shape| -> Result<MarkerShape> {
                    let style = style_for(shape, Style::default())?;
                    match shape.tag_name().name() {
                        "polygon" => Ok(MarkerShape::Polygon {
                            points: parse_points(
                                shape
                                    .attribute("points")
                                    .ok_or_else(|| invalid("marker polygon missing points"))?,
                            )?,
                            style,
                        }),
                        "rect" => Ok(MarkerShape::Rect {
                            x: attr(shape, "x").unwrap_or(0.0),
                            y: attr(shape, "y").unwrap_or(0.0),
                            width: attr(shape, "width")?,
                            height: attr(shape, "height")?,
                            style,
                        }),
                        other => Err(invalid(format!("unsupported marker element <{other}>"))),
                    }
                })
                .transpose()?
                .ok_or_else(|| invalid("marker has no shape"))?,
        };
        markers.insert(id.to_owned(), marker);
    }
    Ok(markers)
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
    let mut current_subpath: Option<ParsedSubpath> = None;
    let mut subpaths = Vec::new();
    let mut previous_command = 'M';
    let mut last_cubic_control = None;
    let mut last_quadratic_control = None;

    while index < tokens.len() {
        if let PathToken::Command(next) = tokens[index] {
            command = next;
            index += 1;
            if matches!(command, 'Z' | 'z') {
                current = subpath_start;
                if let Some(subpath) = current_subpath.as_mut() {
                    subpath.closed = true;
                }
                last_cubic_control = None;
                last_quadratic_control = None;
                previous_command = command;
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
                    if let Some(subpath) = current_subpath.take() {
                        subpaths.push(subpath);
                    }
                    subpath_start = current;
                    current_subpath = Some(ParsedSubpath {
                        points: vec![current],
                        closed: false,
                    });
                    command = if command == 'm' { 'l' } else { 'L' };
                } else if let Some(subpath) = current_subpath.as_mut() {
                    subpath.points.push(current);
                } else {
                    return Err(invalid("path must start with a move-to command"));
                }
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'H' | 'h' => {
                let x = number_token(&tokens, &mut index)?;
                current[0] = if command == 'h' { current[0] + x } else { x };
                current_subpath
                    .as_mut()
                    .ok_or_else(|| invalid("path must start with a move-to command"))?
                    .points
                    .push(current);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'V' | 'v' => {
                let y = number_token(&tokens, &mut index)?;
                current[1] = if command == 'v' { current[1] + y } else { y };
                current_subpath
                    .as_mut()
                    .ok_or_else(|| invalid("path must start with a move-to command"))?
                    .points
                    .push(current);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'C' | 'c' => {
                let mut values = [0.0; 6];
                for value in &mut values {
                    *value = number_token(&tokens, &mut index)?;
                }
                let relative = command == 'c';
                let control1 = relative_point(current, [values[0], values[1]], relative);
                let control2 = relative_point(current, [values[2], values[3]], relative);
                let endpoint = relative_point(current, [values[4], values[5]], relative);
                append_curve(
                    current_subpath
                        .as_mut()
                        .ok_or_else(|| invalid("path must start with a move-to command"))?,
                    cubic_points(current, control1, control2, endpoint),
                );
                current = endpoint;
                last_cubic_control = Some(control2);
                last_quadratic_control = None;
            }
            'S' | 's' => {
                let mut values = [0.0; 4];
                for value in &mut values {
                    *value = number_token(&tokens, &mut index)?;
                }
                let relative = command == 's';
                let control1 = if matches!(previous_command, 'C' | 'c' | 'S' | 's') {
                    reflect_control(current, last_cubic_control.unwrap_or(current))
                } else {
                    current
                };
                let control2 = relative_point(current, [values[0], values[1]], relative);
                let endpoint = relative_point(current, [values[2], values[3]], relative);
                append_curve(
                    current_subpath
                        .as_mut()
                        .ok_or_else(|| invalid("path must start with a move-to command"))?,
                    cubic_points(current, control1, control2, endpoint),
                );
                current = endpoint;
                last_cubic_control = Some(control2);
                last_quadratic_control = None;
            }
            'Q' | 'q' => {
                let mut values = [0.0; 4];
                for value in &mut values {
                    *value = number_token(&tokens, &mut index)?;
                }
                let relative = command == 'q';
                let control = relative_point(current, [values[0], values[1]], relative);
                let endpoint = relative_point(current, [values[2], values[3]], relative);
                append_curve(
                    current_subpath
                        .as_mut()
                        .ok_or_else(|| invalid("path must start with a move-to command"))?,
                    quadratic_points(current, control, endpoint),
                );
                current = endpoint;
                last_quadratic_control = Some(control);
                last_cubic_control = None;
            }
            'T' | 't' => {
                let values = [
                    number_token(&tokens, &mut index)?,
                    number_token(&tokens, &mut index)?,
                ];
                let relative = command == 't';
                let control = if matches!(previous_command, 'Q' | 'q' | 'T' | 't') {
                    reflect_control(current, last_quadratic_control.unwrap_or(current))
                } else {
                    current
                };
                let endpoint = relative_point(current, values, relative);
                append_curve(
                    current_subpath
                        .as_mut()
                        .ok_or_else(|| invalid("path must start with a move-to command"))?,
                    quadratic_points(current, control, endpoint),
                );
                current = endpoint;
                last_quadratic_control = Some(control);
                last_cubic_control = None;
            }
            'A' | 'a' => {
                let rx = number_token(&tokens, &mut index)?.abs();
                let ry = number_token(&tokens, &mut index)?.abs();
                let rotation = number_token(&tokens, &mut index)?;
                let large_arc = number_token(&tokens, &mut index)? != 0.0;
                let sweep = number_token(&tokens, &mut index)? != 0.0;
                let endpoint = relative_point(
                    current,
                    [
                        number_token(&tokens, &mut index)?,
                        number_token(&tokens, &mut index)?,
                    ],
                    command == 'a',
                );
                if rx <= f32::EPSILON || ry <= f32::EPSILON {
                    append_line(
                        current_subpath
                            .as_mut()
                            .ok_or_else(|| invalid("path must start with a move-to command"))?,
                        endpoint,
                    );
                } else {
                    append_curve(
                        current_subpath
                            .as_mut()
                            .ok_or_else(|| invalid("path must start with a move-to command"))?,
                        arc_points(current, endpoint, rx, ry, rotation, large_arc, sweep),
                    );
                }
                current = endpoint;
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            _ => return Err(invalid("unsupported path command")),
        }
        previous_command = command;
    }
    if let Some(subpath) = current_subpath {
        subpaths.push(subpath);
    }
    if subpaths.iter().all(|subpath| subpath.points.len() < 2) {
        return Err(invalid("path needs at least two points"));
    }
    Ok(ParsedPath { subpaths })
}

#[derive(Debug)]
struct ParsedPath {
    subpaths: Vec<ParsedSubpath>,
}

#[derive(Debug)]
struct ParsedSubpath {
    points: Vec<[f32; 2]>,
    closed: bool,
}

const CURVE_SEGMENTS: usize = 12;

fn relative_point(current: [f32; 2], point: [f32; 2], relative: bool) -> [f32; 2] {
    if relative {
        [current[0] + point[0], current[1] + point[1]]
    } else {
        point
    }
}

fn reflect_control(current: [f32; 2], control: [f32; 2]) -> [f32; 2] {
    [2.0 * current[0] - control[0], 2.0 * current[1] - control[1]]
}

fn append_line(subpath: &mut ParsedSubpath, point: [f32; 2]) {
    subpath.points.push(point);
}

fn append_curve(subpath: &mut ParsedSubpath, points: Vec<[f32; 2]>) {
    subpath.points.extend(points);
}

fn quadratic_points(start: [f32; 2], control: [f32; 2], end: [f32; 2]) -> Vec<[f32; 2]> {
    (1..=CURVE_SEGMENTS)
        .map(|step| {
            let t = step as f32 / CURVE_SEGMENTS as f32;
            let mt = 1.0 - t;
            [
                mt * mt * start[0] + 2.0 * mt * t * control[0] + t * t * end[0],
                mt * mt * start[1] + 2.0 * mt * t * control[1] + t * t * end[1],
            ]
        })
        .collect()
}

fn cubic_points(
    start: [f32; 2],
    control1: [f32; 2],
    control2: [f32; 2],
    end: [f32; 2],
) -> Vec<[f32; 2]> {
    (1..=CURVE_SEGMENTS)
        .map(|step| {
            let t = step as f32 / CURVE_SEGMENTS as f32;
            let mt = 1.0 - t;
            [
                mt.powi(3) * start[0]
                    + 3.0 * mt.powi(2) * t * control1[0]
                    + 3.0 * mt * t.powi(2) * control2[0]
                    + t.powi(3) * end[0],
                mt.powi(3) * start[1]
                    + 3.0 * mt.powi(2) * t * control1[1]
                    + 3.0 * mt * t.powi(2) * control2[1]
                    + t.powi(3) * end[1],
            ]
        })
        .collect()
}

fn arc_points(
    start: [f32; 2],
    end: [f32; 2],
    mut rx: f32,
    mut ry: f32,
    rotation_degrees: f32,
    large_arc: bool,
    sweep: bool,
) -> Vec<[f32; 2]> {
    if (start[0] - end[0]).abs() <= f32::EPSILON && (start[1] - end[1]).abs() <= f32::EPSILON {
        return Vec::new();
    }
    let phi = rotation_degrees.to_radians();
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let dx = (start[0] - end[0]) / 2.0;
    let dy = (start[1] - end[1]) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let radii_scale = (x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry)).sqrt();
    if radii_scale > 1.0 {
        rx *= radii_scale;
        ry *= radii_scale;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let numerator = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let denominator = rx2 * y1p2 + ry2 * x1p2;
    let mut coefficient = if denominator <= f32::EPSILON {
        0.0
    } else {
        (numerator / denominator).sqrt()
    };
    if large_arc == sweep {
        coefficient = -coefficient;
    }
    let cxp = coefficient * rx * y1p / ry;
    let cyp = coefficient * -ry * x1p / rx;
    let center = [
        cos_phi * cxp - sin_phi * cyp + (start[0] + end[0]) / 2.0,
        sin_phi * cxp + cos_phi * cyp + (start[1] + end[1]) / 2.0,
    ];
    let unit = |x: f32, y: f32| [x / rx, y / ry];
    let from = unit(
        cos_phi * (start[0] - center[0]) + sin_phi * (start[1] - center[1]),
        -sin_phi * (start[0] - center[0]) + cos_phi * (start[1] - center[1]),
    );
    let to = unit(
        cos_phi * (end[0] - center[0]) + sin_phi * (end[1] - center[1]),
        -sin_phi * (end[0] - center[0]) + cos_phi * (end[1] - center[1]),
    );
    let angle =
        |a: [f32; 2], b: [f32; 2]| (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1]);
    let mut delta = angle(from, to);
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    let segments = ((delta.abs() / (std::f32::consts::PI / 12.0)).ceil() as usize).max(1);
    (1..=segments)
        .map(|step| {
            let theta = angle([1.0, 0.0], from) + delta * step as f32 / segments as f32;
            [
                center[0] + rx * theta.cos() * cos_phi - ry * theta.sin() * sin_phi,
                center[1] + rx * theta.cos() * sin_phi + ry * theta.sin() * cos_phi,
            ]
        })
        .collect()
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
                'M' | 'm'
                    | 'L'
                    | 'l'
                    | 'H'
                    | 'h'
                    | 'V'
                    | 'v'
                    | 'C'
                    | 'c'
                    | 'S'
                    | 's'
                    | 'Q'
                    | 'q'
                    | 'T'
                    | 't'
                    | 'A'
                    | 'a'
                    | 'Z'
                    | 'z'
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
