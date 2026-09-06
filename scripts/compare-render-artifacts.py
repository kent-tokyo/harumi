#!/usr/bin/env python3
"""Compare renderer artifact reports without claiming pixel parity across engines."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


def parse_renderer(value: str) -> tuple[str, Path]:
    name, separator, path = value.partition("=")
    if not separator or not name or not path:
        raise argparse.ArgumentTypeError("renderer must use NAME=REPORT.json")
    return name, Path(path)


def normalize(report: dict[str, Any], path: Path) -> dict[str, Any]:
    pages = report.get("pages")
    if not isinstance(pages, list) or not pages:
        pages = [report]
    if any(not isinstance(page, dict) for page in pages):
        raise ValueError(f"{path}: pages must be an array of objects")
    normalized_pages = []
    for index, page in enumerate(pages, start=1):
        required_page = {
            "page": page.get("page", index),
            "raster_size_pixels": page.get("raster_size_pixels"),
            "sha256": page.get("sha256"),
        }
        missing_page = [
            key for key, value in required_page.items() if value in (None, "")
        ]
        if missing_page:
            raise ValueError(
                f"{path}: page {index} missing artifact fields: {', '.join(missing_page)}"
            )
        normalized_pages.append(required_page)
    page = normalized_pages[0]
    required = {
        "page_count": report.get("page_count"),
        "page_size_points": report.get("page_size_points"),
        "raster_size_pixels": page["raster_size_pixels"],
        "sha256": page["sha256"],
    }
    missing = [key for key, value in required.items() if value in (None, "")]
    if missing:
        raise ValueError(f"{path}: missing artifact fields: {', '.join(missing)}")
    return {
        "renderer": report.get("renderer", path.stem),
        "pages": normalized_pages,
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


def parse_page_size(value: Any) -> tuple[float, float] | None:
    if not isinstance(value, str):
        return None
    match = re.match(r"^\s*([0-9]+(?:\.[0-9]+)?)\s*x\s*([0-9]+(?:\.[0-9]+)?)", value)
    if not match:
        return None
    return float(match.group(1)), float(match.group(2))


def page_size_delta(reference: dict[str, Any], candidate: dict[str, Any]) -> list[float] | None:
    reference_size = parse_page_size(reference["page_size_points"])
    candidate_size = parse_page_size(candidate["page_size_points"])
    if not reference_size or not candidate_size:
        return None
    return [round(candidate_value - reference_value, 4) for reference_value, candidate_value in zip(reference_size, candidate_size)]


def classify(reference: dict[str, Any], candidate: dict[str, Any]) -> str:
    if candidate["page_count"] != reference["page_count"]:
        return "metadata_mismatch"
    if candidate["page_size_points"] != reference["page_size_points"]:
        reference_size = parse_page_size(reference["page_size_points"])
        candidate_size = parse_page_size(candidate["page_size_points"])
        if reference_size and candidate_size and all(
            abs(left - right) <= 0.5
            for left, right in zip(reference_size, candidate_size)
        ):
            return "page_size_rounding_difference"
        return "page_size_mismatch"
    if len(candidate["pages"]) != len(reference["pages"]):
        return "metadata_mismatch"
    if any(
        candidate_page["raster_size_pixels"] != reference_page["raster_size_pixels"]
        for reference_page, candidate_page in zip(
            reference["pages"], candidate["pages"]
        )
    ):
        return "raster_dimensions_differ"
    if all(
        candidate_page["sha256"] == reference_page["sha256"]
        for reference_page, candidate_page in zip(
            reference["pages"], candidate["pages"]
        )
    ):
        return "same_raster_sha256"
    return "pixel_difference"


def classify_pages(reference: dict[str, Any], candidate: dict[str, Any]) -> list[dict[str, Any]]:
    comparisons = []
    for index, (reference_page, candidate_page) in enumerate(
        zip(reference["pages"], candidate["pages"]), start=1
    ):
        if candidate_page["raster_size_pixels"] != reference_page["raster_size_pixels"]:
            classification = "raster_dimensions_differ"
        elif candidate_page["sha256"] == reference_page["sha256"]:
            classification = "same_raster_sha256"
        else:
            classification = "pixel_difference"
        comparisons.append(
            {
                "page": index,
                "reference_sha256": reference_page["sha256"],
                "candidate_sha256": candidate_page["sha256"],
                "reference_raster_size_pixels": reference_page["raster_size_pixels"],
                "candidate_raster_size_pixels": candidate_page["raster_size_pixels"],
                "classification": classification,
            }
        )
    return comparisons


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
        if name != args.reference:
            comparisons[name]["page_size_delta_points"] = page_size_delta(reference, report)
            comparisons[name]["page_comparisons"] = classify_pages(reference, report)

    result = {
        "contract": "render-comparison-v1",
        "reference": args.reference,
        "interpretation": {
            "same_raster_sha256": "same bytes under the pinned renderer artifact contract",
            "pixel_difference": "different raster; investigate before assigning blame",
            "metadata_mismatch": "page count differs; not a pixel comparison",
            "page_size_mismatch": "page dimensions differ; not a pixel comparison",
            "page_size_rounding_difference": "page dimensions differ by at most 0.5 points per axis; treat as a physical-size rounding difference before investigating layout",
            "page_size_delta_points": "candidate minus reference page width and height in points when both reports expose parseable dimensions",
            "raster_dimensions_differ": "target raster dimensions differ; not a pixel comparison",
            "page_comparisons": "per-page diagnostic results; an overall pixel difference may be isolated to one or more pages",
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
