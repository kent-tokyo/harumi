#!/usr/bin/env bash
set -euo pipefail

font_path="${1:-tests/fixtures/NotoSansJP-Regular.ttf}"
image_path="${2:-tests/fixtures/red_1x1.png}"
output_dir="${3:-/private/tmp/harumi-pdf-spec-coverage}"

if [[ ! -f "$font_path" || ! -f "$image_path" ]]; then
    printf 'fixture missing: font=%s image=%s\n' "$font_path" "$image_path" >&2
    exit 1
fi
if ! command -v pdftoppm >/dev/null 2>&1 || ! command -v pdfinfo >/dev/null 2>&1; then
    printf 'pdftoppm and pdfinfo are required\n' >&2
    exit 127
fi

mkdir -p "$output_dir/pdfs"
cargo run --manifest-path tools/pdf-spec-coverage-check/Cargo.toml --quiet -- \
    "$font_path" "$image_path" "$output_dir/pdfs"

for pdf in "$output_dir"/pdfs/*.pdf; do
    name="$(basename "$pdf" .pdf)"
    scripts/check-poppler-render-fixture.sh "$pdf" "$output_dir/$name" 144 1 \
        "$output_dir/$name/report.json"
done

printf 'PDF specification corpus rendered with Poppler: %s\n' "$output_dir"
