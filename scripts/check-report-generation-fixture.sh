#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
font_path="${1:-$repo_root/tests/fixtures/NotoSansJP-Regular.ttf}"
output_dir="${2:-/tmp/harumi-report-generation-fixture}"
dpi="${3:-144}"
expected_pages="${4:-2}"

if [[ ! -f "$font_path" ]]; then
    printf 'font fixture not found: %s\n' "$font_path" >&2
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

mkdir -p "$output_dir"
for backend in harumi-flow harumi-html printpdf genpdf; do
    pdf_path="$output_dir/$backend.pdf"
    backend_dir="$output_dir/$backend-poppler"
    report_path="$backend_dir/report.json"
    cargo run --manifest-path "$repo_root/tools/report-generation-check/Cargo.toml" --quiet -- \
        "$backend" "$font_path" "$pdf_path"
    bash "$repo_root/scripts/check-poppler-render-fixture.sh" \
        "$pdf_path" "$backend_dir" "$dpi" "$expected_pages" "$report_path"
done

comparison_args=(
    --output "$output_dir/renderer-comparison.json"
    --reference harumi-flow
)
for backend in harumi-flow harumi-html printpdf genpdf; do
    comparison_args+=(
        --renderer
        "$backend=$output_dir/$backend-poppler/report.json"
    )
done
python3 "$repo_root/scripts/compare-render-artifacts.py" "${comparison_args[@]}"

printf 'report-generation fixture passed for %s backends: %s\n' \
    4 "$output_dir"
