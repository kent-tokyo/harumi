#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${1:-$repo_root/docs/fixtures/competitive-domains.json}"

python3 - "$manifest" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
assert data["schema"] == "harumi-competitive-domains-v1"
assert data["policy"] == {
    "same_condition_per_domain": True,
    "pixel_differences_are_diagnostic": True,
    "native_runtime_must_be_declared": True,
    "performance_requires_warmup_and_repeats": True,
}
expected = {
    "viewer-edit": "pdfium-render",
    "complex-typesetting": "genpdf/printpdf",
    "object-surgery": "lopdf",
    "bulk-extraction": "unpdf/pdf_oxide",
    "minimal-writer": "pdf-writer",
}
required_oracles = {"viewer-edit": {"render_validity", "warnings"},
                    "complex-typesetting": {"page_geometry", "extraction"},
                    "object-surgery": {"object_graph_roundtrip", "mutation_audit"},
                    "bulk-extraction": {"text_recall", "reading_order"},
                    "minimal-writer": {"pdf_validity", "cjk_extraction"}}
root = path.parent.parent.parent
domains = data["domains"]
assert len(domains) == len(expected)
for domain in domains:
    domain_id = domain["id"]
    assert expected[domain_id] == domain["reference"]
    assert domain["fixture"].startswith("phase-52")
    fixture = root / "docs" / "fixtures" / domain["fixture"]
    assert fixture.is_file(), f"missing domain fixture: {fixture}"
    fixture_data = json.loads(fixture.read_text())
    assert fixture_data["schema"] == "harumi-competitive-domain-fixture-v1"
    assert fixture_data["domain"] == domain_id
    assert fixture_data["reference"] == domain["reference"]
    assert fixture_data["operations"]
    assert fixture_data["required_checks"]
    if domain_id == "complex-typesetting":
        required_cases = {
            "cjk_latin_mixed_paragraph", "colspan_rowspan_table",
            "nested_table_blocks", "repeated_header_and_continuation",
            "page_footnote_reservation", "stable_bookmark_and_toc",
        }
        assert required_cases <= set(fixture_data["cases"])
        assert fixture_data["acceptance"]["unexpected_overflow"] == 0
        assert fixture_data["acceptance"]["marker_order_must_be_stable"] is True
        assert fixture_data["acceptance"]["table_text_must_not_duplicate"] is True
    assert required_oracles[domain_id] <= set(domain["oracles"])
    assert domain["metrics"]
    assert domain["runtime"]
print(f"competitive domain manifest passed: {path}")
PY
