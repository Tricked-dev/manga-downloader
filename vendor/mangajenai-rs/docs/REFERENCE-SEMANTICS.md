# Reference semantics

Behaviour the Rust pipeline must reproduce, read out of the reference
implementations rather than inferred. Every claim here is either quoted from
source or measured by `tools/verify_onnx.py`; nothing in this file is a guess.

Versions inspected: **spandrel 0.4.2**, **torch 2.13.0**, **onnxruntime 1.27.1**.

## The model

`4x_MangaJaNai_1600p_V1_ESRGAN_70k.pth`, as reported by
`spandrel.ModelLoader().load_from_file(...)`:

| property | value |
| --- | --- |
| architecture | `ESRGAN` (`RRDBNet`, 64nf / 23nb, 16,697,987 params) |
| scale | 4 |
| input / output channels | **3 / 3** |
| size requirements | `minimum=2, multiple_of=1, square=False` |
| purpose | `SR` |

> MangaJaNai models are **RGB, not greyscale**, despite operating on black and
> white artwork. A single-channel pipeline would be wrong.

## Inference contract

`ImageModelDescriptor.__call__` (`spandrel/__helpers/model_descriptor.py:447`)
is the reference entry point. In order it:

1. Requires a 4-D `(1, C, H, W)` tensor, dtype/device matching the model, values
   in `[0, 1]`.
2. Pads to satisfy the size requirements (see below).
3. Forces `model.eval()`.
4. Runs the module.
5. **Clamps to `[0, 1]`** — `output = output.clamp_(0, 1)`.
6. Crops padding off the output: `output[..., : h * scale, : w * scale]`.

Step 5 is not cosmetic. ESRGAN routinely produces values outside `[0, 1]`;
`tools/verify_onnx.py` measures excursions up to **0.514** beyond the range on
noise input. Omitting the clamp and relying on the `u8` cast to saturate would
give different results wherever the cast wraps rather than saturates.

The verifier asserts `descriptor(x) == clamp(module(x), 0, 1)` exactly, so the
Rust postprocessing contract is precisely *clamp, then quantise* — the wrapper
adds nothing else for inputs that need no padding.

## Padding rule

From `spandrel/__helpers/size_req.py`. Required padding for a `W x H` input:

```
w = ceil_to_multiple(max(minimum, W), multiple_of)
h = ceil_to_multiple(max(minimum, H), multiple_of)
if square: w = h = max(w, h)
pad_w, pad_h = w - W, h - H
```

Applied by `pad_tensor`, and this part is easy to get subtly wrong:

- Padding is added on the **right and bottom only** — never left or top.
- It is **reflect** padding, but `F.pad(..., "reflect")` cannot pad by more than
  `size - 1`, so the amount is capped at `min(pad, size - 1)`.
- Any remainder beyond that cap is then applied as **replicate** padding.

For MangaJaNai ESRGAN (`minimum=2, multiple_of=1`) this triggers only for inputs
narrower or shorter than 2px, so in practice no padding occurs. The rule still
has to be implemented correctly for phase 9 architectures, which commonly
require `multiple_of` of 8, 16 or 64.

## Pixel conventions

- Decode to 8-bit RGB, convert to `f32` by dividing by `255.0`.
- Layout `HWC -> NCHW` with a leading batch of 1.
- Spandrel works in **RGB**. The surrounding chaiNNer/OpenCV code in
  MangaJaNaiConverterGui works in BGR and converts at its boundary; since this
  project decodes with the `image` crate (natively RGB) there is no swap to do,
  but the ordering must not be assumed — it is asserted by the phase 2
  Rust-vs-Python pixel comparison.
- Quantise back with **clamp, then round half away from zero**:
  `floor(clamp(x, 0, 1) * 255 + 0.5)`. `numpy.round` and Rust's `f32::round`
  disagree on `.5` ties (banker's vs away-from-zero), so the verifier
  deliberately uses the `floor(x + 0.5)` form on both sides.

## Export

`tools/export_onnx.py` exports FP32 with the **dynamo** exporter
(`torch.onnx.export(..., dynamo=True)`), opset 18, IO named `input` / `output`.

Height and width are declared dynamic with `torch.export.Dim`, not the legacy
`dynamic_axes` dict. The exporter then propagates the symbolic relationship into
the graph: the sidecar records output dims as `4*height` and `4*width`, which is
a machine-checked proof that the graph really scales by 4 for any input size
rather than having a traced size baked in. `export_onnx.py` fails the export if
`height`/`width` do not appear as symbolic dimensions.

The TorchScript exporter remains as an automatic fallback for architectures
dynamo cannot trace (relevant in phase 9), and warns loudly when used.

## Measured export fidelity

`4x_MangaJaNai_1600p_V1_ESRGAN_70k`, PyTorch CPU FP32 vs ONNX Runtime CPU with
graph optimisation disabled (`ORT_DISABLE_ALL`, so float operations are not
reassociated by the runtime):

| input | max abs err | mean abs err | RMSE | max 8-bit err |
| --- | --- | --- | --- | --- |
| random 64x64 | 1.156e-05 | 7.048e-07 | 1.041e-06 | 1 |
| random 97x151 | 1.565e-05 | 7.127e-07 | 1.061e-06 | 1 |
| random 128x256 | 1.714e-05 | 7.137e-07 | 1.063e-06 | 1 |
| random 200x200 | 1.383e-05 | 7.138e-07 | 1.060e-06 | 1 |

At most **1 LSB** of 8-bit difference, on 0.017% of pixels — float summation
order, not a graph difference.

Across the full 35-case sweep (31 fixture crops plus the random shapes), 34
cases land at **1 LSB or better**. One does not, and it is worth understanding
why.

### Chaotic inputs, and why a fixed tolerance is the wrong gate

`fixtures/general/gradient_linear.png` — a perfectly smooth 0→255 horizontal
ramp — comes out at max absolute error **0.551**, or **141** in 8-bit, with 37%
of samples differing. That looks like a broken export. It is not:

| measurement | max abs | max 8-bit |
| --- | --- | --- |
| `torch(x)` vs `torch(x)` (same input, twice) | 0.0 | 0 |
| `torch(x)` vs `torch(x + 1 ULP)` | **0.593** | **151** |
| `torch(x)` vs `onnx(x)` | 0.551 | 141 |

PyTorch is bit-deterministic on repeated runs, so this is not measurement
noise. But perturbing the input by a single float32 ULP (1.19e-07) moves
PyTorch's *own* output **further than switching runtimes does**. Twenty-three
RRDB blocks of residual accumulation on a featureless input is chaotic: the
model has no stable answer there to more than about 1 LSB, and no
implementation could agree more closely.

Demanding 1-LSB agreement on such an input is demanding precision that does not
exist. So `verify_onnx.py` no longer gates on a fixed tolerance alone. When a
case exceeds the tolerance it measures the input's **conditioning** — re-running
the same module on the input nudged by one ULP — and passes the case if the
export differs by no more than that floor. The extra forward pass is only paid
for on cases that look bad.

Note what this is *not*: an excuse. A case that exceeds both the tolerance and
its conditioning floor still fails, and is named in the report. The distinction
is between "the export is wrong" and "the model has no stable answer here".

Real manga content is not affected. Screentone, line art, dialogue, furigana,
JPEG-damaged and blurred text all sit at 1 LSB. Only the synthetic smooth
gradients are ill-conditioned, and a smooth gradient is not manga — it is in
the corpus precisely because it is a worst case.

See `artifacts/parity/report.json` for the full run.

## Model selection (MangaJaNaiConverterGui)

Read out of `MangaJaNaiConverterGui/backend/resources/default_cli_configuration.json`
and `backend/src/run_upscale.py` (`should_chain_activate_for_image`), rather
than inventing thresholds.

Selection is an **ordered chain list**; the first chain that matches wins. Each
chain carries a min/max resolution, a grayscale/colour flag pair, a min/max
scale factor, and a model. `0` means "unbounded" in every numeric field:

```python
if min_width  != 0 and min_width  > width:  reject
if min_height != 0 and min_height > height: reject
if max_width  != 0 and max_width  < width:  reject
if max_height != 0 and max_height < height: reject
if is_grayscale and not chain.IsGrayscale:  reject
if not is_grayscale and not chain.IsColor:  reject
if chain.MaxScaleFactor != 0 and target_scale > chain.MaxScaleFactor: reject
if chain.MinScaleFactor != 0 and target_scale < chain.MinScaleFactor: reject
```

The default "Upscale Manga" workflow resolves to this table. Note the bands are
**not** centred on the model names -- 1600p covers 1551..1760, and 1920p covers
1761..1984:

| source height | 2x model (target scale <= 2) | 4x model (target scale >= 2) |
| --- | --- | --- |
| <= 1250 | `2x_MangaJaNai_1200p_V1_ESRGAN_70k` | `4x_MangaJaNai_1200p_V1_ESRGAN_70k` |
| 1251-1350 | `2x_..._1300p_..._75k` | `4x_..._1300p_..._75k` |
| 1351-1450 | `2x_..._1400p_..._70k` | `4x_..._1400p_..._105k` |
| 1451-1550 | `2x_..._1500p_..._90k` | `4x_..._1500p_..._105k` |
| 1551-1760 | `2x_..._1600p_..._90k` | `4x_..._1600p_..._70k` |
| 1761-1984 | `2x_..._1920p_..._70k` | `4x_..._1920p_..._105k` |
| >= 1985 | `2x_..._2048p_..._95k` | `4x_..._2048p_..._70k` |

Only **height** is constrained; every width bound is 0. A target scale of
exactly 2 matches both the 2x and the 4x chain, and the 2x chain is listed
first, so **scale 2 selects the 2x model**.

Colour pages do not use MangaJaNai at all -- chain 1 catches them first and
routes them to IllustrationJaNai. Grayscale detection
(`cv_image_is_grayscale`, default threshold 12) treats an image as grayscale
when the per-pixel channel differences stay within the threshold, ignoring pure
black and pure white pixels.

### Preprocessing we do not yet reproduce

Every MangaJaNai chain sets `AutoAdjustLevels: true`, and the backend applies
it before inference:

```python
if is_grayscale and chain["AutoAdjustLevels"]:
    image = enhance_contrast(image)
else:
    image = normalize(image)
```

`enhance_contrast` picks a black and a white level from the histogram (walking
outward from the ends, stopping after two consecutive non-increasing bins) and
rescales with `clip((x - black) / (white - black), 0, 1)`.

This is a real difference between "the model output" and "what
MangaJaNaiConverterGui writes to disk". v0.1 targets parity with the *model*;
matching the converter end to end additionally requires `enhance_contrast`,
grayscale detection, and the post-upscale resize. Tracked as follow-up work
rather than silently ignored.
