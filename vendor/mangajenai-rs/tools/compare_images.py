#!/usr/bin/env python3
"""Compare two images pixel for pixel and exit non-zero if they differ.

Used to hold the Rust pipeline to the Python reference output.

    python tools/compare_images.py a.png b.png [--tolerance 0]
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
from PIL import Image


def load(path: Path) -> np.ndarray:
    with Image.open(path) as handle:
        return np.asarray(handle.convert("RGB"), dtype=np.uint8)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path)
    parser.add_argument(
        "--tolerance",
        type=int,
        default=0,
        help="maximum permitted per-channel difference (default: exact match)",
    )
    parser.add_argument("--diff", type=Path, help="write an amplified difference image")
    args = parser.parse_args()

    left = load(args.left)
    right = load(args.right)

    if left.shape != right.shape:
        print(f"FAIL shape mismatch: {left.shape} vs {right.shape}")
        return 1

    diff = np.abs(left.astype(np.int16) - right.astype(np.int16))
    max_diff = int(diff.max())
    differing = int((diff > 0).sum())
    total = int(diff.size)

    if args.diff is not None:
        args.diff.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray(
            np.clip(diff * 64, 0, 255).astype(np.uint8), mode="RGB"
        ).save(args.diff)

    print(
        f"{args.left.name} vs {args.right.name}: shape={left.shape} "
        f"max_diff={max_diff} differing={differing}/{total} "
        f"({100.0 * differing / total:.4f}%)"
    )

    if max_diff > args.tolerance:
        print(f"FAIL exceeds tolerance {args.tolerance}")
        return 1
    print("MATCH" if max_diff == 0 else f"WITHIN TOLERANCE {args.tolerance}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
