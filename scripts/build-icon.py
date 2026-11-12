"""Draw the flat ink-green desk icon and export master PNGs.

The icon is intentionally simple so it survives 16 px: an ink rounded square,
an open paper book, and a cinnabar bookmark. Run `npx tauri icon` afterwards to
generate the Windows/macOS icon set.
"""

from __future__ import annotations

import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icon-design-v2"
S = 4096
INK = (27, 45, 37, 255)
INK_LIGHT = (47, 78, 64, 255)
PAPER = (255, 253, 248, 255)
PAPER_EDGE = (226, 220, 207, 255)
SEAL = (180, 71, 47, 255)


def page_layer(offset_x: int, angle: float, bookmark: bool) -> Image.Image:
    page_w, page_h = 1180, 1580
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    x0 = S // 2 + offset_x - page_w // 2
    y0 = S // 2 - page_h // 2 - 60
    draw.rounded_rectangle((x0, y0, x0 + page_w, y0 + page_h), radius=110, fill=PAPER)
    draw.rounded_rectangle((x0, y0, x0 + page_w, y0 + page_h), radius=110, outline=PAPER_EDGE, width=8)
    if bookmark:
        ribbon_w = 120
        ribbon_x = x0 + int(page_w * 0.62)
        ribbon_top = y0 - 30
        ribbon_bottom = y0 + int(page_h * 0.52)
        notch = 46
        draw.polygon([
            (ribbon_x, ribbon_top),
            (ribbon_x + ribbon_w, ribbon_top),
            (ribbon_x + ribbon_w, ribbon_bottom),
            (ribbon_x + ribbon_w // 2, ribbon_bottom - notch),
            (ribbon_x, ribbon_bottom),
        ], fill=SEAL)
    return layer.rotate(angle, resample=Image.BICUBIC, center=(S // 2, S // 2))


def build() -> Image.Image:
    image = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    margin, radius = 170, 520
    draw.rounded_rectangle((margin, margin, S - margin, S - margin), radius=radius, fill=INK)
    draw.rounded_rectangle((margin, margin, S - margin, S - margin), radius=radius, outline=INK_LIGHT, width=10)

    left = page_layer(-300, -7.5, bookmark=False)
    right = page_layer(300, 7.5, bookmark=True)
    image = Image.alpha_composite(image, left)
    image = Image.alpha_composite(image, right)
    return image


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    master = build()
    master.resize((1024, 1024), Image.LANCZOS).save(OUT / "personaldesk-master.png")
    preview = Image.new("RGBA", (1024, 240), (255, 255, 255, 255))
    for index, size in enumerate((16, 24, 32, 48, 64, 128)):
        icon = master.resize((size, size), Image.LANCZOS)
        preview.alpha_composite(icon, (40 + index * 150, 48))
    preview.save(OUT / "size-preview.png")
    master.resize((1024, 1024), Image.LANCZOS).save(ROOT / "assets" / "personaldesk.png")
    print(f"Wrote {OUT / 'personaldesk-master.png'}")


if __name__ == "__main__":
    main()