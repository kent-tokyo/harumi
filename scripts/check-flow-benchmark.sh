#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
font_path="${1:-$repo_root/tests/fixtures/NotoSansJP-Regular.ttf}"
output_path="${2:-/tmp/harumi-flow-benchmark.json}"
tool_root="$repo_root/tools/flow-benchmark-check"

[[ -f "$font_path" ]] || { printf 'font fixture not found: %s\n' "$font_path" >&2; exit 1; }
cargo build --manifest-path "$tool_root/Cargo.toml" --quiet
target_dir="${CARGO_TARGET_DIR:-$tool_root/target}"
if [[ "$target_dir" != /* ]]; then target_dir="$repo_root/$target_dir"; fi
runner_path="$target_dir/debug/harumi-flow-benchmark-check"
[[ -x "$runner_path" ]] || { printf 'benchmark runner not found: %s\n' "$runner_path" >&2; exit 1; }

time_mode="none"
if /usr/bin/time -f '%M' -o /dev/null true 2>/dev/null; then time_mode="gnu"; fi
if [[ "$time_mode" == "none" ]] && /usr/bin/time -l -o /dev/null true 2>/dev/null; then time_mode="darwin"; fi

work_dir="$(mktemp -d /tmp/harumi-flow-benchmark.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT
for pages in 100 1000; do
    metrics_path="$work_dir/$pages.json"
    time_path="$work_dir/$pages.time"
    if [[ "$time_mode" == "gnu" ]]; then
        /usr/bin/time -f '%M' -o "$time_path" "$runner_path" "$font_path" "$pages" "$metrics_path"
        rss_kb="$(awk '{print $1}' "$time_path")"
        if [[ "$rss_kb" =~ ^[0-9]+$ ]]; then
            python3 -c 'import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()); d["peak_rss_bytes"]=int(sys.argv[2])*1024; p.write_text(json.dumps(d,indent=2)+"\n")' "$metrics_path" "$rss_kb"
        fi
    elif [[ "$time_mode" == "darwin" ]]; then
        /usr/bin/time -l -o "$time_path" "$runner_path" "$font_path" "$pages" "$metrics_path"
        rss_bytes="$(awk '/maximum resident set size/ { print $1; exit }' "$time_path")"
        if [[ "$rss_bytes" =~ ^[0-9]+$ ]]; then
            python3 -c 'import json,pathlib,sys; p=pathlib.Path(sys.argv[1]); d=json.loads(p.read_text()); d["peak_rss_bytes"]=int(sys.argv[2]); p.write_text(json.dumps(d,indent=2)+"\n")' "$metrics_path" "$rss_bytes"
        fi
    else
        "$runner_path" "$font_path" "$pages" "$metrics_path"
    fi
done

python3 -c 'import json,pathlib,sys; out={"benchmark":"flow-pagination","version":1,"warmup_policy":"3 pages before each measured run","results":[json.loads(pathlib.Path(p).read_text()) for p in sys.argv[1:3]]}; pathlib.Path(sys.argv[3]).write_text(json.dumps(out,indent=2)+"\n")' "$work_dir/100.json" "$work_dir/1000.json" "$output_path"
printf 'flow benchmark passed: %s\n' "$output_path"
