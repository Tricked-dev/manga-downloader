# Vendored MangaJaNai

Source: https://github.com/Tricked-dev/mangajenai-rs, branch `dev`.
Revision: `747d6ac4c3b5e674046fa8632d8ec8cefc889728`.
License: MIT (see LICENSE).

This directory is a Git archive of that revision. Production model weights are not included.
The server links `crates/manga-core` directly as a path dependency; no download is needed
at application startup. The included tools export separately downloaded `.pth` weights
and create `models.json` for the runtime model directory.

Local patches:

- The workspace minimum Rust version is `1.98` instead of `1.98.1`, matching
  manga-downloader's `nightly-2026-06-01` compiler (1.98.0-nightly).
- Add the AMD MIGraphX device and Cargo provider binding.
- Add `UpscaleModel::open_gpu`, which requires an explicit accelerator and disables
  ONNX CPU fallback before loading a graph. Session helper threads are bounded and
  spinning is disabled. Optional profiles can be finalized for GPU verification.
- Add an explicit export device to `tools/export_onnx.py`, including Metal and ROCm/CUDA.
  The tracing input and reference evaluation use that device, without CPU retries.

Model preprocessing, tiling, inference tensors, and output quantization retain the
upstream implementation. The server links ONNX Runtime through pkg-config.

To update, replace this directory with a fresh archive from the desired upstream revision,
reapply and validate that manifest patch if still necessary, and update this record.
