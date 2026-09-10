# Upscaling

An **Upscale Job** produces a second version of each original chapter page. A missing models directory is normal and leaves originals available. Installing models permits a later requested job; a missing model is not retried indefinitely.

The **GPU Worker** owns ONNX sessions on one dedicated thread, with one request executing and at most one queued page. The server accepts only explicit GPU providers. ONNX CPU fallback is disabled for the entire graph, including unsupported individual operators. The development PC must never run CPU model inference. OpenVINO is restricted to its GPU device. The AMD path uses MIGraphX.

The **Model Cache** is keyed by graph path, follows manifest selection by source height and requested scale, and reloads changed files. It retains at most two graphs. Tiled inference preserves the full source dimensions multiplied by the model scale; its default tile size is 256 pixels with 32 pixels of overlap.

**Upscale Output** is lossless AVIF: libavif/libaom, full range, 4:4:4, identity RGB matrix, and lossless quantization. Pixel equality after decoding is the acceptance criterion. Encoding uses at most two CPU threads; model inference remains on the GPU.

The diagnostic `upscale` example uses the same worker and saves ONNX execution profiles. Profiles and real GPU execution are required evidence beyond compilation and tests that reject CPU requests.

Chapter jobs run through apalis with one consumer. Completion of original downloads enqueues a job when both `auto_upscale` and `source.<key>.auto_upscale` permit it (both default to true). `POST /v1/downloads/{id}/upscale` requests a rerun. The job stages one GPU output at a time and publishes all pages together with the original footer hash as a concurrency guard. Missing models finish the job without retries.
