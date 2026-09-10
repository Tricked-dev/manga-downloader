#!/usr/bin/env python3
"""Benchmark PyTorch/Spandrel, ONNX Runtime Python and ONNX Runtime Rust.

Measures the three implementations on the same pages and checks that they
still agree, because a speed number for an implementation that has drifted
numerically is worthless. Output error is reported alongside every timing.

Reported per backend and page size:

* model startup time (loading and preparing the graph)
* page inference time and total page processing time
* tiles per second, where the backend tiles
* CPU time (user + system) and peak resident memory
* maximum absolute difference against the PyTorch baseline, in 8-bit units

VRAM is only reported when a GPU execution provider is actually in use; it is
left null rather than guessed.

    python tools/benchmark.py \\
        --pth  models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth \\
        --onnx models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx \\
        --binary target/release/mangajanai-rs \\
        --heights 1200 1600 --repeats 1
"""

from __future__ import annotations

import argparse
import json
import os
import resource
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

# A manga page is roughly 2:3. Widths are derived so the aspect stays realistic
# across the height buckets MangaJaNai targets.
PAGE_ASPECT = 0.7


@dataclass
class Measurement:
    backend: str
    page: str
    width: int
    height: int
    startup_seconds: float
    inference_seconds: float
    total_seconds: float
    cpu_seconds: float
    peak_rss_mb: float
    tiles: int | None = None
    tiles_per_second: float | None = None
    vram_mb: float | None = None
    max_u8_error_vs_torch: int | None = None
    # For tiled runs only: difference against the same binary run whole-page.
    # Separated from the figure above because tiling legitimately changes the
    # result -- neighbouring tiles see different context -- so folding it into
    # "error against the reference implementation" conflates two things.
    max_u8_error_vs_untiled: int | None = None
    notes: list[str] = field(default_factory=list)


def synthetic_page(width: int, height: int, seed: int = 0) -> Image.Image:
    """A page-like image: screentone, hairlines and a gradient.

    Real scans cannot be committed, and flat noise would misrepresent the
    workload -- an upscaler's cost is roughly content-independent but its
    output error is not.
    """
    rng = np.random.default_rng(seed)
    yy, xx = np.mgrid[0:height, 0:width].astype(np.float64)

    # Halftone field over the lower half.
    pitch = 5.0
    angle = np.deg2rad(45.0)
    xr = xx * np.cos(angle) + yy * np.sin(angle)
    yr = -xx * np.sin(angle) + yy * np.cos(angle)
    radial = np.hypot((xr % pitch) - pitch / 2, (yr % pitch) - pitch / 2)
    page = np.where(radial < pitch * np.sqrt(0.4 / np.pi), 0.0, 255.0)
    page[: height // 2] = 255.0

    # Gradient band.
    band = slice(height // 2 - height // 8, height // 2)
    page[band] = np.linspace(0, 255, width)[None, :]

    # Hairlines and panel borders.
    for offset in range(0, height, max(height // 12, 1)):
        page[offset : offset + 1] = 0.0
    page[:, :3] = 0.0
    page[:, -3:] = 0.0

    # A little grain, so the model has something to do everywhere.
    page = np.clip(page + rng.normal(0, 2.0, page.shape), 0, 255)
    return Image.fromarray(page.astype(np.uint8), mode="L").convert("RGB")


def run_measured(command: list[str]) -> tuple[float, float, float, str]:
    """Run a command and return (wall, cpu, peak RSS MB, combined output).

    CPU time comes from ``os.wait4``, which reports the rusage of this
    specific child; ``getrusage(RUSAGE_CHILDREN)`` would give a high-water
    mark across every child ever reaped, so differencing two samples
    attributes the largest figure seen so far to every backend.

    Peak memory does *not* come from rusage. ``posix_spawn`` clones with
    ``CLONE_VM``, so the child shares the parent's address space until
    ``exec`` and its ``ru_maxrss`` inherits the parent's peak -- with PyTorch
    loaded in this process that is several gigabytes, and every backend gets
    attributed the same figure. Instead we sample the child's own
    ``/proc/<pid>/status`` ``VmHWM``, which lives in the ``mm_struct`` and is
    therefore replaced at ``exec``.
    """
    read_fd, write_fd = os.pipe()
    started = time.perf_counter()
    pid = os.posix_spawn(
        command[0],
        command,
        os.environ,
        file_actions=[
            (os.POSIX_SPAWN_DUP2, write_fd, 1),
            (os.POSIX_SPAWN_DUP2, write_fd, 2),
        ],
    )
    os.close(write_fd)

    # Sample the child's own high-water mark while it runs. Reading in a
    # thread keeps the pipe drained so the child never blocks on a full pipe.
    peak_kib = 0

    def sample_peak() -> None:
        nonlocal peak_kib
        while True:
            try:
                with open(f"/proc/{pid}/status", "r") as status_file:
                    for line in status_file:
                        if line.startswith("VmHWM:"):
                            peak_kib = max(peak_kib, int(line.split()[1]))
                            break
            except (OSError, ValueError):
                return
            time.sleep(0.05)

    sampler = threading.Thread(target=sample_peak, daemon=True)
    sampler.start()

    chunks = []
    with os.fdopen(read_fd, "rb") as stream:
        while True:
            chunk = stream.read(65536)
            if not chunk:
                break
            chunks.append(chunk)

    _, status, usage = os.wait4(pid, 0)
    wall = time.perf_counter() - started
    sampler.join(timeout=1.0)
    output = b"".join(chunks).decode(errors="replace")

    if status != 0:
        sys.stderr.write(output)
        raise SystemExit(f"benchmark command failed ({status}): {' '.join(command)}")

    cpu = usage.ru_utime + usage.ru_stime
    return wall, cpu, peak_kib / 1024.0, output


def to_u8_array(path: Path) -> np.ndarray:
    with Image.open(path) as handle:
        return np.asarray(handle.convert("RGB"), dtype=np.uint8)


def max_u8_error(left: np.ndarray, right: np.ndarray) -> int | None:
    if left.shape != right.shape:
        return None
    return int(np.abs(left.astype(np.int16) - right.astype(np.int16)).max())


def bench_torch(pth: Path, page: Image.Image, out_path: Path) -> Measurement:
    import torch
    from spandrel import ModelLoader

    started = time.perf_counter()
    descriptor = ModelLoader().load_from_file(str(pth))
    module = descriptor.model
    module.eval()
    module.to(torch.device("cpu")).to(torch.float32)
    startup = time.perf_counter() - started

    array = np.asarray(page, dtype=np.uint8).astype(np.float32) / 255.0
    tensor = torch.from_numpy(array.transpose(2, 0, 1)[None, ...].copy())

    total_started = time.perf_counter()
    inference_started = time.perf_counter()
    with torch.no_grad():
        output = descriptor(tensor)
    inference = time.perf_counter() - inference_started

    quantised = np.floor(
        np.clip(output.numpy()[0].transpose(1, 2, 0), 0, 1) * 255.0 + 0.5
    ).astype(np.uint8)
    Image.fromarray(quantised, mode="RGB").save(out_path)
    total = time.perf_counter() - total_started

    usage = resource.getrusage(resource.RUSAGE_SELF)
    return Measurement(
        backend="pytorch+spandrel",
        page="",
        width=page.width,
        height=page.height,
        startup_seconds=startup,
        inference_seconds=inference,
        total_seconds=total,
        cpu_seconds=usage.ru_utime + usage.ru_stime,
        peak_rss_mb=usage.ru_maxrss / 1024.0,
        notes=["whole page, no tiling"],
    )


def bench_subprocess(
    backend: str, command: list[str], page: Image.Image, notes: list[str]
) -> Measurement:
    total, cpu, peak_rss, output = run_measured(command)

    # Both runners log their own inference time; prefer it over wall clock so
    # process startup and image IO do not pollute the inference figure.
    inference = parse_reported_seconds(output)
    return Measurement(
        backend=backend,
        page="",
        width=page.width,
        height=page.height,
        startup_seconds=0.0,
        inference_seconds=inference if inference is not None else total,
        total_seconds=total,
        cpu_seconds=cpu,
        peak_rss_mb=peak_rss,
        notes=notes + ([] if inference is not None else ["inference time not reported"]),
    )


def parse_reported_seconds(text: str) -> float | None:
    """Pull an inference time out of either runner's log line."""
    for token in text.replace("\x1b", " ").split():
        if token.startswith("inference_ms="):
            try:
                return int(token.split("=", 1)[1]) / 1000.0
            except ValueError:
                return None
    marker = "inference "
    if marker in text:
        tail = text.split(marker, 1)[1]
        number = tail.split("s")[0]
        try:
            return float(number)
        except ValueError:
            return None
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pth", type=Path, help="baseline PyTorch checkpoint")
    parser.add_argument("--onnx", required=True, type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/mangajanai-rs"))
    parser.add_argument(
        "--heights",
        type=int,
        nargs="+",
        default=[1200, 1600, 2048],
        help="page heights to benchmark",
    )
    parser.add_argument(
        "--large-scan",
        type=int,
        default=0,
        help="also benchmark a page of this height, representing a large scan",
    )
    parser.add_argument("--tile-size", type=int, default=512)
    parser.add_argument("--overlap", type=int, default=32)
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--work-dir", type=Path, default=Path("artifacts/benchmark"))
    parser.add_argument(
        "--skip-torch",
        action="store_true",
        help="skip the PyTorch baseline (much the slowest of the three)",
    )
    args = parser.parse_args()

    args.work_dir.mkdir(parents=True, exist_ok=True)
    heights = list(args.heights)
    if args.large_scan:
        heights.append(args.large_scan)

    measurements: list[Measurement] = []

    for height in heights:
        width = int(round(height * PAGE_ASPECT))
        label = f"{height}p"
        page = synthetic_page(width, height)
        page_path = args.work_dir / f"page_{label}.png"
        page.save(page_path)

        torch_output: np.ndarray | None = None
        print(f"\n=== {label} ({width}x{height}) ===")

        if args.pth is not None and not args.skip_torch:
            for _ in range(args.repeats):
                out = args.work_dir / f"{label}.torch.png"
                measurement = bench_torch(args.pth, page, out)
                measurement.page = label
                measurements.append(measurement)
                print(
                    f"  pytorch+spandrel  startup={measurement.startup_seconds:6.2f}s "
                    f"inference={measurement.inference_seconds:7.2f}s "
                    f"rss={measurement.peak_rss_mb:7.0f}MB"
                )
            torch_output = to_u8_array(args.work_dir / f"{label}.torch.png")

        for _ in range(args.repeats):
            out = args.work_dir / f"{label}.ortpy.png"
            measurement = bench_subprocess(
                "onnxruntime-python",
                [
                    sys.executable,
                    "tools/onnx_upscale.py",
                    "--model",
                    str(args.onnx),
                    str(page_path),
                    str(out),
                ],
                page,
                notes=["whole page, no tiling"],
            )
            measurement.page = label
            if torch_output is not None:
                measurement.max_u8_error_vs_torch = max_u8_error(
                    torch_output, to_u8_array(out)
                )
            measurements.append(measurement)
            print(
                f"  onnxruntime-py    inference={measurement.inference_seconds:7.2f}s "
                f"total={measurement.total_seconds:7.2f}s "
                f"rss={measurement.peak_rss_mb:7.0f}MB "
                f"err_vs_torch={measurement.max_u8_error_vs_torch}"
            )

        untiled_output: np.ndarray | None = None
        for tiling, note in ((0, "whole page, no tiling"), (args.tile_size, "tiled")):
            for _ in range(args.repeats):
                suffix = "untiled" if tiling == 0 else f"tile{tiling}"
                out = args.work_dir / f"{label}.rust-{suffix}.png"
                measurement = bench_subprocess(
                    f"onnxruntime-rust ({suffix})",
                    [
                        str(args.binary),
                        "upscale",
                        "--model",
                        str(args.onnx),
                        "--device",
                        args.device,
                        "--tile-size",
                        str(tiling),
                        "--overlap",
                        str(args.overlap),
                        str(page_path),
                        str(out),
                    ],
                    page,
                    notes=[note, f"device={args.device}"],
                )
                measurement.page = label
                if tiling:
                    columns = tile_count(width, tiling, args.overlap)
                    rows = tile_count(height, tiling, args.overlap)
                    measurement.tiles = columns * rows
                    if measurement.inference_seconds > 0:
                        measurement.tiles_per_second = (
                            measurement.tiles / measurement.inference_seconds
                        )
                produced = to_u8_array(out)
                if torch_output is not None:
                    measurement.max_u8_error_vs_torch = max_u8_error(
                        torch_output, produced
                    )
                if tiling == 0:
                    untiled_output = produced
                elif untiled_output is not None:
                    measurement.max_u8_error_vs_untiled = max_u8_error(
                        untiled_output, produced
                    )
                measurements.append(measurement)
                tiles_text = (
                    f"tiles={measurement.tiles} "
                    f"({measurement.tiles_per_second:.2f}/s)"
                    if measurement.tiles
                    else ""
                )
                error_text = f"err_vs_torch={measurement.max_u8_error_vs_torch}"
                if measurement.max_u8_error_vs_untiled is not None:
                    error_text += f" err_vs_untiled={measurement.max_u8_error_vs_untiled}"
                print(
                    f"  rust {suffix:<9} inference={measurement.inference_seconds:7.2f}s "
                    f"total={measurement.total_seconds:7.2f}s "
                    f"rss={measurement.peak_rss_mb:7.0f}MB "
                    f"{error_text} {tiles_text}"
                )

    report = args.work_dir / "benchmark.json"
    report.write_text(
        json.dumps([m.__dict__ for m in measurements], indent=2) + "\n"
    )
    print(f"\nreport: {report}")
    return 0


def tile_count(length: int, tile: int, overlap: int) -> int:
    """Mirror of `plan_axis` in crates/manga-core/src/tiling.rs."""
    if length <= tile:
        return 1
    step = tile - overlap
    starts: list[int] = []
    start = 0
    while True:
        if start + tile >= length:
            final = length - tile
            if not starts or starts[-1] != final:
                starts.append(final)
            break
        starts.append(start)
        start += step
    return len(starts)


if __name__ == "__main__":
    raise SystemExit(main())
