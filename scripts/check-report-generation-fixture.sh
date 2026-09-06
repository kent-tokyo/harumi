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
time_mode="none"
if /usr/bin/time -f '%M' -o /dev/null true 2>/dev/null; then
    time_mode="gnu"
elif /usr/bin/time -l -o /dev/null true 2>/dev/null; then
    time_mode="darwin"
fi

# Build before starting the resource probe so peak RSS excludes cargo and
# compiler processes. The measured command below is the standalone runner.
cargo build --manifest-path "$repo_root/tools/report-generation-check/Cargo.toml" --quiet
target_dir="${CARGO_TARGET_DIR:-$repo_root/tools/report-generation-check/target}"
if [[ "$target_dir" != /* ]]; then
    target_dir="$repo_root/$target_dir"
fi
runner_path="$target_dir/debug/harumi-report-generation-check"
if [[ ! -x "$runner_path" ]]; then
    printf 'report-generation runner not found after build: %s\n' "$runner_path" >&2
    exit 1
fi

for backend in harumi-flow harumi-html printpdf genpdf; do
    pdf_path="$output_dir/$backend.pdf"
    backend_dir="$output_dir/$backend-poppler"
    report_path="$backend_dir/report.json"
    metrics_path="$backend_dir/backend-metrics.json"
    time_path="$backend_dir/process-time.txt"
    mkdir -p "$backend_dir"
    if [[ "$time_mode" == "gnu" ]]; then
        /usr/bin/time -f '%e %M' -o "$time_path" \
            env HARUMI_METRICS_PATH="$metrics_path" \
            "$runner_path" \
            "$backend" "$font_path" "$pdf_path"
    elif [[ "$time_mode" == "darwin" ]]; then
        /usr/bin/time -l -o "$time_path" \
            env HARUMI_METRICS_PATH="$metrics_path" \
            "$runner_path" \
            "$backend" "$font_path" "$pdf_path"
    else
        HARUMI_METRICS_PATH="$metrics_path" "$runner_path" \
            "$backend" "$font_path" "$pdf_path"
    fi
    peak_rss_bytes=""
    if [[ "$time_mode" == "gnu" ]]; then
        peak_rss_kb="$(awk '{print $2}' "$time_path")"
        [[ "$peak_rss_kb" =~ ^[0-9]+$ ]] && peak_rss_bytes=$((peak_rss_kb * 1024))
    elif [[ "$time_mode" == "darwin" ]]; then
        peak_rss_bytes="$(awk '/maximum resident set size/ { print $1; exit }' "$time_path")"
        [[ "$peak_rss_bytes" =~ ^[0-9]+$ ]] || peak_rss_bytes=""
    fi
    if [[ -n "$peak_rss_bytes" ]]; then
        python3 - "$metrics_path" "$peak_rss_bytes" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data["peak_rss_bytes"] = int(sys.argv[2])
path.write_text(json.dumps(data, indent=2) + "\n")
PY
    fi
    bash "$repo_root/scripts/check-poppler-render-fixture.sh" \
        "$pdf_path" "$backend_dir" "$dpi" "$expected_pages" "$report_path" "$metrics_path"
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
