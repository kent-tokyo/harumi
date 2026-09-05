#!/usr/bin/env bash
set -euo pipefail

# This is a dependency-boundary check, not a proof that arbitrary build scripts
# cannot invoke native tooling. Keep the forbidden package list explicit and
# review it when the workspace adds a new optional feature.
target="${CARGO_BUILD_TARGET:-wasm32-unknown-unknown}"
tree="$(cargo tree --workspace --target "$target" --all-features -e normal,build)"

for package in cc cmake pkg-config openssl-sys pdfium pdfium-render bindgen; do
    if printf '%s\n' "$tree" | grep -Eq "(^|[├└│ ]+)${package} v"; then
        printf 'forbidden native dependency on %s for %s\n' "$package" "$target" >&2
        exit 1
    fi
done

printf 'WASM dependency boundary passed for %s\n' "$target"
