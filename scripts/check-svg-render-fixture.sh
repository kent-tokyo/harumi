#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:-/tmp/harumi-svg-render-fixture}"
dpi="${2:-144}"
pdf_path="$output_dir/svg-visual-fixture.pdf"
report_path="$output_dir/poppler/report.json"

mkdir -p "$output_dir"
cargo run --quiet \
    --manifest-path "$repo_root/Cargo.toml" \
    --features svg \
    --example svg_visual_fixture \
    -- "$pdf_path"

bash "$repo_root/scripts/check-poppler-render-fixture.sh" \
    "$pdf_path" \
    "$output_dir/poppler" \
    "$dpi" \
    1 \
    "$report_path"

printf 'SVG visual fixture passed: %s\n' "$output_dir"
