#!/usr/bin/env bash
set -euo pipefail

input="${1:-examples/fixtures/scanned_sample.pdf}"
output="${2:-/tmp/harumi-pdfium-render-fixture.png}"
page_index="${3:-0}"
target_width="${4:-1600}"
report_path="${5:-${output%.png}.json}"

if [[ ! -f "$input" ]]; then
    printf 'fixture not found: %s\n' "$input" >&2
    exit 1
fi

if [[ -z "${PDFIUM_LIBRARY_PATH:-}" ]]; then
    printf 'Pdfium runtime is not pinned. Set PDFIUM_LIBRARY_PATH to a fixed library before running this fixture.\n' >&2
    exit 127
fi
if [[ ! -f "$PDFIUM_LIBRARY_PATH" ]]; then
    printf 'PDFIUM_LIBRARY_PATH is not a file: %s\n' "$PDFIUM_LIBRARY_PATH" >&2
    exit 127
fi

renderer_output="$(cargo run \
    --manifest-path tools/pdfium-render-check/Cargo.toml \
    --quiet -- "$input" "$output" "$page_index" "$target_width")"
printf '%s\n' "$renderer_output"

if [[ ! -s "$output" ]]; then
    printf 'renderer produced no output: %s\n' "$output" >&2
    exit 1
fi

if ! file "$output" | grep -q 'PNG image data'; then
    printf 'renderer output is not PNG data: %s\n' "$output" >&2
    exit 1
fi

if ! command -v shasum >/dev/null 2>&1; then
    printf 'shasum is required for the renderer artifact report\n' >&2
    exit 127
fi

page_count="$(printf '%s\n' "$renderer_output" | awk -F= '/^page_count=/ { print $2; exit }')"
page_size="$(printf '%s\n' "$renderer_output" | awk -F= '/^page_size_points=/ { print $2; exit }')"
raster_size="$(printf '%s\n' "$renderer_output" | awk -F= '/^raster_size_pixels=/ { print $2; exit }')"
if [[ -z "$page_count" || -z "$page_size" || -z "$raster_size" ]]; then
    printf 'Pdfium runner did not emit the artifact dimensions\n' >&2
    exit 1
fi

if [[ "$page_count" -lt $((page_index + 1)) ]]; then
    printf 'page count is smaller than requested page index: %s\n' "$page_count" >&2
    exit 1
fi

mkdir -p "$(dirname "$report_path")"
cat > "$report_path" <<EOF
{
  "renderer": "pdfium",
  "command": "harumi-pdfium-render-check",
  "input": "$(basename "$input")",
  "page_index": $page_index,
  "target_width": $target_width,
  "page_count": $page_count,
  "page_size_points": "$page_size",
  "raster_size_pixels": "$raster_size",
  "sha256": "$(shasum -a 256 "$output" | awk '{ print $1 }')"
}
EOF

printf 'Pdfium fixture passed: %s\n' "$output"
printf 'renderer artifact report: %s\n' "$report_path"
