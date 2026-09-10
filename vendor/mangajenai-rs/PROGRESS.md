# Progress

Living status for the v0.1 milestone. Updated as phases land.

Legend: **done** / **in progress** / **blocked** / *not started*

| Phase | Status |
| --- | --- |
| 0. Toolchain (nix flake, workspace) | **done** |
| 1. Export MangaJaNai to ONNX + parity gate | **done** |
| 2. Minimal Rust ONNX inference | **done** |
| 3. Tiled inference | **done** |
| 4. Model selection | **done** |
| 5. CBZ and batch support | **done** |
| 6. Execution providers / GPU | **done** |
| 7. Performance benchmarking | **done** |
| 8. Furigana / scan-quality work | **measurement in place** |
| 9. Additional architectures | **done for the tested set** |

## Phase 0 — toolchain

Flake exposes `packages.mangajanai-rs` (crane), `packages.tools-python`
(PyTorch + Spandrel for the one-time conversion), `checks` (build, tests,
clippy `-D warnings`, fmt) and a `devShells.default` carrying both toolchains.

ONNX Runtime comes from nixpkgs (1.27.1) via ort's `pkg-config` feature rather
than ort's bundled-binary download, which cannot work in the nix sandbox.
`ORT_SKIP_DOWNLOAD=1` turns a discovery failure into a build error instead of a
silent fallback.

## Phase 1 — export and parity

Done, and the phase 2 gate is satisfied.

- `tools/export_onnx.py` — Spandrel loads the `.pth`, the underlying
  `torch.nn.Module` is exported FP32 via the **dynamo** exporter at opset 18
  with `torch.export.Dim` dynamic height/width, IO named `input`/`output`,
  followed by `onnx.checker.check_model(full_check=True)`. Writes a
  `models/exported.json` sidecar (scale, channels, size requirements, source
  hashes) that phase 4 will build its manifest from.
- `tools/verify_onnx.py` — compares PyTorch against ONNX Runtime and reports
  max/mean absolute error, RMSE, 8-bit error and a saved difference image.
- `tools/make_fixtures.py` — generates the 37-image deterministic regression
  corpus (screentones, hairlines, gradients, furigana, vertical text, tone
  behind text, JPEG-damaged and blurred text, plus tile-seam images for
  phase 3).

### Result

Full 35-case sweep (31 fixture crops plus random shapes) on
`4x_MangaJaNai_1600p_V1_ESRGAN_70k`: **34 of 35 cases at 1 LSB or better**,
worst well-conditioned max absolute error **1.9e-05**. Screentone at every
pitch and angle, line art, dialogue, furigana, vertical text, JPEG-damaged and
blurred text all sit at 1 LSB.

The 35th case is `gradient_linear` — a perfectly smooth 0→255 ramp — at 0.551
max, 141 in 8-bit. That is **not** an export defect:

| measurement | max abs | max 8-bit |
| --- | --- | --- |
| `torch(x)` vs `torch(x)` twice | 0.0 | 0 |
| `torch(x)` vs `torch(x + 1 ULP)` | **0.593** | **151** |
| `torch(x)` vs `onnx(x)` | 0.551 | 141 |

A single float32 ULP of input change moves PyTorch's own output further than
switching runtimes does. Twenty-three RRDB blocks on a featureless input is
chaotic; the model has no stable answer there and no implementation could
agree more closely.

So the gate was wrong, not the export. `verify_onnx.py` now measures a case's
**conditioning** when it exceeds the tolerance — re-running the module on the
input nudged by one ULP — and passes it only if the export error stays within
that floor. A case that exceeds both still fails and is named. Full reasoning
in [`docs/REFERENCE-SEMANTICS.md`](docs/REFERENCE-SEMANTICS.md).

### Things found by checking rather than assuming

- MangaJaNai V1 is **3-channel RGB**, not greyscale, despite being black and
  white artwork.
- Spandrel's `__call__` **clamps to `[0, 1]`** — measured excursions reach 0.514
  outside the range, so this is load-bearing, not cosmetic. The verifier now
  asserts `descriptor(x) == clamp(module(x), 0, 1)` exactly.
- ort 2.0.0-rc.13 has **no `load-dynamic` feature** any more; system discovery
  is via `pkg-config` / `ORT_LIB_LOCATION`.

Full details in [`docs/REFERENCE-SEMANTICS.md`](docs/REFERENCE-SEMANTICS.md).

## Phase 2 — minimal Rust inference

The whole-image path is implemented and **bit-exact against the Python ONNX
Runtime reference** on every fixture checked -- zero differing samples across
six inputs covering flat fields, line art, gradients, screentone and Japanese
text, at square and non-square sizes:

| fixture | output | max diff |
| --- | --- | --- |
| `flat_white` | 512x512 | 0 |
| `lineart_strokes` | 1024x1024 | 0 |
| `gradient_linear` | 1024x1024 | 0 |
| `ruby_base20` | 1280x384 | 0 |
| `small_kanji` | 1024x384 | 0 |
| `dots_p3_a45` | 1024x1024 | 0 |

- `manga-core::tensor` — HWC u8 <-> NCHW f32, and the quantisation rule
  (`floor(clamp(x, 0, 1) * 255 + 0.5)`, chosen to match the reference on .5
  ties rather than relying on `f32::round`).
- `manga-core::padding` — port of Spandrel's size requirements and its
  reflect-then-replicate right/bottom padding.
- `manga-core::model` — ONNX Runtime session. Model properties are read from
  the graph's `metadata_props`, which `tools/export_onnx.py` now writes, so a
  `.onnx` file is self-describing (scale, channels, size requirements, target
  height) and needs no sidecar.
- `manga-core::image` — decode/encode and the tensor boundary.
- `mangajanai-rs upscale --model M in out`.

Supporting tools: `tools/onnx_upscale.py` (Python reference implementation of
the identical pipeline) and `tools/check_rust_parity.py` (runs both and
compares pixel for pixel).

18 unit tests cover quantisation ties, channel interleaving, round-trips and
every branch of the padding rule.

### Note on `nix build`

Building the CLI through the flake works end to end. Getting there needed one
non-obvious fix: `ort`'s **default features include `download-binaries`**,
which pulls in `ureq` -> `native-tls` -> `openssl-sys` and fails in the nix
sandbox. Defaults are disabled and `api-27` pinned to match nixpkgs'
onnxruntime 1.27.1.

## Phase 3 — tiled inference

Implemented and validated on real ESRGAN output.

### Seam measurement

480x360 -> 1920x1440 through 256px tiles with 32px overlap, so content
straddles a real tile boundary. Boundary columns and rows are scored against
the distribution of every other column and row using a median/MAD z-score:

| fixture | worst vertical z | worst horizontal z | vs whole-page render |
| --- | --- | --- | --- |
| `seam_screentone_256` | **+0.56** | **+0.42** | max 49, mean 0.07, 3.3% differ |
| `seam_text_256` | **+1.06** | **+1.11** | max 31, mean 0.004, 0.3% differ |

Against a limit of 6.0. Screentone is the worst case, being a regular pattern
that makes any discontinuity obvious. A z near zero means the tile boundaries
are statistically indistinguishable from every other column and row — there is
nothing there to see, and a crop spanning both seam positions confirms it by
eye.

Tiling does change the result, which is expected: neighbouring tiles see
different context. What matters is that the difference is spread across the
image rather than concentrated at the joins.

- `manga-core::tiling` — tile layout, and reassembly through a **sliding
  accumulation band**. Memory is bounded by the tile height rather than the
  page height: rows no future tile can touch are quantised into the output and
  dropped. Tiles are read straight out of the source buffer, so no cropped
  sub-image is ever materialised.
- `manga-core::blending` — cross-fade ramps. Total weight is kept as **two 1-D
  vectors instead of a page-sized 2-D map**: the tile grid is a full Cartesian
  product, so `total(x, y)` factorises into `totals_x(x) * totals_y(y)`.
- The final tile on each axis is shifted back to sit flush with the edge rather
  than truncated, so the model never sees a narrow sliver whose context differs
  from every other tile. That makes its overlap with the previous tile larger
  than nominal, which the ramps handle by clamping to the *actual* overlap.

The seam logic is tested without ONNX by driving it with an exact
nearest-neighbour upscaler: every tile then agrees on the pixels it shares, so
the reassembled page must come back bit-identical to a direct upscale. Any
indexing, ramp or normalisation error breaks that equality. This runs over
awkward geometry -- odd sizes, clamped final tiles, overlap larger than half a
tile, zero overlap.

`fixtures/tiling/` holds twelve seam images at two geometries (a 256px set that
is cheap to run, and a 512px set matching the default tile size), each placing
screentone, hairlines, gradients, dialogue or furigana across the first tile
boundary. `tools/check_seams.py` scores tile boundaries against the
distribution of all other column/row differences using a median/MAD z-score.

## Phase 4 — model selection

`manga-core::manifest` plus `tools/make_manifest.py`, and `--model auto`.

The selection rule is transcribed from MangaJaNaiConverterGui's
`default_cli_configuration.json` and `should_chain_activate_for_image`, not
invented. Two things would have been wrong if guessed:

- The source-height bands are **not centred on the model names**. The 1600p
  model covers 1551-1760 and the 1920p model covers 1761-1984.
- A target scale of exactly 2 matches both the 2x and 4x chain, and upstream
  lists the 2x chain first, so **scale 2 resolves to the 2x model**.

An unserved height is an error naming what is available, never a silent
fallback to the nearest model.

All 14 MangaJaNai V1 models are exported and in the manifest. `--model auto`
resolves correctly across every band, checked at the edges where an
off-by-one would hide:

| source height | selected |
| --- | --- |
| 1200, 1250 | `4x_MangaJaNai_1200p` |
| **1251** | `4x_MangaJaNai_1300p` |
| 1400 | `4x_MangaJaNai_1400p` |
| 1550 | `4x_MangaJaNai_1500p` |
| **1551**, 1600, 1760 | `4x_MangaJaNai_1600p` |
| **1761** | `4x_MangaJaNai_1920p` |

## Phase 5 — archives and batches

`manga-core::archive` plus the `convert` subcommand, handling a single image, a
directory tree, or a `.zip`/`.cbz`.

- Archives are **streamed one entry at a time**. A 200-page volume upscaled 4x
  is several gigabytes of output; reading everything up front and writing at
  the end would need all of it resident. Peak memory is one page.
- Order, names and directory prefixes are preserved exactly, because that is
  what page order in a reader depends on.
- Non-image entries (`ComicInfo.xml` and friends) are copied through byte for
  byte, never decoded.
- Each processed page is encoded **once**, and those bytes go straight into the
  archive with `CompressionMethod::Stored` -- PNG payloads are already
  compressed, so deflating them again costs time for nothing. Non-image
  entries still get deflate, where it does help.

### Verified end to end

A CBZ with three pages written deliberately out of order (003, 001, 002) plus
a `ComicInfo.xml`:

```
  in                 : ['ComicInfo.xml', 'chapter 1/003.png', 'chapter 1/001.png', 'chapter 1/002.png']
  out                : ['ComicInfo.xml', 'chapter 1/003.png', 'chapter 1/001.png', 'chapter 1/002.png']
  ComicInfo preserved: True
  page order kept    : True
  upscaled 4x        : True
  images STORED      : True
  xml DEFLATED       : True
```

### On parallelism

The brief asked to parallelise decode/encode. Measured against this pipeline it
is not worth it: inference is 30-90 s per page on CPU against milliseconds for
PNG decode and encode, so overlapping them would hide well under 1% of the
runtime while adding a second page's worth of memory. Pages are processed
serially for now. This should be revisited once phase 6 has a GPU provider
working and phase 7 has numbers, because the balance changes completely when a
page takes 200 ms.

## Phase 6 — execution providers

`manga-core::device`, and `--device auto|cpu|cuda|directml|coreml|openvino`.

Two conditions have to hold to use a provider, and conflating them produces
confusing errors, so they are reported separately:

1. **This binary was compiled with the bindings.** `ort` gates each provider
   behind a cargo feature, so `manga-core` re-exports them as its own features,
   all off by default. The nix build enables `openvino`.
2. **The linked ONNX Runtime was built with it**, which is only knowable at
   runtime.

`Device` variants exist unconditionally, so command line parsing and error
messages do not change shape with build flags. `auto` tries CUDA, DirectML,
CoreML, then OpenVINO, and logs what it chose; OpenVINO is last of the
accelerators because it also runs on Intel CPUs and would otherwise shadow a
real GPU. **An explicitly named accelerator never falls back to CPU** -- it is
an error, naming which of the two conditions failed.

Enabling `ort/openvino` is safe with the nix setup: `ort-sys`' build script
tries `pkg-config` first and returns before any provider-specific linking, so
the feature only affects the prebuilt-download path we do not use.

### Verified

```
--device cuda      Error: this build of mangajanai-rs was compiled without cuda
                   support. Rebuild with `--features cuda`, or pick one of:
                   cpu, openvino.
--device openvino  [OpenVINO-EP] Running graph ...   (runs)
--device auto      selects openvino, and logs the choice
```

### Hardware note

This development machine is WSL2 with an Intel iGPU and no CUDA (`/dev/dxg`
present, no `libcuda.so`). nixpkgs' onnxruntime does ship
`libonnxruntime_providers_openvino.so`, so OpenVINO is the realistic
acceleration path here; CUDA and DirectML are implemented but untestable
locally.

There is also **no `/dev/dri`** on this machine, so OpenVINO is running on the
Intel *CPU*, not the iGPU — WSL2 exposes only D3D12 through `/dev/dxg`, which
Linux OpenVINO cannot use. This is worth stating plainly because OpenVINO
targets Intel CPUs and GPUs alike and chooses for itself when unconstrained,
so "OpenVINO is active" never implies the GPU is in use. `--openvino-device-type
GPU` makes the intent explicit and turns a missing GPU into an error.

## Phase 7 — benchmarking

`tools/benchmark.py` compares PyTorch/Spandrel, ONNX Runtime Python and ONNX
Runtime Rust (tiled and untiled) on synthetic pages at the MangaJaNai height
buckets, reporting startup time, inference time, total time, tiles/sec, CPU
time and peak RSS.

Every timing is reported next to the **maximum 8-bit difference against the
PyTorch baseline**, because a speed number for an implementation that has
drifted numerically is worthless. VRAM is left null rather than guessed when no
GPU provider is in use.

### Results (CPU only, this machine)

Synthetic pages, 128px tiles so the tiled path is genuinely exercised:

| page | backend | inference | peak RSS | err vs torch |
| --- | --- | --- | --- | --- |
| 240p | pytorch+spandrel | 6.01s | 999 MB | — |
| | onnxruntime-python | 5.93s | 794 MB | 1 |
| | onnxruntime-rust untiled | 9.10s | 749 MB | 1 |
| | onnxruntime-rust tiled | 14.53s | **410 MB** | 238 (see below) |
| 360p | pytorch+spandrel | 21.92s | 1586 MB | — |
| | onnxruntime-python | 18.41s | 1493 MB | 1 |
| | onnxruntime-rust untiled | 20.14s | 1066 MB | 1 |
| | onnxruntime-rust tiled | 43.32s | **427 MB** | 239 |
| 600p | pytorch+spandrel | 66.99s | 3489 MB | — |
| | onnxruntime-python | 53.72s | 3362 MB | 1 |
| | onnxruntime-rust untiled | 55.56s | 2463 MB | 1 |
| | onnxruntime-rust tiled | 87.29s | **448 MB** | 255 |

**The memory column is the real result.** Tiled peak RSS is flat at
410-448 MB across a 6x range of page area, while every whole-page backend
grows with it: PyTorch reaches 3.5 GB at 600p and ONNX Runtime 2.5-3.4 GB.
That is the sliding accumulation band doing its job — memory bounded by tile
height rather than page height. Extrapolating the untiled curve, a 2048p page
would need roughly 28 GB, which does not fit on this 15 GB machine; the tiled
path would still sit around 450 MB. Tiling is not an optimisation here, it is
what makes full-size pages possible at all.

ONNX Runtime is ~20% faster than PyTorch at 600p, and the Rust and Python ONNX
Runtime paths track each other closely, as they should — both drive the same
runtime, and the Rust side is only the surrounding pipeline. Tiling costs
~55% more wall time at 128px because overlapped regions are computed twice;
the shipped default is 512px, where the overhead is far smaller.

### On the tiled error column

238-255 against PyTorch looks alarming and is not a pipeline defect. The
difference is concentrated in the benchmark page's smooth gradient band — mean
row difference 15.5 there against ~0.1 elsewhere — which is precisely the
chaotically ill-conditioned content identified in phase 1, where a 1-ULP input
change already moves this model by 151 in 8-bit. Crops across the tile
boundary show no discontinuity: both renders turn the gradient into arbitrary
horizontal banding, and tiling merely selects a different arbitrary answer.
On real content the seam fixtures differ by 49 (screentone) and 31 (text).

Two measurement bugs were fixed to get these numbers, both of which produced
plausible-looking output:

- Peak memory was identical for all four backends. `posix_spawn` clones with
  `CLONE_VM`, so each child shared the parent's address space until `exec` and
  inherited its PyTorch-laden peak. Now sampled from the child's own
  `/proc/<pid>/status` `VmHWM`.
- The error column folded the legitimate effect of tiling into "error against
  the reference implementation". Tiled rows now also report against the same
  binary run whole-page.

## Phase 8 — tiny text: measurement first

No restoration algorithm is added and none is enabled. The v0.1 non-goals rule
out speculative restoration, and an improvement that cannot be measured is
exactly how this work goes wrong. What exists now is the ability to measure.

- `manga-core::text` locates 1-2px strokes with a two-pass chamfer distance
  transform: for a pixel of ink, the distance to the nearest non-ink pixel is
  about half the stroke width. Diagonal steps are weighted sqrt(2) so a
  diagonal hairline does not measure thicker than a horizontal one.
- `fixtures/groundtruth/` holds **matched pairs** — each scene rendered
  natively at 1x and 4x with every dimension, font size and stroke width
  scaled together. Because the corpus is synthetic the 4x render is genuine
  high-resolution ground truth, so what the model *loses* can be measured
  rather than guessed at from a reference-free sharpness score.
- `tools/text_quality.py` reports stroke survival, stroke darkness, background
  cleanliness and edge energy against that ground truth, **and measures a
  Lanczos baseline alongside**. Without the baseline there is no way to tell
  whether a learned upscaler is helping tiny text or merely changing it.

### Baseline results, and why PSNR would have misled us

`4x_MangaJaNai_1600p` against native 4x renders, with plain Lanczos as the
control:

| scene | PSNR (model / lanczos) | stroke survival | edge energy |
| --- | --- | --- | --- |
| `small_text` | 17.35 / **19.50** | **0.281** / 0.030 | **1.078** / 0.491 |
| `ruby` | 20.14 / **20.59** | **0.632** / 0.409 | **1.053** / 0.550 |
| `hairlines` | 14.97 / **15.96** | 0.791 / 0.794 | 0.993 / 1.017 |
| `text_on_tone` | 9.68 / **10.49** | 0.790 / 0.789 | 1.025 / 1.024 |

**Lanczos wins PSNR on every single scene** — while destroying 97% of the
smallest strokes (survival 0.030) and halving edge energy. The model keeps
9.5x more of that text. Optimising for PSNR would drive tiny-text quality in
exactly the wrong direction, and this is the concrete evidence for that, not a
general caution.

The triptychs make it obvious by eye: at the smallest size Lanczos renders
faint grey ghosts while the model keeps the strokes dark.

The absolute numbers are the actual phase 8 target. Even the model only
preserves **28% of the smallest strokes**, and pushes ruby strokes to level 92
where the ground truth is 33 — legible, but washed out and breaking up. That
is the thing to improve, and it can now be measured rather than argued about.

## Phase 9 — other architectures

All five non-ESRGAN test subjects export. Only SPAN needed an adapter; the
transformer architectures traced cleanly as written.

| architecture | ONNX | adapter |
| --- | --- | --- |
| SPAN | 1.7 MB | **reparameterisation fold** |
| DAT2 | 143 MB | none |
| FDAT-M | 19 MB | none |
| FDAT-M unshuffle | 19 MB | none |
| FDAT-XL | 97 MB | none |

SPAN did not fail — it **segfaulted**, reproducibly, with a null dereference
inside `libtorch_cpu.so`. `Conv3XC.forward` calls `update_params()` on every
eval pass, which assigns to `eval_conv.weight.data`, so each traced call
mutates a Parameter in place. Because it is a crash rather than an exception,
the existing dynamo-to-TorchScript fallback never ran.

The fix folds the recomputation: with frozen weights `update_params()` is
deterministic, so running it once and skipping it afterwards is
mathematically identical. `export_one` captures the module output before
applying any adapter and refuses to export if it moved — SPAN folds 20
modules with a difference of exactly **0.0**.

The adapter is provably inert for ESRGAN: re-exporting
`4x_MangaJaNai_1600p` afterwards records `adapters: []` and produces a
**bit-identical** ONNX file to the committed one, which also confirms the
dynamo export is deterministic.

The folded SPAN graph then passes the same parity gate as ESRGAN — worst max
absolute error **8.0e-06**, 1 LSB across six cases — so the adapter is not
merely making the export succeed, it is preserving the maths.

Detail in [`docs/PHASE9-ARCHITECTURES.md`](docs/PHASE9-ARCHITECTURES.md).
DAT2 and the FDAT variants are exports only: they have not been through the
parity gate, and their size requirements are untested.

## Definition of done — v0.1

| requirement | status |
| --- | --- |
| runs entirely through Rust after export | yes |
| uses ONNX Runtime | yes |
| selects the MangaJaNai model automatically | yes, matching upstream bands |
| processes large pages through tiles | yes |
| no visible tile seams | yes, boundary z <= 1.11 against a limit of 6 |
| preserves CBZ ordering | yes |
| GPU acceleration on the dev machine's provider | OpenVINO EP runs; see the hardware note |
| output matches the Spandrel FP32 result | yes, 1 LSB on every well-conditioned input |

`nix flake check`: **all checks passed** — build, tests, clippy `-D warnings`,
rustfmt.
