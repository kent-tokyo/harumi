#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
font_path="${1:-$repo_root/tests/fixtures/NotoSansJP-Regular.ttf}"
output_dir="${2:-/tmp/harumi-bulk-extraction-fixture}"
report_path="$output_dir/harumi.json"

[[ -f "$font_path" ]] || {
    printf 'font fixture not found: %s\n' "$font_path" >&2
    exit 1
}
mkdir -p "$output_dir"

cargo run --manifest-path "$repo_root/tools/bulk-extraction-check/Cargo.toml" --quiet -- \
    "$font_path" "$report_path"

python3 - "$report_path" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], encoding="utf-8"))
assert report["corpus"] == "bulk-extraction-corpus-v1"
assert report["version"] == 2
assert len(report["inputs"]) == 5
for item in report["inputs"]:
    assert item["text_marker_recall"] == 1.0, item
    assert item["coordinate_coverage"] == 1.0, item
    assert item["page_batch_count"] >= 1, item
    assert item["resumable_page_failures"] == 0, item
PY

printf 'bulk extraction fixture passed: %s\n' "$report_path"
