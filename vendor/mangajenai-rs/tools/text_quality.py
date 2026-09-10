#!/usr/bin/env python3
"""Measure how well tiny Japanese text survives upscaling.

This is the measurement half of phase 8. No restoration algorithm is proposed
here; the point is to make quality changes *judgeable* before anything is
changed, so an "improvement" can be shown rather than asserted.

It works because `fixtures/groundtruth/` holds matched pairs: `<scene>_1x.png`
and `<scene>_4x.png` are the same scene rendered at both resolutions, so the
4x render is genuine high-resolution ground truth. Upscaling the 1x version
and comparing against the 4x version measures what the model actually loses --
something no reference-free sharpness metric can do.

A **Lanczos baseline** is measured alongside. Without it there is no way to
know whether the model is helping tiny text or merely changing it: if a
learned upscaler scores worse than plain resampling on furigana, that is the
single most important thing to know before touching the algorithm.

Metrics, all against the ground truth:

* `rmse` / `psnr` -- overall fidelity.
* `stroke_survival` -- fraction of ground-truth ink pixels that are still ink.
  Strokes vanishing entirely is the failure mode that matters most for ruby.
* `stroke_darkness` -- mean level where the ground truth has ink. Higher means
  strokes have been washed out toward paper.
* `background_cleanliness` -- mean level where the ground truth is paper.
  Lower means haloing or smearing into the margins.
* `edge_energy_ratio` -- mean gradient magnitude relative to ground truth.
  Below 1.0 is blurring; far above 1.0 is ringing.

    python tools/text_quality.py --model models/4x_....onnx \\
        --binary target/debug/mangajanai-rs
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

# Below this level a pixel counts as ink. Manga line art is near-bilevel, so
# the exact value matters little; 128 is the midpoint and avoids tuning.
INK_THRESHOLD = 128


def load_gray(path: Path) -> np.ndarray:
    with Image.open(path) as handle:
        return np.asarray(handle.convert("L"), dtype=np.float64)


def gradient_energy(image: np.ndarray) -> float:
    gy, gx = np.gradient(image)
    return float(np.hypot(gx, gy).mean())


def measure(reference: np.ndarray, candidate: np.ndarray) -> dict[str, float]:
    if reference.shape != candidate.shape:
        raise SystemExit(
            f"shape mismatch: reference {reference.shape} vs candidate {candidate.shape}"
        )

    error = reference - candidate
    mse = float((error**2).mean())
    rmse = float(np.sqrt(mse))
    psnr = float("inf") if mse == 0 else 10.0 * np.log10(255.0**2 / mse)

    ink = reference < INK_THRESHOLD
    paper = ~ink
    ink_count = int(ink.sum())

    reference_energy = gradient_energy(reference)

    return {
        "rmse": rmse,
        "psnr": psnr,
        "stroke_survival": (
            float((candidate[ink] < INK_THRESHOLD).mean()) if ink_count else float("nan")
        ),
        "stroke_darkness": float(candidate[ink].mean()) if ink_count else float("nan"),
        "reference_stroke_darkness": float(reference[ink].mean()) if ink_count else float("nan"),
        "background_cleanliness": float(candidate[paper].mean()) if paper.any() else float("nan"),
        "edge_energy_ratio": (
            gradient_energy(candidate) / reference_energy if reference_energy else float("nan")
        ),
        "ink_pixels": ink_count,
    }


def lanczos_upscale(source: Path, scale: int, destination: Path) -> None:
    with Image.open(source) as handle:
        image = handle.convert("RGB")
        image = image.resize(
            (image.width * scale, image.height * scale), Image.Resampling.LANCZOS
        )
    image.save(destination)


def triptych(reference: Path, model: Path, baseline: Path, out: Path, crop: tuple) -> None:
    """Reference | model | baseline, stacked for eyeballing at a glance."""
    panels = []
    for path in (reference, model, baseline):
        with Image.open(path) as handle:
            panels.append(handle.convert("L").crop(crop))
    width, height = panels[0].size
    sheet = Image.new("L", (width, height * 3 + 8), 255)
    for index, panel in enumerate(panels):
        sheet.paste(panel, (0, index * (height + 4)))
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path, help="exported .onnx")
    parser.add_argument("--binary", type=Path, default=Path("target/debug/mangajanai-rs"))
    parser.add_argument("--fixtures", type=Path, default=Path("fixtures/groundtruth"))
    parser.add_argument("--scale", type=int, default=4)
    parser.add_argument("--work-dir", type=Path, default=Path("artifacts/text-quality"))
    parser.add_argument("--device", default="cpu")
    args = parser.parse_args()

    args.work_dir.mkdir(parents=True, exist_ok=True)
    scenes = sorted(p.stem[:-3] for p in args.fixtures.glob(f"*_1x.png"))
    if not scenes:
        raise SystemExit(f"no *_1x.png fixtures under {args.fixtures}")

    results = []
    for scene in scenes:
        source = args.fixtures / f"{scene}_1x.png"
        reference_path = args.fixtures / f"{scene}_{args.scale}x.png"
        if not reference_path.exists():
            print(f"  {scene}: no {args.scale}x ground truth, skipping")
            continue

        model_out = args.work_dir / f"{scene}.model.png"
        baseline_out = args.work_dir / f"{scene}.lanczos.png"

        result = subprocess.run(
            [
                str(args.binary), "upscale",
                "--model", str(args.model),
                "--device", args.device,
                "--tile-size", "0",
                str(source), str(model_out),
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            sys.stderr.write(result.stderr)
            raise SystemExit(f"upscale failed for {scene}")

        lanczos_upscale(source, args.scale, baseline_out)

        reference = load_gray(reference_path)
        model_metrics = measure(reference, load_gray(model_out))
        baseline_metrics = measure(reference, load_gray(baseline_out))

        results.append(
            {"scene": scene, "model": model_metrics, "lanczos": baseline_metrics}
        )

        crop = (0, 0, min(reference.shape[1], 480), min(reference.shape[0], 160))
        triptych(
            reference_path, model_out, baseline_out,
            args.work_dir / f"{scene}.triptych.png", crop,
        )

        print(f"\n  {scene}  ({model_metrics['ink_pixels']} ink pixels in the reference)")
        print(f"    {'':<24}{'model':>12}{'lanczos':>12}")
        for key in (
            "psnr", "rmse", "stroke_survival", "stroke_darkness",
            "background_cleanliness", "edge_energy_ratio",
        ):
            better = ""
            m, b = model_metrics[key], baseline_metrics[key]
            if not (np.isnan(m) or np.isnan(b)):
                if key in ("psnr", "stroke_survival"):
                    better = "  model" if m > b else "  lanczos"
                elif key in ("rmse", "stroke_darkness"):
                    better = "  model" if m < b else "  lanczos"
            print(f"    {key:<24}{m:>12.4f}{b:>12.4f}{better}")
        print(
            f"    {'(reference stroke level)':<24}"
            f"{model_metrics['reference_stroke_darkness']:>12.4f}"
        )

    report = args.work_dir / "text_quality.json"
    report.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\nreport: {report}")
    print("triptychs (reference / model / lanczos) written alongside it")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
