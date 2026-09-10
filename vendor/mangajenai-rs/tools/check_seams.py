#!/usr/bin/env python3
"""Detect tile seams in an upscaled page.

A seam is a discontinuity that runs the full height (or width) of the image at
exactly a tile boundary. Eyeballing is unreliable on screentone, so this
measures it: for every column, take the mean absolute difference against the
previous column, then ask whether the columns at tile boundaries stand out from
the distribution of all the others.

Reported as a robust z-score using the median and the median absolute
deviation, because the distribution of column differences on manga line art is
extremely heavy-tailed -- a mean and standard deviation would be dominated by
panel borders and dialogue.

    python tools/check_seams.py out.png --tile-size 512 --overlap 32 --scale 4

Optionally compares against a whole-page (untiled) render of the same input,
which measures how much tiling changed the result overall:

    python tools/check_seams.py out.png --reference untiled.png ...
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def load(path: Path) -> np.ndarray:
    with Image.open(path) as handle:
        return np.asarray(handle.convert("RGB"), dtype=np.float64)


def boundaries(length: int, tile: int, overlap: int, scale: int) -> list[int]:
    """Output coordinates where tile edges land, mirroring `plan_axis`."""
    source_length = length // scale
    if source_length <= tile:
        return []

    step = tile - overlap
    starts: list[int] = []
    start = 0
    while True:
        if start + tile >= source_length:
            final = source_length - tile
            if not starts or starts[-1] != final:
                starts.append(final)
            break
        starts.append(start)
        start += step

    edges: set[int] = set()
    for tile_start in starts:
        for edge in (tile_start, tile_start + tile):
            scaled = edge * scale
            if 0 < scaled < length:
                edges.add(scaled)
    return sorted(edges)


def robust_z(values: np.ndarray, indices: list[int]) -> list[float]:
    median = float(np.median(values))
    mad = float(np.median(np.abs(values - median)))
    # 1.4826 makes the MAD a consistent estimator of sigma for normal data.
    sigma = mad * 1.4826
    if sigma <= 0:
        sigma = float(values.std()) or 1.0
    return [float((values[i] - median) / sigma) for i in indices]


def axis_report(
    image: np.ndarray, axis: int, tile: int, overlap: int, scale: int
) -> dict:
    """`axis=1` checks vertical seams (column differences), `axis=0` horizontal."""
    if axis == 1:
        differences = np.abs(np.diff(image, axis=1)).mean(axis=(0, 2))
        length = image.shape[1]
    else:
        differences = np.abs(np.diff(image, axis=0)).mean(axis=(1, 2))
        length = image.shape[0]

    edges = boundaries(length, tile, overlap, scale)
    # np.diff index i is the step between i and i+1, so an edge at coordinate e
    # is the step at index e-1.
    indices = [e - 1 for e in edges if 0 < e <= len(differences)]
    if not indices:
        return {"boundaries": [], "max_z": 0.0, "worst_boundary": None}

    scores = robust_z(differences, indices)
    worst = int(np.argmax(scores))
    return {
        "boundaries": [
            {"coordinate": edges[i], "difference": float(differences[indices[i]]), "z": scores[i]}
            for i in range(len(indices))
        ],
        "max_z": float(max(scores)),
        "worst_boundary": edges[worst],
        "median_difference": float(np.median(differences)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--tile-size", type=int, required=True)
    parser.add_argument("--overlap", type=int, required=True)
    parser.add_argument("--scale", type=int, default=4)
    parser.add_argument(
        "--reference",
        type=Path,
        help="whole-page render of the same input, for an overall difference",
    )
    parser.add_argument(
        "--max-z",
        type=float,
        default=6.0,
        help="fail if a tile boundary stands this far out of the distribution",
    )
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()

    image = load(args.image)
    vertical = axis_report(image, 1, args.tile_size, args.overlap, args.scale)
    horizontal = axis_report(image, 0, args.tile_size, args.overlap, args.scale)

    result = {
        "image": str(args.image),
        "shape": list(image.shape),
        "tile_size": args.tile_size,
        "overlap": args.overlap,
        "scale": args.scale,
        "vertical_seams": vertical,
        "horizontal_seams": horizontal,
    }

    print(f"{args.image.name}  {image.shape[1]}x{image.shape[0]}")
    for name, report in (("vertical", vertical), ("horizontal", horizontal)):
        count = len(report["boundaries"])
        if count == 0:
            print(f"  {name:<11} no tile boundaries on this axis")
            continue
        print(
            f"  {name:<11} {count} boundaries, worst z={report['max_z']:+.2f} "
            f"at {report['worst_boundary']}"
        )

    if args.reference is not None:
        reference = load(args.reference)
        if reference.shape != image.shape:
            print(f"  reference   shape mismatch {reference.shape} vs {image.shape}")
            return 1
        difference = np.abs(image - reference)
        result["vs_reference"] = {
            "max": float(difference.max()),
            "mean": float(difference.mean()),
            "differing_samples": int((difference > 0).sum()),
            "total_samples": int(difference.size),
        }
        print(
            f"  vs untiled  max={difference.max():.0f} mean={difference.mean():.4f} "
            f"differing={(difference > 0).sum()}/{difference.size}"
        )

    if args.report is not None:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=2) + "\n")

    worst = max(vertical["max_z"], horizontal["max_z"])
    if worst > args.max_z:
        print(f"\nFAIL: a tile boundary stands out at z={worst:+.2f} (limit {args.max_z})")
        return 1
    print(f"\nNO SEAMS DETECTED (worst boundary z={worst:+.2f}, limit {args.max_z})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
