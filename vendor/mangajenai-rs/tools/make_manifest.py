#!/usr/bin/env python3
"""Generate `models/models.json` from the exported ONNX graphs.

Automatic model selection has to reproduce MangaJaNaiConverterGui's semantics
rather than invent thresholds, so the source-height bands below are copied from
its `default_cli_configuration.json` (the "Upscale Manga (Default)" workflow).
They are deliberately *not* derived from the model names: the bands are not
centred on them. The 1600p model covers 1551-1760 and the 1920p model covers
1761-1984.

See `docs/REFERENCE-SEMANTICS.md` for the full rule this encodes.

    python tools/make_manifest.py --models-dir models
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import onnx

# target_height -> (min source height, max source height); 0 means unbounded.
# Transcribed from MangaJaNaiConverterGui's default workflow chain list.
SOURCE_HEIGHT_BANDS: dict[int, tuple[int, int]] = {
    1200: (0, 1250),
    1300: (1251, 1350),
    1400: (1351, 1450),
    1500: (1451, 1550),
    1600: (1551, 1760),
    1920: (1761, 1984),
    2048: (1985, 0),
}

# Upstream lists the 2x chain before the 4x chain for every band, and a target
# scale of exactly 2 satisfies both, so 2 resolves to the 2x model.
SCALE_BOUNDARY = 2


def read_metadata(path: Path) -> dict[str, str]:
    model = onnx.load(str(path), load_external_data=False)
    return {entry.key: entry.value for entry in model.metadata_props}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--models-dir", type=Path, default=Path("models"))
    parser.add_argument("--output", type=Path, help="defaults to <models-dir>/models.json")
    args = parser.parse_args()

    output = args.output or (args.models_dir / "models.json")
    entries = []
    unbanded = []

    for path in sorted(args.models_dir.glob("*.onnx")):
        metadata = read_metadata(path)
        if "mangajanai.scale" not in metadata:
            print(f"  skipping {path.name}: no mangajanai metadata (re-export it)")
            continue

        target_height = int(metadata.get("mangajanai.target_height", 0))
        band = SOURCE_HEIGHT_BANDS.get(target_height)
        if band is None:
            unbanded.append(path.name)

        entries.append(
            {
                "name": metadata.get("mangajanai.name", path.stem),
                "path": path.name,
                "architecture": metadata.get("mangajanai.architecture", "unknown"),
                "scale": int(metadata["mangajanai.scale"]),
                "input_channels": int(metadata.get("mangajanai.input_channels", 3)),
                "target_height": target_height or None,
                "source_height_min": band[0] if band else None,
                "source_height_max": band[1] if band else None,
            }
        )

    manifest = {
        "version": 1,
        "comment": (
            "Source-height bands are transcribed from MangaJaNaiConverterGui's "
            "default workflow; see docs/REFERENCE-SEMANTICS.md. 0 means unbounded, "
            "null means the model does not participate in automatic selection."
        ),
        "scale_boundary": SCALE_BOUNDARY,
        "models": entries,
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2) + "\n")

    banded = [entry for entry in entries if entry["source_height_min"] is not None]
    print(f"wrote {output}: {len(entries)} model(s), {len(banded)} usable for --model auto")
    for entry in sorted(banded, key=lambda e: (e["source_height_min"], e["scale"])):
        low = entry["source_height_min"]
        high = entry["source_height_max"] or "inf"
        print(f"  {low:>5}..{high:<5} x{entry['scale']}  {entry['name']}")
    if unbanded:
        print(f"\nno source-height band for: {', '.join(unbanded)}")
        print("  (these can still be selected explicitly with --model <path>)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
