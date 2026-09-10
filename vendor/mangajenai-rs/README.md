# mangajanai-rs

A Rust inference pipeline for [MangaJaNai](https://github.com/the-database/MangaJaNai)
manga upscaling models, running on ONNX Runtime.

Converting `.pth` weights to ONNX is a **one-time step** that uses Python,
PyTorch and Spandrel. Everything after that — decoding, tiling, inference,
blending, CBZ handling — is Rust with no Python or PyTorch dependency.

> Status: the v0.1 pipeline works end to end. Exported models are numerically
> equivalent to PyTorch, the Rust output is bit-exact against Python ONNX
> Runtime, tiling is seam-free, model selection matches upstream, and CBZ
> conversion preserves ordering and metadata. `nix flake check` is green.
> See [PROGRESS.md](PROGRESS.md) for the measurements behind each of those.

## Why

MangaJaNai ships a separate model per source resolution because halftone
frequency depends on the resolution of the original scan. The existing
converter is a .NET GUI wrapping chaiNNer and PyTorch. This project aims for the
same output from a single native binary, and then — once parity is proven — to
improve how small Japanese text and furigana survive upscaling.

## Getting started

Everything is driven by the nix flake:

```sh
nix develop          # Rust toolchain + PyTorch/Spandrel conversion tools
nix build            # build the CLI
nix flake check      # build, tests, clippy -D warnings, rustfmt
```

### One-time model conversion

```sh
# Fetch MangaJaNai_V1_ModelsOnly.zip from the upstream releases, unzip into models/
python tools/export_onnx.py models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth --out-dir models
python tools/export_onnx.py --all-in-dir models --out-dir models   # or all at once
```

### Verifying an exported model

Never ship a model that has not been through the parity gate:

```sh
python tools/make_fixtures.py            # deterministic regression corpus
python tools/verify_onnx.py \
    --pth  models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth \
    --onnx models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx \
    --inputs fixtures/general fixtures/furigana fixtures/screentones \
    --report artifacts/parity
```

This reports max/mean absolute error, RMSE and 8-bit error per input, writes
amplified difference images, and exits non-zero if the graph has drifted from
the PyTorch original.

It does not gate on a fixed tolerance alone. A deep residual network can be
chaotically ill-conditioned on some inputs — on a perfectly smooth gradient,
nudging the input by one float32 ULP moves this model's own output further
than switching runtimes does. Demanding 1-LSB agreement there asks for
precision that does not exist in any implementation. So a case that exceeds
the tolerance has its conditioning measured, and passes only if the export
stays within that floor. A case that exceeds both still fails and is named.

### Building the model manifest

Automatic model selection reads `models/models.json`:

```sh
python tools/make_manifest.py --models-dir models
```

The source-height bands it writes are transcribed from
MangaJaNaiConverterGui's default workflow, not derived from the model names --
they are not the same thing. The 1600p model covers source heights 1551-1760.

### Choosing hardware

```sh
mangajanai-rs upscale --device auto   ...   # prefer a GPU backend, log the choice
mangajanai-rs upscale --device openvino --openvino-device-type GPU ...
```

`auto` tries CUDA, DirectML, CoreML then OpenVINO and falls back to CPU,
logging which it picked. Naming a backend explicitly **never** falls back — if
it cannot be used, that is an error.

Two separate things have to line up, and the error message says which failed:
this binary must be built with the bindings (cargo features `cuda`,
`directml`, `coreml`, `openvino`; the nix build enables `openvino`), and the
linked ONNX Runtime must have been built with the provider.

Note that OpenVINO runs on Intel CPUs as well as Intel GPUs and picks for
itself when unconstrained, so "OpenVINO is active" does not mean the GPU is in
use — pass `--openvino-device-type GPU` if that is what you want.

### Upscaling

```sh
# explicit model
mangajanai-rs upscale --model models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx \
    page.png page-upscaled.png

# pick the model from the page height
mangajanai-rs upscale --model auto page.png page-upscaled.png

# tiling (the default is 512px tiles with 32px overlap; 0 disables it)
mangajanai-rs upscale --tile-size 512 --overlap 32 page.png out.png
```

### Converting a volume

Works on a single image, a directory tree, or a `.cbz`/`.zip`:

```sh
mangajanai-rs convert --model auto manga.cbz manga-upscaled.cbz
```

Archive order, entry names and directory structure are preserved, and
non-image entries such as `ComicInfo.xml` are copied through untouched.
Archives are streamed one entry at a time, so peak memory is one page rather
than a whole volume, and each processed page is encoded once with those bytes
going straight into the output archive.

## Layout

```
crates/manga-core/   image, tensor, tiling, blending, model, device, archive
crates/manga-cli/    the mangajanai-rs binary
tools/               one-time .pth -> .onnx conversion and verification (Python)
models/              exported .onnx graphs and their manifest (weights gitignored)
fixtures/            generated regression corpus
docs/                reference semantics extracted from Spandrel/MangaJaNai
nix/                 package definitions used by flake.nix
```

## Measuring tiny-text quality

`fixtures/groundtruth/` holds matched pairs — each scene rendered natively at
1x and 4x — so the 4x render is genuine high-resolution ground truth rather
than a guess:

```sh
python tools/text_quality.py --model models/4x_MangaJaNai_1600p_V1_ESRGAN_70k.onnx
```

This reports stroke survival, stroke darkness, background cleanliness and edge
energy against that ground truth, with a Lanczos baseline alongside, and
writes reference/model/baseline triptychs.

The baseline matters. On this corpus **Lanczos wins PSNR on every scene while
destroying 97% of the smallest strokes**; the model keeps 9.5x more of them.
Optimising tiny-text quality against PSNR would make it worse.

## Non-goals

No neural network architectures are reimplemented in Rust, and neither PyTorch
nor Spandrel is ported. Model graphs are produced once by the Python tooling and
consumed as ONNX.

## Licence

MIT.
