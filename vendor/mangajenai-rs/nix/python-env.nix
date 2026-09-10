# One-time .pth -> .onnx conversion toolchain.
#
# This is deliberately the *only* place PyTorch appears in the project. Phases 1
# and 7 (export, parity verification, benchmark baseline) need it; the shipped
# Rust pipeline does not. See tools/export_onnx.py and tools/verify_onnx.py.
{ python3 }:
python3.withPackages (ps: [
  # Model loading — Spandrel owns architecture detection and .pth parsing.
  ps.spandrel
  ps.torch
  ps.torchvision
  ps.safetensors
  ps.einops

  # Export + validation.
  ps.onnx
  ps.onnxscript # required by torch.onnx.export(dynamo=True)
  ps.onnxruntime

  # Image IO and parity reporting.
  ps.numpy
  ps.pillow
  ps.rich
])
