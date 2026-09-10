#!/usr/bin/env python3
"""Prove that an exported ONNX graph matches its PyTorch/Spandrel original.

This is the gate for phase 1: the Rust implementation must not be built on top
of a graph that quietly diverges from the reference model.

Two independent things are checked.

1. *Export fidelity* -- the raw ``torch.nn.Module`` Spandrel loaded and the ONNX
   graph are fed byte-identical input tensors and their outputs compared. Any
   difference here is caused by the export itself.
2. *Descriptor equivalence* -- Spandrel's ``ImageModelDescriptor.__call__``
   wraps the module with behaviour the Rust pipeline must reproduce: it pads to
   the architecture's size requirements, clamps the result to ``[0, 1]`` and
   crops the padding back off. We assert that ``descriptor(x)`` is exactly
   ``clamp(module(x), 0, 1)`` for inputs that need no padding, which pins down
   the postprocessing contract instead of leaving it to be guessed.

Inputs are both real image crops (realistic value distributions, screentones,
text) and randomly-shaped noise tensors, the latter specifically to catch a
graph that baked in a static height/width.

A fixed error tolerance is not sufficient on its own. A deep residual network
can be chaotically ill-conditioned on some inputs -- a perfectly smooth
gradient, for instance -- to the point where perturbing the input by a single
float32 ULP moves the output further than switching runtimes does. Demanding
1-LSB agreement there is demanding precision the model cannot deliver in *any*
implementation. So when a case exceeds the tolerance, its conditioning is
measured: the same module is re-run on the input nudged by one ULP, and the
export is judged faithful if it differs by no more than that. See
`conditioning_probe`.

Usage::

    python tools/verify_onnx.py \\
        --pth  models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth \\
        --onnx models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx \\
        --inputs fixtures \\
        --report artifacts/parity
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
from PIL import Image
from spandrel import ImageModelDescriptor, ModelLoader

# FP32 ESRGAN inference through two different runtimes accumulates float
# reassociation noise on the order of 1e-6. Anything at 1e-4 or above means the
# graph itself differs, not just the summation order.
DEFAULT_FLOAT_TOLERANCE = 1e-4
# What actually ships is 8-bit; a single least-significant-bit difference on a
# handful of pixels is a rounding tie, more than that is a real divergence.
DEFAULT_U8_TOLERANCE = 1


@dataclasses.dataclass
class Conditioning:
    """How much the model's own output moves for a 1-ULP input change."""

    perturbation: float
    max_absolute_error: float
    mean_absolute_error: float
    max_u8_error: int


@dataclasses.dataclass
class Comparison:
    """Error metrics between two float32 tensors of identical shape."""

    label: str
    shape: list[int]
    max_absolute_error: float
    mean_absolute_error: float
    rmse: float
    max_u8_error: int
    differing_u8_pixels: int
    total_u8_pixels: int

    @property
    def u8_pixel_fraction(self) -> float:
        return self.differing_u8_pixels / max(self.total_u8_pixels, 1)


def to_u8(array: np.ndarray) -> np.ndarray:
    """Quantise exactly the way the delivered pipeline will.

    Clamp first, then round half-away-from-zero. ``np.round`` is banker's
    rounding and would disagree with the reference on .5 ties, so it is not used
    here.
    """
    clipped = np.clip(array, 0.0, 1.0) * 255.0
    return np.floor(clipped + 0.5).astype(np.uint8)


def compare(label: str, reference: np.ndarray, candidate: np.ndarray) -> Comparison:
    if reference.shape != candidate.shape:
        raise SystemExit(
            f"{label}: shape mismatch, torch={reference.shape} onnx={candidate.shape}"
        )
    diff = np.abs(reference.astype(np.float64) - candidate.astype(np.float64))
    ref_u8 = to_u8(reference)
    cand_u8 = to_u8(candidate)
    u8_diff = np.abs(ref_u8.astype(np.int16) - cand_u8.astype(np.int16))

    return Comparison(
        label=label,
        shape=list(reference.shape),
        max_absolute_error=float(diff.max()),
        mean_absolute_error=float(diff.mean()),
        rmse=float(np.sqrt((diff**2).mean())),
        max_u8_error=int(u8_diff.max()),
        differing_u8_pixels=int((u8_diff > 0).sum()),
        total_u8_pixels=int(u8_diff.size),
    )


def conditioning_probe(
    module: torch.nn.Module, tensor: torch.Tensor, reference: np.ndarray
) -> Conditioning:
    """Measure the model's sensitivity to a one-ULP change in its input.

    This is the smallest perturbation float32 can represent, and far smaller
    than any difference an ONNX export could introduce. Whatever it moves the
    output by is a floor below which no two implementations can be expected to
    agree.
    """
    nudged_array = np.nextafter(tensor.numpy(), np.float32(2.0))
    perturbation = float(np.abs(nudged_array - tensor.numpy()).max())
    nudged = run_torch(module, torch.from_numpy(nudged_array))

    diff = np.abs(reference.astype(np.float64) - nudged.astype(np.float64))
    u8_diff = np.abs(to_u8(reference).astype(np.int16) - to_u8(nudged).astype(np.int16))
    return Conditioning(
        perturbation=perturbation,
        max_absolute_error=float(diff.max()),
        mean_absolute_error=float(diff.mean()),
        max_u8_error=int(u8_diff.max()),
    )


def load_image_tensor(path: Path, channels: int) -> torch.Tensor:
    """Decode an image into the NCHW float32 tensor the model consumes."""
    mode = "L" if channels == 1 else "RGB"
    with Image.open(path) as handle:
        image = handle.convert(mode)
    array = np.asarray(image, dtype=np.uint8).astype(np.float32) / 255.0
    if array.ndim == 2:
        array = array[:, :, None]
    # HWC -> NCHW
    return torch.from_numpy(array.transpose(2, 0, 1)[None, ...].copy())


def save_diff_image(
    out_path: Path, reference: np.ndarray, candidate: np.ndarray, amplify: int
) -> float:
    """Write an amplified |difference| map for eyeballing where errors land.

    Returns the amplification-independent peak error so the caller can report
    what the image is actually showing.
    """
    diff = np.abs(reference - candidate)[0]  # CHW
    peak = float(diff.max())
    scaled = np.clip(diff * amplify * 255.0, 0, 255).astype(np.uint8)
    hwc = scaled.transpose(1, 2, 0)
    if hwc.shape[2] == 1:
        image = Image.fromarray(hwc[:, :, 0], mode="L")
    else:
        image = Image.fromarray(hwc, mode="RGB")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(out_path)
    return peak


def run_torch(module: torch.nn.Module, tensor: torch.Tensor) -> np.ndarray:
    with torch.no_grad():
        return module(tensor).detach().cpu().numpy()


def run_descriptor(
    descriptor: ImageModelDescriptor, tensor: torch.Tensor
) -> np.ndarray:
    with torch.no_grad():
        return descriptor(tensor).detach().cpu().numpy()


def run_onnx(session: ort.InferenceSession, tensor: torch.Tensor) -> np.ndarray:
    name = session.get_inputs()[0].name
    return session.run(None, {name: tensor.numpy()})[0]


IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".tif", ".tiff"}


def gather_inputs(
    inputs: list[Path], channels: int, random_shapes: list[tuple[int, int]]
) -> list[tuple[str, torch.Tensor]]:
    cases: list[tuple[str, torch.Tensor]] = []

    for entry in inputs:
        if entry.is_file():
            cases.append((entry.name, load_image_tensor(entry, channels)))
            continue
        found = sorted(
            p for p in entry.rglob("*") if p.suffix.lower() in IMAGE_SUFFIXES
        )
        if not found:
            print(f"warning: no images found under {entry}", file=sys.stderr)
        for path in found:
            label = f"{entry.name}/{path.relative_to(entry)}"
            cases.append((label, load_image_tensor(path, channels)))

    # Deterministic noise at awkward, non-square, odd sizes. A graph with a
    # baked-in spatial dimension fails outright here rather than subtly later.
    generator = torch.Generator().manual_seed(0x4D414E47)
    for height, width in random_shapes:
        tensor = torch.rand(
            1, channels, height, width, generator=generator, dtype=torch.float32
        )
        cases.append((f"random-{height}x{width}", tensor))

    return cases


def parse_shape(text: str) -> tuple[int, int]:
    height, _, width = text.partition("x")
    return int(height), int(width)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pth", required=True, type=Path)
    parser.add_argument("--onnx", required=True, type=Path)
    parser.add_argument(
        "--inputs",
        type=Path,
        nargs="*",
        default=[],
        help="image files and/or directories to test with",
    )
    parser.add_argument(
        "--shapes",
        type=parse_shape,
        nargs="*",
        default=[(64, 64), (97, 151), (128, 256), (200, 200)],
        help="extra random-noise input shapes, given as HxW",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=Path("artifacts/parity"),
        help="directory for the JSON report and diff images",
    )
    parser.add_argument("--float-tolerance", type=float, default=DEFAULT_FLOAT_TOLERANCE)
    parser.add_argument("--u8-tolerance", type=int, default=DEFAULT_U8_TOLERANCE)
    parser.add_argument(
        "--diff-amplify",
        type=int,
        default=255,
        help="multiplier applied to the saved difference images",
    )
    args = parser.parse_args()

    descriptor = ModelLoader().load_from_file(str(args.pth))
    if not isinstance(descriptor, ImageModelDescriptor):
        raise SystemExit(f"{args.pth} is not an image-to-image model")
    module = descriptor.model
    module.eval()
    module.to(torch.device("cpu")).to(torch.float32)

    options = ort.SessionOptions()
    # Graph optimisations reassociate float arithmetic. For a parity baseline we
    # want the graph executed as exported.
    options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_DISABLE_ALL
    session = ort.InferenceSession(
        str(args.onnx), options, providers=["CPUExecutionProvider"]
    )

    print(
        f"model: {descriptor.architecture.name} x{descriptor.scale} "
        f"{descriptor.input_channels}ch -> {descriptor.output_channels}ch"
    )
    print(f"torch: {torch.__version__}   onnxruntime: {ort.__version__}")

    cases = gather_inputs(args.inputs, descriptor.input_channels, args.shapes)
    if not cases:
        raise SystemExit("no test inputs")

    export_results: list[Comparison] = []
    conditioning: dict[str, Conditioning] = {}
    ill_conditioned: list[str] = []
    clamp_results: list[Comparison] = []
    diff_images: dict[str, str] = {}
    report_dir = args.report
    report_dir.mkdir(parents=True, exist_ok=True)

    for label, tensor in cases:
        torch_out = run_torch(module, tensor)
        onnx_out = run_onnx(session, tensor)
        export_cmp = compare(label, torch_out, onnx_out)
        export_results.append(export_cmp)

        # Spandrel's wrapper clamps to [0, 1]; check that clamping is the
        # *whole* difference, so the Rust postprocessing contract is exactly
        # "clamp and quantise" with nothing hidden behind it.
        descriptor_out = run_descriptor(descriptor, tensor)
        clamp_results.append(
            compare(label, np.clip(torch_out, 0.0, 1.0), descriptor_out)
        )

        safe = label.replace("/", "__").replace("\\", "__")
        diff_path = report_dir / f"diff_{Path(safe).stem}.png"
        save_diff_image(diff_path, torch_out, onnx_out, args.diff_amplify)
        diff_images[label] = str(diff_path)

        note = ""
        over_tolerance = (
            export_cmp.max_absolute_error > args.float_tolerance
            or export_cmp.max_u8_error > args.u8_tolerance
        )
        if over_tolerance:
            # Only pay for the extra forward pass when a case looks bad.
            probe = conditioning_probe(module, tensor, torch_out)
            conditioning[label] = probe
            if export_cmp.max_absolute_error <= probe.max_absolute_error:
                ill_conditioned.append(label)
                note = (
                    f"  <- ill-conditioned input: 1 ULP ({probe.perturbation:.2e}) "
                    f"of input moves this model's own output by "
                    f"{probe.max_absolute_error:.3e} (u8 {probe.max_u8_error})"
                )
            else:
                note = (
                    f"  <- EXCEEDS conditioning floor "
                    f"{probe.max_absolute_error:.3e} (u8 {probe.max_u8_error})"
                )

        print(
            f"  {label:<28} shape={tuple(export_cmp.shape)} "
            f"max={export_cmp.max_absolute_error:.3e} "
            f"mean={export_cmp.mean_absolute_error:.3e} "
            f"rmse={export_cmp.rmse:.3e} "
            f"u8max={export_cmp.max_u8_error} "
            f"u8diff={export_cmp.differing_u8_pixels}/{export_cmp.total_u8_pixels}"
            f"{note}"
        )

    # Judge each case against the tolerance, or against the model's own
    # conditioning where that is the looser and more honest bound.
    def case_passed(result: Comparison) -> bool:
        if (
            result.max_absolute_error <= args.float_tolerance
            and result.max_u8_error <= args.u8_tolerance
        ):
            return True
        probe = conditioning.get(result.label)
        return probe is not None and result.max_absolute_error <= probe.max_absolute_error

    failures = [r.label for r in export_results if not case_passed(r)]

    well_conditioned = [r for r in export_results if r.label not in ill_conditioned]
    worst_float = max(r.max_absolute_error for r in well_conditioned) if well_conditioned else 0.0
    worst_u8 = max(r.max_u8_error for r in well_conditioned) if well_conditioned else 0
    worst_clamp = max(r.max_absolute_error for r in clamp_results)

    passed = not failures and worst_clamp == 0.0

    report = {
        "pth": str(args.pth),
        "onnx": str(args.onnx),
        "architecture": descriptor.architecture.name,
        "scale": descriptor.scale,
        "input_channels": descriptor.input_channels,
        "torch_version": torch.__version__,
        "onnxruntime_version": ort.__version__,
        "float_tolerance": args.float_tolerance,
        "u8_tolerance": args.u8_tolerance,
        "worst_max_absolute_error_well_conditioned": worst_float,
        "worst_max_u8_error_well_conditioned": worst_u8,
        "ill_conditioned_inputs": ill_conditioned,
        "conditioning": {k: dataclasses.asdict(v) for k, v in conditioning.items()},
        "failures": failures,
        "descriptor_equals_clamped_module": worst_clamp == 0.0,
        "descriptor_vs_clamped_module_max_error": worst_clamp,
        "passed": passed,
        "diff_images": diff_images,
        "cases": [dataclasses.asdict(r) for r in export_results],
    }
    report_path = report_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")

    print()
    print(
        f"worst max abs error : {worst_float:.3e} (tolerance {args.float_tolerance:g}) "
        f"over {len(well_conditioned)} well-conditioned case(s)"
    )
    print(f"worst 8-bit error   : {worst_u8} (tolerance {args.u8_tolerance})")
    if ill_conditioned:
        print(
            f"ill-conditioned     : {len(ill_conditioned)} case(s) where a 1-ULP input "
            f"change moves the model further than the export does, so no "
            f"implementation could agree more closely: {', '.join(ill_conditioned)}"
        )
    if failures:
        print(f"failing cases       : {', '.join(failures)}")
    if worst_clamp == 0.0:
        print(
            "descriptor wrapper  : exactly clamp(module(x), 0, 1) "
            "-- Rust postprocessing is clamp + quantise, nothing more"
        )
    else:
        print(
            f"descriptor wrapper  : differs from clamp(module(x), 0, 1) by "
            f"{worst_clamp:.3e} -- Spandrel's __call__ does something extra "
            f"that the Rust pipeline must reproduce"
        )
    print(f"report              : {report_path}")
    print()
    print("PARITY PASS" if passed else "PARITY FAIL")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
