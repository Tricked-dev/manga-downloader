# Models and upscaling

The Rust worker links the vendored `manga-core` library and the system ONNX Runtime.
Python is used only to export model weights. Model files are not committed to this repository.
All export scripts and their Nix environment live under `vendor/mangajenai-rs`; the original
upstream repository is not needed.

Export separately obtained MangaJaNai `.pth` weights with the vendored tools:

```sh
devenv --profile models shell
python vendor/mangajenai-rs/tools/export_onnx.py --all-in-dir /path/to/weights --out-dir data/models --device cpu
python vendor/mangajenai-rs/tools/make_manifest.py --models-dir data/models
```

Choose an export device suitable for the machine; `--device mps` uses Apple Metal, and
`--device cuda` selects an available CUDA/ROCm PyTorch device. The exporter does not retry on CPU
when an explicitly selected accelerator fails. See the script's `--help` for supported options.
The exporter writes `exported.json` as a report. The second command creates the `models.json`
manifest used by the server. The export environment can be large and is loaded only by the
`models` profile.

`models.json` selects the model by original page height and requested scale. The default chapter
job uses a 2× model; an explicit job can request 4×. Source-height model names describe the input
scan band, not a limit on the output dimensions. The worker uses the upstream tiled inference and
quantization pipeline, then encodes the result as lossless AVIF with libavif/libaom.

Chapters whose median original page width is at least 2000 pixels skip inference. The median
avoids interpreting one wide spread or narrow credits page as the resolution of the whole chapter.
Other chapters retain the normal model scale: a 1400-pixel original becomes 2800 pixels at 2×.
There is no 1400-pixel output cap. Original pages are never resized or replaced.

The server finishes a download when its original BBF section is sealed. The background job stages
one inferred page at a time, then uses BBF's append API to publish the entire upscaled section.
Readers keep the original variant until publication. An interrupted job cannot publish a partial
section; a rerun replaces the upscaled page references while retaining the original payloads.

Linux defaults to explicit CPU inference with two intra-op threads, one inter-op thread, and
spinning disabled. One chapter job runs at a time. Accelerators are opt-in and must be supported by
the installed ONNX Runtime; they never silently fall back to CPU. Missing manifests or model files
produce a terminal skip. Models added later are detected when the chapter is queued again.

To measure the same worker independently:

```sh
cargo build --release -p backend-upscale --example upscale
./target/release/examples/upscale data/models page.jpg output.avif cpu 2
./target/release/examples/upscale data/models page.jpg output-4x.avif cpu 4
```

The report includes output dimensions, elapsed time and file size. Execution profiles are written
beside the output and finalized when the worker stops. Compare identical input pages and resource
limits; whole-chapter time also depends on page count, source size, model band and AVIF encoding.
