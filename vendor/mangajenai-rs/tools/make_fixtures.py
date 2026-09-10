#!/usr/bin/env python3
"""Generate the synthetic manga regression corpus under ``fixtures/``.

Real scanned manga cannot be committed to this repository, but the failure
modes we care about are all reproducible synthetically: aliased screentone
grids, hairline strokes, tiny ruby text, tone visible *behind* text, and JPEG
ringing around hard black-on-white edges.

Two families are produced:

``fixtures/{general,screentones,furigana}/``
    Small crops used for export-parity checks (phase 1) and for the quality
    corpus (phase 8).

``fixtures/tiling/``
    Images deliberately sized so that the interesting content straddles a tile
    boundary for the default 512px tile / 32px overlap, used by the phase 3
    seam regression tests.

Everything is greyscale-valued and deterministic, so a byte-identical corpus is
regenerated on any machine::

    python tools/make_fixtures.py
"""

from __future__ import annotations

import argparse
import io
import os
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

# Deliberately mundane sample text: common kanji with their kana readings, the
# kind of thing that fills a speech bubble.
SAMPLE_LINES = [
    ("時間", "じかん"),
    ("世界", "せかい"),
    ("魔法", "まほう"),
    ("約束", "やくそく"),
    ("必要", "ひつよう"),
]
SAMPLE_SENTENCE = "そんなことはないと思うけど"


def find_cjk_font() -> Path | None:
    """Locate a CJK-capable font, preferring the one the dev shell pins."""
    for var in ("MANGAJANAI_CJK_FONT", "MANGAJANAI_CJK_FONT_DIR"):
        value = os.environ.get(var)
        if not value:
            continue
        path = Path(value)
        if path.is_file():
            return path
        if path.is_dir():
            for pattern in ("**/*.ttc", "**/*.otf", "**/*.ttf"):
                for candidate in sorted(path.glob(pattern)):
                    return candidate
    try:
        out = subprocess.run(
            ["fc-match", "-f", "%{file}", ":lang=ja"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if out.returncode == 0 and out.stdout.strip():
            return Path(out.stdout.strip())
    except (OSError, subprocess.SubprocessError):
        pass
    return None


def load_font(font_path: Path | None, size: int) -> ImageFont.FreeTypeFont | None:
    if font_path is None:
        return None
    try:
        return ImageFont.truetype(str(font_path), size)
    except OSError:
        return None


def halftone(
    width: int,
    height: int,
    pitch: float,
    angle_deg: float,
    coverage: float | np.ndarray,
) -> np.ndarray:
    """Render a classic AM halftone screen as hard-edged black dots on white.

    Real screentone is a bilevel pattern; keeping it aliased (no anti-aliasing)
    is the point, because the moire interaction between that grid and the
    model's receptive field is exactly what MangaJaNai's per-resolution models
    exist to handle.
    """
    yy, xx = np.mgrid[0:height, 0:width].astype(np.float64)
    angle = np.deg2rad(angle_deg)
    xr = xx * np.cos(angle) + yy * np.sin(angle)
    yr = -xx * np.sin(angle) + yy * np.cos(angle)

    fx = (xr % pitch) - pitch / 2.0
    fy = (yr % pitch) - pitch / 2.0
    radial = np.hypot(fx, fy)

    # Dot radius that yields the requested ink coverage for this cell pitch.
    radius = pitch * np.sqrt(np.clip(coverage, 0.0, 1.0) / np.pi)
    return np.where(radial < radius, 0, 255).astype(np.uint8)


def gradient_coverage(width: int, height: int, lo: float, hi: float) -> np.ndarray:
    ramp = np.linspace(lo, hi, width)
    return np.repeat(ramp[None, :], height, axis=0)


def blank(width: int, height: int, value: int = 255) -> Image.Image:
    return Image.new("L", (width, height), value)


def draw_ruby_text(
    image: Image.Image,
    origin: tuple[int, int],
    pairs: list[tuple[str, str]],
    base_size: int,
    font_path: Path | None,
) -> None:
    """Draw kanji with furigana ruby above, the way typeset manga does it.

    Ruby is drawn at half the base size, which is where strokes collapse to
    1-2px and upscalers tend to destroy them.
    """
    base_font = load_font(font_path, base_size)
    ruby_font = load_font(font_path, max(base_size // 2, 6))
    if base_font is None or ruby_font is None:
        return

    draw = ImageDraw.Draw(image)
    x, y = origin
    for kanji, reading in pairs:
        base_box = draw.textbbox((0, 0), kanji, font=base_font)
        base_width = base_box[2] - base_box[0]
        ruby_box = draw.textbbox((0, 0), reading, font=ruby_font)
        ruby_width = ruby_box[2] - ruby_box[0]

        ruby_y = y
        base_y = y + (ruby_box[3] - ruby_box[1]) + max(base_size // 8, 2)

        draw.text(
            (x + (base_width - ruby_width) / 2, ruby_y), reading, font=ruby_font, fill=0
        )
        draw.text((x, base_y), kanji, font=base_font, fill=0)
        x += base_width + max(base_size // 3, 4)


def draw_vertical_text(
    image: Image.Image,
    origin: tuple[int, int],
    text: str,
    size: int,
    font_path: Path | None,
) -> None:
    font = load_font(font_path, size)
    if font is None:
        return
    draw = ImageDraw.Draw(image)
    x, y = origin
    for char in text:
        draw.text((x, y), char, font=font, fill=0)
        y += int(size * 1.05)


def speech_bubble(
    width: int, height: int, text: str, font_path: Path | None, tone_behind: bool
) -> Image.Image:
    if tone_behind:
        base = halftone(width, height, pitch=6.0, angle_deg=45.0, coverage=0.35)
        image = Image.fromarray(base, mode="L")
    else:
        image = blank(width, height)

    draw = ImageDraw.Draw(image)
    margin = max(width // 12, 6)
    draw.ellipse(
        (margin, margin, width - margin, height - margin), fill=255, outline=0, width=2
    )

    font = load_font(font_path, max(height // 10, 8))
    if font is not None:
        box = draw.textbbox((0, 0), text, font=font)
        draw.text(
            ((width - (box[2] - box[0])) / 2, (height - (box[3] - box[1])) / 2),
            text,
            font=font,
            fill=0,
        )
    return image


def line_art(width: int, height: int) -> Image.Image:
    """Strokes at 1, 2 and 3px plus curves -- the hairlines are the hard part."""
    image = blank(width, height)
    draw = ImageDraw.Draw(image)

    for index, stroke_width in enumerate((1, 1, 2, 3)):
        y = 12 + index * 18
        draw.line((8, y, width - 8, y), fill=0, width=stroke_width)

    for index, stroke_width in enumerate((1, 2, 3)):
        x = 12 + index * 16
        draw.line((x, 90, x, height - 8), fill=0, width=stroke_width)

    draw.arc((width // 2, 90, width - 8, height - 8), 0, 270, fill=0, width=1)
    draw.arc((width // 3, 100, width - 30, height - 20), 45, 300, fill=0, width=2)

    # A near-horizontal hairline: worst case for any resampling step.
    draw.line((8, height - 6, width - 8, height - 14), fill=0, width=1)
    return image


def jpeg_damage(image: Image.Image, quality: int) -> Image.Image:
    buffer = io.BytesIO()
    image.convert("L").save(buffer, format="JPEG", quality=quality)
    buffer.seek(0)
    with Image.open(buffer) as decoded:
        return decoded.convert("L").copy()


def composite_page(width: int, height: int, font_path: Path | None) -> Image.Image:
    """A small stand-in for a real page: tone, panel borders, text, gradient."""
    image = blank(width, height)
    tone = Image.fromarray(
        halftone(width, height // 2, pitch=5.0, angle_deg=15.0, coverage=0.4), mode="L"
    )
    image.paste(tone, (0, height // 2))

    gradient = Image.fromarray(
        halftone(
            width,
            height // 4,
            pitch=4.0,
            angle_deg=75.0,
            coverage=gradient_coverage(width, height // 4, 0.05, 0.85),
        ),
        mode="L",
    )
    image.paste(gradient, (0, height // 4))

    draw = ImageDraw.Draw(image)
    draw.rectangle((4, 4, width - 5, height - 5), outline=0, width=3)
    draw.line((0, height // 4 - 2, width, height // 4 - 2), fill=0, width=2)

    bubble = speech_bubble(
        width // 2, height // 5, SAMPLE_SENTENCE[:6], font_path, tone_behind=False
    )
    image.paste(bubble, (width // 4, 12))

    draw_ruby_text(image, (16, height // 2 + 16), SAMPLE_LINES[:2], 24, font_path)
    draw_vertical_text(image, (width - 40, height // 2 + 10), "縦書き", 20, font_path)
    return image


def build_general(font_path: Path | None) -> dict[str, Image.Image]:
    out: dict[str, Image.Image] = {}

    ramp = np.repeat(
        np.linspace(0, 255, 256, dtype=np.float64)[None, :], 256, axis=0
    ).astype(np.uint8)
    out["gradient_linear"] = Image.fromarray(ramp, mode="L")

    yy, xx = np.mgrid[0:256, 0:256].astype(np.float64)
    radial = np.hypot(xx - 127.5, yy - 127.5)
    out["gradient_radial"] = Image.fromarray(
        np.clip(radial / radial.max() * 255.0, 0, 255).astype(np.uint8), mode="L"
    )

    out["lineart_strokes"] = line_art(256, 256)
    out["composite_page"] = composite_page(384, 512, font_path)
    out["flat_white"] = blank(128, 128, 255)
    out["flat_black"] = blank(128, 128, 0)
    return out


def build_screentones() -> dict[str, Image.Image]:
    out: dict[str, Image.Image] = {}
    for pitch in (3.0, 4.0, 6.0, 9.0):
        for angle in (0.0, 15.0, 45.0):
            key = f"dots_p{pitch:g}_a{angle:g}"
            out[key] = Image.fromarray(
                halftone(256, 256, pitch, angle, 0.4), mode="L"
            )

    out["dots_gradient"] = Image.fromarray(
        halftone(256, 256, 5.0, 45.0, gradient_coverage(256, 256, 0.02, 0.95)),
        mode="L",
    )

    # Line screen rather than a dot screen.
    yy, xx = np.mgrid[0:256, 0:256].astype(np.float64)
    for pitch, angle in ((4.0, 0.0), (5.0, 45.0)):
        a = np.deg2rad(angle)
        xr = xx * np.cos(a) + yy * np.sin(a)
        out[f"lines_p{pitch:g}_a{angle:g}"] = Image.fromarray(
            np.where((xr % pitch) < pitch / 2, 0, 255).astype(np.uint8), mode="L"
        )
    return out


def build_furigana(font_path: Path | None) -> dict[str, Image.Image]:
    out: dict[str, Image.Image] = {}

    for base_size in (14, 20, 28):
        image = blank(320, 96)
        draw_ruby_text(image, (12, 16), SAMPLE_LINES[:3], base_size, font_path)
        out[f"ruby_base{base_size}"] = image

    small = blank(256, 96)
    for index, size in enumerate((8, 10, 12)):
        font = load_font(font_path, size)
        if font is not None:
            ImageDraw.Draw(small).text(
                (10, 8 + index * 26), SAMPLE_SENTENCE, font=font, fill=0
            )
    out["small_kanji"] = small

    vertical = blank(160, 256)
    draw_vertical_text(vertical, (24, 12), "縦書きの台詞", 22, font_path)
    draw_vertical_text(vertical, (84, 12), "小さい文字", 13, font_path)
    out["vertical_text"] = vertical

    out["bubble_plain"] = speech_bubble(
        256, 160, SAMPLE_SENTENCE[:7], font_path, tone_behind=False
    )
    out["bubble_over_tone"] = speech_bubble(
        256, 160, SAMPLE_SENTENCE[:7], font_path, tone_behind=True
    )

    # Damaged variants of the ruby crop: the two things that most often destroy
    # thin strokes before an upscaler ever sees them.
    ruby = out["ruby_base20"]
    out["ruby_jpeg_q25"] = jpeg_damage(ruby, quality=25)
    out["ruby_jpeg_q60"] = jpeg_damage(ruby, quality=60)
    out["ruby_blurred"] = ruby.filter(ImageFilter.GaussianBlur(radius=0.8))
    return out


# Seam fixtures are generated at two geometries. The 256 set is small enough
# to run through a 4x model in seconds, so it is the one used routinely; the
# 512 set matches the shipped default tile size and is for thorough runs.
# Widths are chosen as `tile + (tile - overlap)` for a 32px overlap, so the
# horizontal split lands on exactly two tiles sharing the *nominal* overlap.
# The heights deliberately do not divide evenly, so the vertical split
# exercises the clamped final tile, which overlaps its predecessor by more
# than the nominal amount.
SEAM_GEOMETRIES = [
    # (suffix, width, height, seam position == tile size)
    ("256", 480, 360, 256),
    ("512", 992, 700, 512),
]


def build_tiling(font_path: Path | None) -> dict[str, Image.Image]:
    """Content placed exactly across the first tile seam.

    A tile of size N puts the first seam at x=N, so features are centred there.
    A blending bug shows up as a vertical discontinuity right down the middle
    of these images.
    """
    out: dict[str, Image.Image] = {}
    for suffix, width, height, seam in SEAM_GEOMETRIES:
        out.update(
            {
                f"{name}_{suffix}": image
                for name, image in build_tiling_at(
                    font_path, width, height, seam
                ).items()
            }
        )
    return out


def build_tiling_at(
    font_path: Path | None, width: int, height: int, seam: int
) -> dict[str, Image.Image]:
    out: dict[str, Image.Image] = {}

    tone = Image.fromarray(halftone(width, height, 5.0, 45.0, 0.45), mode="L")
    out["seam_screentone"] = tone

    tone_gradient = Image.fromarray(
        halftone(width, height, 5.0, 15.0, gradient_coverage(width, height, 0.05, 0.9)),
        mode="L",
    )
    out["seam_screentone_gradient"] = tone_gradient

    lines = blank(width, height)
    draw = ImageDraw.Draw(lines)
    for index, stroke in enumerate((1, 2, 3, 5)):
        y = int(height * (index + 1) / 5)
        draw.line((0, y, width, y), fill=0, width=stroke)
    for index, stroke in enumerate((1, 2, 3)):
        x = seam - 40 + index * 40
        draw.line((x, 0, x, height), fill=0, width=stroke)
    draw.line((0, 0, width, height), fill=0, width=1)
    out["seam_lines"] = lines

    ramp = np.repeat(
        np.linspace(0, 255, width, dtype=np.float64)[None, :], height, axis=0
    ).astype(np.uint8)
    out["seam_gradient"] = Image.fromarray(ramp, mode="L")

    text = blank(width, height)
    font = load_font(font_path, 26)
    if font is not None:
        draw = ImageDraw.Draw(text)
        for row in range(max(height // 90, 1)):
            draw.text((seam - 130, 20 + row * 80), SAMPLE_SENTENCE, font=font, fill=0)
    draw_ruby_text(text, (seam - 90, int(height * 0.6)), SAMPLE_LINES[:3], 22, font_path)
    draw_vertical_text(text, (seam - 8, int(height * 0.75)), "縦書き", 20, font_path)
    out["seam_text"] = text

    bubble_page = blank(width, height)
    bubble_size = (min(360, width - 20), min(220, height // 2))
    bubble = speech_bubble(
        bubble_size[0], bubble_size[1], SAMPLE_SENTENCE[:7], font_path, tone_behind=True
    )
    bubble_page.paste(bubble, (seam - bubble_size[0] // 2, height // 3))
    bubble_page.paste(
        Image.fromarray(halftone(width, height // 4, 4.0, 75.0, 0.5), mode="L"), (0, 0)
    )
    out["seam_bubble"] = bubble_page
    return out


def ground_truth_scene(
    kind: str, scale: int, font_path: Path | None
) -> Image.Image:
    """Render a text scene at `scale`, so 1x and 4x renders are a matched pair.

    This is the point of the whole group: because the corpus is synthetic, the
    4x render is genuine high-resolution ground truth for the 1x render -- what
    the page *would* look like scanned at four times the resolution. Upscaling
    the 1x version and comparing against the 4x version measures what the model
    actually loses, which no reference-free metric can do.

    Every dimension, font size and stroke width is multiplied by `scale`, so
    the two renders differ only in resolution.
    """
    width, height = 240 * scale, 80 * scale
    image = blank(width, height)
    draw = ImageDraw.Draw(image)

    if kind == "ruby":
        base_size = 20 * scale
        ruby_size = 10 * scale
        base_font = load_font(font_path, base_size)
        ruby_font = load_font(font_path, ruby_size)
        if base_font is not None and ruby_font is not None:
            x = 10 * scale
            for kanji, reading in SAMPLE_LINES[:3]:
                box = draw.textbbox((0, 0), kanji, font=base_font)
                ruby_box = draw.textbbox((0, 0), reading, font=ruby_font)
                base_width = box[2] - box[0]
                ruby_width = ruby_box[2] - ruby_box[0]
                draw.text(
                    (x + (base_width - ruby_width) / 2, 6 * scale),
                    reading,
                    font=ruby_font,
                    fill=0,
                )
                draw.text((x, 26 * scale), kanji, font=base_font, fill=0)
                x += base_width + 8 * scale
    elif kind == "small_text":
        for index, size in enumerate((8, 11, 14)):
            font = load_font(font_path, size * scale)
            if font is not None:
                draw.text((8 * scale, (6 + index * 22) * scale), SAMPLE_SENTENCE, font=font, fill=0)
    elif kind == "hairlines":
        for index, stroke in enumerate((1, 2, 3)):
            y = (14 + index * 22) * scale
            draw.line((4 * scale, y, width - 4 * scale, y), fill=0, width=stroke * scale)
        # A 1px diagonal: the case most easily lost to any resampling.
        draw.line((4 * scale, 4 * scale, width - 4 * scale, height - 4 * scale), fill=0, width=scale)
    elif kind == "text_on_tone":
        tone = halftone(width, height, 6.0 * scale, 45.0, 0.3)
        image = Image.fromarray(tone, mode="L")
        draw = ImageDraw.Draw(image)
        font = load_font(font_path, 14 * scale)
        if font is not None:
            draw.text((10 * scale, 30 * scale), SAMPLE_SENTENCE, font=font, fill=0)
    else:
        raise ValueError(f"unknown ground truth scene: {kind}")

    return image


GROUND_TRUTH_SCENES = ["ruby", "small_text", "hairlines", "text_on_tone"]


def build_ground_truth(font_path: Path | None, scale: int) -> dict[str, Image.Image]:
    out: dict[str, Image.Image] = {}
    for kind in GROUND_TRUTH_SCENES:
        out[f"{kind}_1x"] = ground_truth_scene(kind, 1, font_path)
        out[f"{kind}_{scale}x"] = ground_truth_scene(kind, scale, font_path)
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, default=Path("fixtures"))
    parser.add_argument(
        "--font", type=Path, help="explicit CJK font file (overrides discovery)"
    )
    parser.add_argument(
        "--ground-truth-scale",
        type=int,
        default=4,
        help="scale factor for the matched ground-truth renders",
    )
    args = parser.parse_args()

    font_path = args.font or find_cjk_font()
    if font_path is None:
        print(
            "warning: no CJK font found -- text fixtures will be blank.\n"
            "         Enter `nix develop`, or pass --font /path/to/font.otf",
        )
    else:
        print(f"font: {font_path}")

    groups = {
        "general": build_general(font_path),
        "screentones": build_screentones(),
        "furigana": build_furigana(font_path),
        "tiling": build_tiling(font_path),
        "groundtruth": build_ground_truth(font_path, args.ground_truth_scale),
    }

    total = 0
    for group, images in groups.items():
        directory = args.out_dir / group
        directory.mkdir(parents=True, exist_ok=True)
        for name, image in sorted(images.items()):
            path = directory / f"{name}.png"
            image.save(path, optimize=True)
            total += 1
        print(f"  {group:<12} {len(images):>3} images -> {directory}")

    print(f"\n{total} fixtures written")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
