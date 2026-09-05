#!/usr/bin/env bash
set -euo pipefail

run_test() {
    local target="$1"
    local name="$2"
    printf 'spec coverage: %s::%s\n' "$target" "$name"
    cargo test --quiet --all-features --test "$target" "$name" -- --exact
}

run_test page_ops nested_pages_remove_page_preserves_mediabox
run_test page_ops nested_pages_insert_blank_preserves_mediabox
run_test page_ops nested_pages_reorder_preserves_mediabox
run_test smoke smoke_contents_array
run_test smoke smoke_indirect_resources
run_test replace replace_text_in_form_xobject_inherited_resources
run_test extract_text extracts_type0_font_with_indirect_descendant_fonts
run_test extract_text roundtrip_cjk_text
run_test extract_text simple_font_encoding_fallback_no_tounicode
run_test extract_image extract_jpeg_roundtrip
run_test extract_image extract_png_roundtrip
run_test draw_smoke smoke_add_jpeg_image
run_test draw_smoke smoke_add_png_image

printf 'PDF specification unit/save-reload corpus passed.\n'
printf 'External renderer stage remains required; use the Phase 48 renderer runners for that stage.\n'
