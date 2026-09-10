#!/usr/bin/env python3
"""Export a MangaJaNai ``.pth`` checkpoint to an FP32 ONNX graph.

This is the one-time conversion step. Everything downstream (the Rust pipeline)
consumes only the ``.onnx`` file plus the JSON sidecar written next to it, so
PyTorch and Spandrel are never needed at inference time.

Spandrel owns architecture detection and state-dict parsing; we ask it for the
underlying ``torch.nn.Module`` and export that. The descriptor also tells us the
properties the Rust side must honour -- scale factor, channel count and input
size requirements -- which we record in the sidecar rather than hardcoding
anywhere.

Usage::

    python tools/export_onnx.py models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth
    python tools/export_onnx.py --all-in-dir /path/to/models --out-dir models
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import sys
import traceback
from pathlib import Path
from typing import Any

import torch

try:
    from spandrel import ImageModelDescriptor, ModelLoader
except ImportError as exc:  # pragma: no cover - environment problem, not logic
    raise SystemExit(
        "spandrel is required. Enter the dev shell with `nix develop`, or "
        "`pip install -r tools/requirements.txt`."
    ) from exc


DEFAULT_OPSET = 18
# Bounds for the dynamic height/width dimensions. The lower bound keeps the
# symbolic shape solver away from degenerate sizes (an ESRGAN body downsamples
# nothing, but torch.export still refuses ranges that include 0/1); the upper
# bound is generous enough for a full 2048p page processed untiled.
MIN_DYNAMIC_EXTENT = 16
MAX_DYNAMIC_EXTENT = 8192


@dataclasses.dataclass
class ExportResult:
    """Everything the Rust side (and the parity checker) needs to know."""

    name: str
    path: str
    architecture: str
    scale: int
    input_channels: int
    output_channels: int
    target_height: int | None
    size_requirements: dict[str, Any]
    opset: int
    exporter: str
    adapters: list[str]
    dynamic_axes: list[str]
    input_names: list[str]
    output_names: list[str]
    source_pth: str
    source_pth_sha256: str
    onnx_sha256: str
    torch_version: str
    spandrel_version: str


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def infer_target_height(name: str) -> int | None:
    """Recover the source-resolution bucket from a MangaJaNai filename.

    MangaJaNai ships one model per source height (``..._1600p_...``) because
    halftone frequency depends on the resolution of the original scan. Phase 4
    uses this to pick a model automatically; here we only record it.
    """
    for token in name.replace("-", "_").split("_"):
        if token.endswith("p") and token[:-1].isdigit():
            return int(token[:-1])
    return None


def load_descriptor(model_path: Path) -> ImageModelDescriptor:
    descriptor = ModelLoader().load_from_file(str(model_path))
    if not isinstance(descriptor, ImageModelDescriptor):
        raise SystemExit(
            f"{model_path.name}: expected an image-to-image model, got "
            f"{type(descriptor).__name__}"
        )
    return descriptor


def export_dynamo(
    module: torch.nn.Module,
    dummy: torch.Tensor,
    out_path: Path,
    opset: int,
    input_names: list[str],
    output_names: list[str],
) -> None:
    """Export via the TorchDynamo-based exporter (the current default path).

    Dynamic height/width are expressed with ``torch.export.Dim`` rather than the
    legacy ``dynamic_axes`` dict; the dynamo exporter traces real symbolic shapes
    and will raise if the graph secretly depends on a concrete size, which is
    exactly the signal we want before shipping a model.
    """
    from torch.export import Dim

    dynamic_shapes = (
        {
            2: Dim("height", min=MIN_DYNAMIC_EXTENT, max=MAX_DYNAMIC_EXTENT),
            3: Dim("width", min=MIN_DYNAMIC_EXTENT, max=MAX_DYNAMIC_EXTENT),
        },
    )

    torch.onnx.export(
        module,
        (dummy,),
        str(out_path),
        input_names=input_names,
        output_names=output_names,
        opset_version=opset,
        dynamic_shapes=dynamic_shapes,
        dynamo=True,
        optimize=True,
        external_data=False,
    )


def export_legacy(
    module: torch.nn.Module,
    dummy: torch.Tensor,
    out_path: Path,
    opset: int,
    input_names: list[str],
    output_names: list[str],
) -> None:
    """Fallback to the TorchScript exporter when dynamo cannot trace the graph."""
    torch.onnx.export(
        module,
        (dummy,),
        str(out_path),
        input_names=input_names,
        output_names=output_names,
        opset_version=opset,
        do_constant_folding=True,
        dynamic_axes={
            input_names[0]: {2: "height", 3: "width"},
            output_names[0]: {2: "height", 3: "width"},
        },
        dynamo=False,
    )


def fold_reparameterising_convs(module: torch.nn.Module) -> int:
    """Fold SPAN-style reparameterising convolutions before tracing.

    SPAN's ``Conv3XC.forward`` calls ``update_params()`` on *every* eval
    forward pass, which assigns to ``self.eval_conv.weight.data`` -- an
    in-place mutation of a Parameter during the traced call. Exporting that
    does not raise; it segfaults inside ``libtorch_cpu.so`` with a null
    dereference.

    The recomputation is redundant: with frozen weights ``update_params()`` is
    deterministic, so running it once and then skipping it leaves the eval
    forward mathematically identical. We call it once and shadow the method
    with a no-op on each instance, which keeps the rest of ``forward``
    (padding, the skip connection, the LeakyReLU) exactly as written rather
    than reimplementing it.

    This is a rewrite of *when* an operation runs, not a substitution of one
    operation for another; `export_one` still verifies the module's output is
    unchanged before exporting.

    Returns the number of modules folded.
    """
    targets = [
        child
        for child in module.modules()
        if callable(getattr(child, "update_params", None))
        and hasattr(child, "eval_conv")
    ]
    for child in targets:
        child.update_params()
        # Instance attribute shadows the class method for later calls.
        child.update_params = lambda *args, **kwargs: None
    return len(targets)


def annotate_metadata(out_path: Path, properties: dict[str, str]) -> None:
    """Write model properties into the ONNX graph's ``metadata_props``.

    The Rust side reads these back through ONNX Runtime's session metadata, so
    a ``.onnx`` file is self-describing: scale factor, channel count and size
    requirements travel with the graph instead of living in a sidecar that can
    be separated from it or go stale.
    """
    import onnx

    model = onnx.load(str(out_path))
    existing = {entry.key for entry in model.metadata_props}
    for key, value in properties.items():
        if key in existing:
            continue
        entry = model.metadata_props.add()
        entry.key = key
        entry.value = value
    onnx.save(model, str(out_path))


def check_onnx(out_path: Path) -> None:
    import onnx

    model = onnx.load(str(out_path))
    onnx.checker.check_model(model, full_check=True)


def describe_io(out_path: Path) -> tuple[list[str], list[str], list[str]]:
    """Return (input names, output names, symbolic dim names) from the graph."""
    import onnx

    model = onnx.load(str(out_path))
    inputs = [i.name for i in model.graph.input]
    outputs = [o.name for o in model.graph.output]
    dynamic: list[str] = []
    for value in list(model.graph.input) + list(model.graph.output):
        for dim in value.type.tensor_type.shape.dim:
            if dim.HasField("dim_param") and dim.dim_param not in dynamic:
                dynamic.append(dim.dim_param)
    return inputs, outputs, dynamic


def export_one(
    model_path: Path,
    out_dir: Path,
    opset: int,
    dummy_size: int,
    force_legacy: bool,
    device: str = "cpu",
) -> ExportResult:
    import spandrel

    descriptor = load_descriptor(model_path)
    module = descriptor.model
    module.eval()

    # Explicit device selection also applies to the tracing/reference input.
    # GPU export never retries inference on the CPU.
    if device == "cuda" and not torch.cuda.is_available():
        raise RuntimeError("CUDA/ROCm GPU is unavailable; CPU fallback is disabled")
    if device == "mps" and not torch.backends.mps.is_available():
        raise RuntimeError("Metal GPU is unavailable; CPU fallback is disabled")
    module.to(torch.device(device))
    module.to(torch.float32)

    input_names = ["input"]
    output_names = ["output"]

    # Use the channel count the architecture actually declares rather than
    # assuming 3; assuming would silently export a differently-shaped graph.
    channels = descriptor.input_channels
    dummy = torch.rand(1, channels, dummy_size, dummy_size, dtype=torch.float32, device=device)

    # Some architectures are not traceable as written. Adapters rewrite *when*
    # an operation runs, never what it computes, and the result is checked
    # against the untouched module below before anything is exported.
    adapters: list[str] = []
    with torch.no_grad():
        reference = module(dummy).clone()

    folded = fold_reparameterising_convs(module)
    if folded:
        adapters.append(f"fold-reparameterising-convs({folded})")

    if adapters:
        with torch.no_grad():
            adapted = module(dummy)
        drift = float((reference - adapted).abs().max())
        if drift != 0.0:
            raise SystemExit(
                f"{model_path.name}: export adapters changed the model output by "
                f"{drift:.3e}; refusing to export a graph that does not match the "
                f"original"
            )
        print(f"    adapters: {', '.join(adapters)} (output unchanged)")

    out_path = out_dir / f"{model_path.stem}.onnx"
    out_dir.mkdir(parents=True, exist_ok=True)

    exporter = "legacy"
    with torch.no_grad():
        if force_legacy:
            export_legacy(module, dummy, out_path, opset, input_names, output_names)
        else:
            try:
                export_dynamo(module, dummy, out_path, opset, input_names, output_names)
                exporter = "dynamo"
            except Exception:
                print(
                    f"  ! dynamo export failed for {model_path.name}, falling back "
                    f"to the TorchScript exporter:",
                    file=sys.stderr,
                )
                traceback.print_exc()
                export_legacy(module, dummy, out_path, opset, input_names, output_names)

    req = descriptor.size_requirements
    annotate_metadata(
        out_path,
        {
            "mangajanai.name": model_path.stem,
            "mangajanai.architecture": descriptor.architecture.name,
            "mangajanai.scale": str(descriptor.scale),
            "mangajanai.input_channels": str(descriptor.input_channels),
            "mangajanai.output_channels": str(descriptor.output_channels),
            "mangajanai.size_minimum": str(req.minimum),
            "mangajanai.size_multiple_of": str(req.multiple_of),
            "mangajanai.size_square": "1" if req.square else "0",
            "mangajanai.target_height": str(infer_target_height(model_path.stem) or 0),
            "mangajanai.source_pth_sha256": sha256_of(model_path),
        },
    )

    check_onnx(out_path)
    graph_inputs, graph_outputs, dynamic = describe_io(out_path)

    if graph_inputs != input_names or graph_outputs != output_names:
        raise SystemExit(
            f"{out_path.name}: exporter renamed the graph IO "
            f"(inputs={graph_inputs}, outputs={graph_outputs}); the Rust side "
            f"binds by name and would break"
        )
    if not {"height", "width"} <= set(dynamic):
        raise SystemExit(
            f"{out_path.name}: height/width were baked in as static dimensions "
            f"(symbolic dims found: {dynamic or 'none'}); tiled inference needs "
            f"them dynamic"
        )

    return ExportResult(
        name=model_path.stem,
        path=str(out_path),
        architecture=descriptor.architecture.name,
        scale=descriptor.scale,
        input_channels=descriptor.input_channels,
        output_channels=descriptor.output_channels,
        target_height=infer_target_height(model_path.stem),
        size_requirements={
            "minimum": req.minimum,
            "multiple_of": req.multiple_of,
            "square": req.square,
        },
        opset=opset,
        exporter=exporter,
        adapters=adapters,
        dynamic_axes=dynamic,
        input_names=graph_inputs,
        output_names=graph_outputs,
        source_pth=str(model_path),
        source_pth_sha256=sha256_of(model_path),
        onnx_sha256=sha256_of(out_path),
        torch_version=torch.__version__,
        spandrel_version=spandrel.__version__,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", nargs="*", type=Path, help="MangaJaNai .pth file(s)")
    parser.add_argument(
        "--all-in-dir",
        type=Path,
        help="export every .pth in this directory",
    )
    parser.add_argument(
        "--out-dir", type=Path, default=Path("models"), help="destination directory"
    )
    parser.add_argument("--opset", type=int, default=DEFAULT_OPSET)
    parser.add_argument(
        "--dummy-size",
        type=int,
        default=256,
        help="height/width of the tracing input; does not constrain inference",
    )
    parser.add_argument(
        "--legacy-exporter",
        action="store_true",
        help="skip the dynamo exporter and use the TorchScript path directly",
    )
    parser.add_argument("--device", choices=["cpu", "cuda", "mps"], default="cpu")
    args = parser.parse_args()

    paths: list[Path] = list(args.model)
    if args.all_in_dir:
        paths.extend(sorted(args.all_in_dir.glob("*.pth")))
    if not paths:
        parser.error("no models given; pass paths or --all-in-dir")

    results: list[ExportResult] = []
    failures: list[tuple[Path, str]] = []
    for path in paths:
        print(f"==> {path.name}")
        try:
            result = export_one(
                path, args.out_dir, args.opset, args.dummy_size, args.legacy_exporter, args.device
            )
        except Exception as exc:  # keep going; report at the end
            traceback.print_exc()
            failures.append((path, str(exc)))
            continue
        results.append(result)
        print(
            f"    {result.architecture} x{result.scale} "
            f"{result.input_channels}ch -> {result.path} "
            f"(via {result.exporter}, opset {result.opset})"
        )

    if results:
        sidecar = args.out_dir / "exported.json"
        sidecar.write_text(
            json.dumps([dataclasses.asdict(r) for r in results], indent=2) + "\n"
        )
        print(f"\nwrote {sidecar} ({len(results)} model(s))")

    if failures:
        print(f"\n{len(failures)} model(s) failed:", file=sys.stderr)
        for path, err in failures:
            print(f"  {path.name}: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
