#!/usr/bin/env python3
"""Reference upscaler: the exact pipeline the Rust implementation must match.

Deliberately minimal and written to mirror `docs/REFERENCE-SEMANTICS.md` step
for step, so that when `mangajanai-rs upscale` and this script disagree, the
disagreement is in the Rust code and not in an incidental difference of image
library behaviour.

    python tools/onnx_upscale.py --model models/foo.onnx page.png out.png
"""

from __future__ import annotations

import argparse
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image


def ceil_to_multiple(value: int, multiple: int) -> int:
    if value % multiple == 0:
        return value
    return (value // multiple + 1) * multiple


def read_metadata(session: ort.InferenceSession) -> dict[str, str]:
    return dict(session.get_modelmeta().custom_metadata_map)


def pad_reflect_replicate(
    array: np.ndarray, pad_width: int, pad_height: int
) -> np.ndarray:
    """Right/bottom padding: reflect up to size-1, replicate the remainder.

    Mirrors spandrel's ``pad_tensor``.
    """
    if pad_width == 0 and pad_height == 0:
        return array

    _, _, height, width = array.shape
    reflect_w = min(pad_width, width - 1)
    reflect_h = min(pad_height, height - 1)

    if reflect_w or reflect_h:
        array = np.pad(
            array, ((0, 0), (0, 0), (0, reflect_h), (0, reflect_w)), mode="reflect"
        )
    remaining_w = pad_width - reflect_w
    remaining_h = pad_height - reflect_h
    if remaining_w or remaining_h:
        array = np.pad(
            array, ((0, 0), (0, 0), (0, remaining_h), (0, remaining_w)), mode="edge"
        )
    return array


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--disable-graph-optimizations",
        action="store_true",
        help="run the graph exactly as exported (matches verify_onnx.py)",
    )
    args = parser.parse_args()

    options = ort.SessionOptions()
    if args.disable_graph_optimizations:
        options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_DISABLE_ALL
    started = time.perf_counter()
    session = ort.InferenceSession(
        str(args.model), options, providers=["CPUExecutionProvider"]
    )
    load_seconds = time.perf_counter() - started

    metadata = read_metadata(session)
    scale = int(metadata.get("mangajanai.scale", 0))
    if scale <= 0:
        raise SystemExit(
            f"{args.model} has no mangajanai.scale metadata; re-export it with "
            f"tools/export_onnx.py"
        )
    minimum = int(metadata.get("mangajanai.size_minimum", 0))
    multiple_of = max(int(metadata.get("mangajanai.size_multiple_of", 1)), 1)
    square = metadata.get("mangajanai.size_square", "0") != "0"

    # decode RGB -> f32 [0,1] -> NCHW
    with Image.open(args.input) as handle:
        image = handle.convert("RGB")
    source = np.asarray(image, dtype=np.uint8)
    height, width = source.shape[:2]
    tensor = (source.astype(np.float32) / 255.0).transpose(2, 0, 1)[None, ...]

    # pad to size requirements
    target_w = ceil_to_multiple(max(minimum, width), multiple_of)
    target_h = ceil_to_multiple(max(minimum, height), multiple_of)
    if square:
        target_w = target_h = max(target_w, target_h)
    tensor = pad_reflect_replicate(tensor, target_w - width, target_h - height)

    # inference
    input_name = session.get_inputs()[0].name
    started = time.perf_counter()
    output = session.run(None, {input_name: np.ascontiguousarray(tensor)})[0]
    inference_seconds = time.perf_counter() - started

    # clamp, crop padding, quantise, encode
    output = np.clip(output, 0.0, 1.0)
    output = output[:, :, : height * scale, : width * scale]
    quantised = np.floor(output[0].transpose(1, 2, 0) * 255.0 + 0.5).astype(np.uint8)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(quantised, mode="RGB").save(args.output)

    print(
        f"{args.input} {width}x{height} -> {args.output} "
        f"{quantised.shape[1]}x{quantised.shape[0]} "
        f"(load {load_seconds:.2f}s, inference {inference_seconds:.2f}s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
