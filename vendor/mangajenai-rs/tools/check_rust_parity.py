#!/usr/bin/env python3
"""Hold the Rust pipeline to the Python ONNX Runtime reference, pixel for pixel.

Runs `mangajanai-rs upscale` and `tools/onnx_upscale.py` over the same inputs
with the same model and compares the encoded results. Both go through ONNX
Runtime with the same graph, so the only thing under test is the surrounding
pipeline: decoding, normalisation, layout, clamping, quantisation and encoding.
Anything other than an exact match means the Rust side has a bug.

    python tools/check_rust_parity.py \\
        --binary target/debug/mangajanai-rs \\
        --model models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx \\
        fixtures/general/lineart_strokes.png ...
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image


def load(path: Path) -> np.ndarray:
    with Image.open(path) as handle:
        return np.asarray(handle.convert("RGB"), dtype=np.uint8)


def run(command: list[str]) -> float:
    started = time.perf_counter()
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(f"command failed: {' '.join(command)}")
    return time.perf_counter() - started


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/mangajanai-rs"))
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--work-dir", type=Path, default=Path("artifacts/rust-parity"))
    parser.add_argument("--tolerance", type=int, default=0)
    parser.add_argument("inputs", nargs="+", type=Path)
    args = parser.parse_args()

    args.work_dir.mkdir(parents=True, exist_ok=True)
    results = []
    failures = 0

    for source in args.inputs:
        stem = source.stem
        rust_out = args.work_dir / f"{stem}.rust.png"
        python_out = args.work_dir / f"{stem}.python.png"

        rust_seconds = run(
            [
                str(args.binary),
                "upscale",
                "--model",
                str(args.model),
                str(source),
                str(rust_out),
            ]
        )
        python_seconds = run(
            [
                sys.executable,
                "tools/onnx_upscale.py",
                "--model",
                str(args.model),
                str(source),
                str(python_out),
            ]
        )

        left = load(rust_out)
        right = load(python_out)
        if left.shape != right.shape:
            print(f"  {stem:<28} FAIL shape {left.shape} vs {right.shape}")
            failures += 1
            continue

        diff = np.abs(left.astype(np.int16) - right.astype(np.int16))
        max_diff = int(diff.max())
        differing = int((diff > 0).sum())
        ok = max_diff <= args.tolerance
        failures += 0 if ok else 1

        results.append(
            {
                "input": str(source),
                "shape": list(left.shape),
                "max_diff": max_diff,
                "differing_samples": differing,
                "total_samples": int(diff.size),
                "rust_seconds": rust_seconds,
                "python_seconds": python_seconds,
                "match": ok,
            }
        )
        print(
            f"  {stem:<28} {left.shape[1]}x{left.shape[0]:<6} "
            f"max_diff={max_diff} differing={differing} "
            f"rust={rust_seconds:6.2f}s python={python_seconds:6.2f}s "
            f"{'OK' if ok else 'FAIL'}"
        )

    report = args.work_dir / "report.json"
    report.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\nreport: {report}")

    if failures:
        print(f"{failures} mismatch(es)")
        return 1
    print(f"all {len(results)} input(s) match the Python reference exactly")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
