#!/usr/bin/env bash
set -euo pipefail

input="${1:-examples/fixtures/scanned_sample.pdf}"
output="${2:-/tmp/harumi-pdfium-render-fixture.png}"
page_index="${3:-0}"
target_width="${4:-1600}"

if [[ ! -f "$input" ]]; then
    printf 'fixture not found: %s\n' "$input" >&2
    exit 1
fi

cargo run \
    --manifest-path tools/pdfium-render-check/Cargo.toml \
    --quiet -- "$input" "$output" "$page_index" "$target_width"

if [[ ! -s "$output" ]]; then
    printf 'renderer produced no output: %s\n' "$output" >&2
    exit 1
fi

if ! file "$output" | grep -q 'PNG image data'; then
    printf 'renderer output is not PNG data: %s\n' "$output" >&2
    exit 1
fi

printf 'Pdfium fixture passed: %s\n' "$output"
