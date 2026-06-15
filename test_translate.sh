#!/bin/bash
# Translate a few test PDFs and report results.
# Usage: ANTHROPIC_API_KEY=sk-ant-xxx bash test_translate.sh

set -euo pipefail

FONT="test_documents/kanto_chemical/NotoSansJP-Regular.ttf"
OUT_DIR="test_documents/kanto_chemical/translated"
mkdir -p "$OUT_DIR"

PDFS=(
  "test_documents/kanto_chemical/J_10005.pdf"
  "test_documents/kanto_chemical/J_10006.pdf"
  "test_documents/kanto_chemical/J_10007.pdf"
)

for PDF in "${PDFS[@]}"; do
  BASE=$(basename "$PDF" .pdf)

  echo "=== $BASE: overlay (ja→en) ==="
  TRANSLATE_FONT="$FONT" \
    cargo run -p harumi-ai --example translate_pdf --features anthropic --quiet -- \
    "$PDF" "$OUT_DIR/${BASE}_en_overlay.pdf" en ja overlay

  echo "=== $BASE: inplace (ja→en) ==="
  TRANSLATE_FONT="$FONT" \
    cargo run -p harumi-ai --example translate_pdf --features anthropic --quiet -- \
    "$PDF" "$OUT_DIR/${BASE}_en_inplace.pdf" en ja inplace

  echo ""
done

echo "Done. Output files:"
ls -lh "$OUT_DIR/"
