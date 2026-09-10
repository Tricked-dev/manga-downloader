# Exporting other architectures

ESRGAN is the v0.1 target. This records what happens with the other
architectures the wider MangaJaNai/IllustrationJaNai ecosystem uses, following
the procedure in the plan: isolate the unsupported operation, check whether the
modern exporter handles it, try a decomposition or rewrite, add a narrowly
scoped adapter only if needed, and **verify numerical equivalence again**.

Test subjects are the IllustrationJaNai V3 denoise models, which ship SPAN,
DAT2 and FDAT variants. FDAT is not part of spandrel proper; upstream
distributes it as a patch to the spandrel package tree (`spandrel-fdat.zip`),
applied here through a writable overlay on the nixpkgs spandrel.

## Results

All five non-ESRGAN test subjects export successfully. Only SPAN needed an
adapter; the transformer architectures traced cleanly as they are.

| architecture | model | ONNX | adapter needed | parity |
| --- | --- | --- | --- | --- |
| ESRGAN | `4x_MangaJaNai_1600p_V1_ESRGAN_70k` | 70 MB | none | **PASS**, 1 LSB |
| SPAN | `2x_..._SPAN_S_30k` | 1.7 MB | **reparameterisation fold** | **PASS**, 1 LSB |
| DAT2 | `4x_..._DAT2_27k` | 143 MB | none | not run |
| FDAT-M | `4x_..._FDAT_M_47k` | 19 MB | none | not run |
| FDAT-M unshuffle | `2x_..._FDAT_M_unshuffle_30k` | 19 MB | none | not run |
| FDAT-XL | `4x_..._FDAT_XL_32k` | 97 MB | none | not run |

All via the dynamo exporter at opset 18, all with symbolic height/width — the
export fails deliberately if either is baked in.

These are exports, not yet a supported pipeline: none has been through
`verify_onnx.py`, and their size requirements have not been exercised. See
"Notes for future work".

## SPAN: a segfault, not an exception

The first export attempt did not raise. It **crashed the process**:

```
python3.14[25398]: segfault at 0 ip 0000790e37558a40 sp 00007ffc5bb5cea0
    error 4 in libtorch_cpu.so[8558a40,790e2fd82000+c88e000]
```

A null-pointer read at the same instruction on every attempt — reproducible,
and not memory exhaustion (6 GB was still available, and an OOM kill is
SIGKILL, not SIGSEGV).

This matters for tooling as much as for SPAN: **a segfault cannot be caught by
`except Exception`**, so `export_onnx.py`'s automatic fallback to the
TorchScript exporter never got a chance to run. A crash in the exporter takes
the whole process with it.

### Isolating the operation

`spandrel/architectures/SPAN/__arch/span.py`, `Conv3XC.forward`:

```python
def forward(self, x):
    if self.training:
        ...
    else:
        self.update_params()      # <-- every forward pass
        out = self.eval_conv(x)
```

and `update_params` ends with:

```python
self.eval_conv.weight.data = self.weight_concat.contiguous()
self.eval_conv.bias.data = self.bias_concat.contiguous()
```

So each eval forward recomputes a fused kernel and **assigns to a Parameter's
`.data` in the middle of the traced call**. It also writes `self.weight_concat`
and `self.bias_concat`, which are plain attributes rather than registered
buffers — PyTorch warns about exactly this before dying:

> The tensor attributes `self.block_2.c3_r.bias_concat`, ... were assigned
> during export. Such attributes must be registered as buffers using the
> `register_buffer` API.

Tracing a graph that mutates its own parameters is the pathological case here.

### The adapter

The recomputation is **redundant**. With frozen weights `update_params()` is
deterministic, so running it once and skipping it afterwards leaves the eval
forward mathematically identical. `fold_reparameterising_convs` calls it once
per module and then shadows the method with a no-op on that instance, which
keeps the rest of `forward` — the skip connection, the LeakyReLU — exactly as
written instead of reimplementing it.

This is a rewrite of *when* an operation runs, not a substitution of one
operation for another. Nothing is approximated, and the plan's rule against
replacing operators with approximate alternatives is not being bent.

### Verification

`export_one` captures the module's output *before* applying any adapter and
compares afterwards, refusing to export if anything moved:

```
    adapters: fold-reparameterising-convs(20) (output unchanged)
    SPAN x2 3ch -> ..._SPAN_S_30k_fp16.onnx (via dynamo, opset 18)
```

20 modules folded, output difference exactly **0.0**. The adapter is inert for
architectures that do not have `update_params`: re-exporting
`4x_MangaJaNai_1600p` after adding it records `adapters: []` and produces a
**bit-identical** ONNX file to the one committed before the change, which also
happens to establish that the dynamo export is deterministic.

The folded graph then goes through the same parity gate as ESRGAN, which is
the step that actually matters — an adapter that exports cleanly but changes
the maths would be worse than a failed export:

```
  lineart_strokes.png   max=2.146e-06  u8max=1  u8diff=1/786432
  ruby_base20.png       max=2.623e-06  u8max=1  u8diff=3/368640
  random-200x200        max=8.017e-06  u8max=1  u8diff=75/480000
  ...
  PARITY PASS
```

Worst max absolute error **8.0e-06**, 1 LSB across all six cases — the same
quality of agreement as the unmodified ESRGAN path.

## Notes for future work

- `export_onnx.py`'s dynamo-then-TorchScript fallback only covers exceptions.
  Running the dynamo attempt in a subprocess would let a segfault fall back
  rather than abort the run. Worth doing before adding more architectures.
- Transformer architectures (DAT, FDAT, HAT) use windowed attention and
  typically declare a `multiple_of` size requirement. The padding rule in
  `manga-core::padding` already implements Spandrel's reflect-then-replicate
  behaviour for exactly this, and it is tested, but it has never been exercised
  by a model that actually needs it. That is the next thing to check for these
  graphs.
- DAT2 and the three FDAT variants have **not** been through
  `verify_onnx.py`. Exporting without a crash is not the same as exporting
  correctly. SPAN has been verified (above); the transformer models are large
  enough that a parity sweep on this CPU-only machine costs hours, so it is
  left as the next step rather than skipped silently.
- These are IllustrationJaNai models, which target colour illustration rather
  than manga line art. They are useful here purely as architecture test
  subjects.
