#!/usr/bin/env bash
set -euo pipefail

input="${1:-examples/fixtures/scanned_sample.pdf}"
output_dir="${2:-/tmp/harumi-poppler-render-fixture}"
dpi="${3:-144}"
expected_pages="${4:-1}"
report_path="${5:-$output_dir/report.json}"

if [[ ! -f "$input" ]]; then
    printf 'fixture not found: %s\n' "$input" >&2
    exit 1
fi
if ! [[ "$dpi" =~ ^[1-9][0-9]*$ ]]; then
    printf 'dpi must be a positive integer: %s\n' "$dpi" >&2
    exit 2
fi
if ! [[ "$expected_pages" =~ ^[1-9][0-9]*$ ]]; then
    printf 'expected-pages must be a positive integer: %s\n' "$expected_pages" >&2
    exit 2
fi
if ! command -v pdftoppm >/dev/null 2>&1; then
    printf 'pdftoppm is required for the Poppler render contract\n' >&2
    exit 127
fi
if ! command -v pdfinfo >/dev/null 2>&1; then
    printf 'pdfinfo is required for the Poppler render contract\n' >&2
    exit 127
fi
if ! command -v shasum >/dev/null 2>&1; then
    printf 'shasum is required for the renderer artifact report\n' >&2
    exit 127
fi

pdf_info="$(pdfinfo "$input")"
page_count="$(printf '%s\n' "$pdf_info" | awk '/^Pages:/ { print $2; exit }')"
if [[ "$page_count" != "$expected_pages" ]]; then
    printf 'page count mismatch: expected %s, got %s\n' "$expected_pages" "$page_count" >&2
    exit 1
fi

mkdir -p "$output_dir"
prefix="$output_dir/page"
pdftoppm -png -r "$dpi" -f 1 -l "$expected_pages" "$input" "$prefix" >/dev/null

actual_count="$(find "$output_dir" -maxdepth 1 -type f -name 'page-*.png' | wc -l | tr -d ' ')"
if [[ "$actual_count" != "$expected_pages" ]]; then
    printf 'rendered page count mismatch: expected %s, got %s\n' "$expected_pages" "$actual_count" >&2
    exit 1
fi

for image in "$output_dir"/page-*.png; do
    [[ -s "$image" ]] || { printf 'empty render: %s\n' "$image" >&2; exit 1; }
    file "$image" | grep -q 'PNG image data' || {
        printf 'renderer output is not PNG data: %s\n' "$image" >&2
        exit 1
    }
done

page_size="$(printf '%s\n' "$pdf_info" | awk -F': ' '/^Page size:/ { print $2; exit }')"
pages_json=""
for ((page = 1; page <= expected_pages; page++)); do
    image="$output_dir/page-$page.png"
    image_info="$(file "$image")"
    raster_size="$(printf '%s\n' "$image_info" | sed -E 's/.*PNG image data, ([0-9]+ x [0-9]+).*/\1/')"
    if [[ "$raster_size" == "$image_info" ]]; then
        printf 'could not read PNG dimensions: %s\n' "$image_info" >&2
        exit 1
    fi
    [[ -n "$pages_json" ]] && pages_json+=$',\n'
    pages_json+=$'    {\n      "page": '
    pages_json+="$page"
    pages_json+=$',\n      "raster_size_pixels": "'
    pages_json+="$raster_size"
    pages_json+=$'",\n      "sha256": "'
    pages_json+="$(shasum -a 256 "$image" | awk '{ print $1 }')"
    pages_json+=$'"\n    }'
done
mkdir -p "$(dirname "$report_path")"
cat > "$report_path" <<EOF
{
  "renderer": "poppler",
  "command": "pdftoppm",
  "input": "$(basename "$input")",
  "dpi": $dpi,
  "page_count": $page_count,
  "page_size_points": "${page_size}",
  "pages": [
${pages_json}
  ]
}
EOF

printf 'Poppler render contract passed: %s pages at %s dpi (%s)\n' \
    "$expected_pages" "$dpi" "$output_dir"
printf 'renderer artifact report: %s\n' "$report_path"
