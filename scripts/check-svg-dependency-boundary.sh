#!/usr/bin/env bash
set -euo pipefail

graph="$(cargo tree --locked --no-default-features --features svg --edges normal)"

if grep -q 'ttf-parser' <<<"$graph"; then
    echo "ttf-parser must not be present in the published svg dependency graph" >&2
    exit 1
fi

grep -q 'skrifa ' <<<"$graph" || {
    echo "skrifa must remain the font metadata parser for the svg graph" >&2
    exit 1
}

echo "SVG dependency boundary passed: skrifa present, ttf-parser absent"
