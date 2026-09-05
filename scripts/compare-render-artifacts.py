#!/usr/bin/env python3
"""Compare renderer artifact reports without claiming pixel parity across engines."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_renderer(value: str) -> tuple[str, Path]:
    name, separator, path = value.partition("=")
    if not separator or not name or not path:
        raise argparse.ArgumentTypeError("renderer must use NAME=REPORT.json")
    return name, Path(path)


def normalize(report: dict[str, Any], path: Path) -> dict[str, Any]:
    pages = report.get("pages")
    page = pages[0] if isinstance(pages, list) and pages else report
    required = {
        "page_count": report.get("page_count"),
        "page_size_points": report.get("page_size_points"),
        "raster_size_pixels": page.get("raster_size_pixels"),
        "sha256": page.get("sha256"),
    }
    missing = [key for key, value in required.items() if value in (None, "")]
    if missing:
        raise ValueError(f"{path}: missing artifact fields: {', '.join(missing)}")
    return {
        "renderer": report.get("renderer", path.stem),
        **required,
    }


def load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"{path}: cannot read JSON report: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{path}: report root must be an object")
    return normalize(value, path)


def classify(reference: dict[str, Any], candidate: dict[str, Any]) -> str:
    if candidate["page_count"] != reference["page_count"]:
        return "metadata_mismatch"
    if candidate["page_size_points"] != reference["page_size_points"]:
        return "page_size_mismatch"
    if candidate["raster_size_pixels"] != reference["raster_size_pixels"]:
        return "raster_dimensions_differ"
    if candidate["sha256"] == reference["sha256"]:
        return "same_raster_sha256"
    return "pixel_difference"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare renderer reports while keeping engine differences explicit."
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--reference", default="poppler")
    parser.add_argument(
        "--renderer",
        action="append",
        type=parse_renderer,
        required=True,
        metavar="NAME=REPORT.json",
    )
    args = parser.parse_args()

    reports = {name: load(path) for name, path in args.renderer}
    if args.reference not in reports:
        parser.error(f"reference renderer is missing: {args.reference}")
    reference = reports[args.reference]
    comparisons = {}
    for name, report in reports.items():
        comparisons[name] = {
            **report,
            "classification": "reference"
            if name == args.reference
            else classify(reference, report),
        }

    result = {
        "contract": "render-comparison-v1",
        "reference": args.reference,
        "interpretation": {
            "same_raster_sha256": "same bytes under the pinned renderer artifact contract",
            "pixel_difference": "different raster; investigate before assigning blame",
            "metadata_mismatch": "page count differs; not a pixel comparison",
            "page_size_mismatch": "page dimensions differ; not a pixel comparison",
            "raster_dimensions_differ": "target raster dimensions differ; not a pixel comparison",
        },
        "renderers": comparisons,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"render comparison written: {args.output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"error: {error}", flush=True)
        raise SystemExit(1) from error
