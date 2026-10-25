//! Generate the fixed SVG visual-coverage fixture used by CI.
//!
//! Usage: cargo run --example svg_visual_fixture --features svg -- <output.pdf>

#[cfg(feature = "svg")]
use harumi::svg_to_pdf;

#[cfg(feature = "svg")]
const FONT: &[u8] = include_bytes!("../tests/fixtures/NotoSansJP-Regular.ttf");

#[cfg(feature = "svg")]
const SVG: &str = r##"<svg width="240" height="160" viewBox="0 0 240 160">
  <defs>
    <marker id="arrow" refX="5" refY="2" markerWidth="5" markerHeight="4">
      <polygon points="0 0, 5 2, 0 4" fill="#111111"/>
    </marker>
  </defs>
  <g transform="translate(10 10)" fill="none" stroke="#111111" stroke-width="2">
    <path d="M 10 20 L 70 20 M 10 45 L 70 45" marker-end="url(#arrow)"/>
    <path d="M 100 10 H 150 V 55 H 100 Z" stroke="#008844"/>
  </g>
  <g transform="matrix(0 1 -1 0 205 15)">
    <text x="10" y="20" font-size="16" text-anchor="middle" dominant-baseline="central" fill="#3050f8">化学</text>
  </g>
</svg>"##;

#[cfg(feature = "svg")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: svg_visual_fixture <output.pdf>")?;
    std::fs::write(output, svg_to_pdf(SVG.as_bytes(), FONT)?)?;
    Ok(())
}

#[cfg(not(feature = "svg"))]
fn main() {
    eprintln!("svg_visual_fixture requires the `svg` feature");
}
