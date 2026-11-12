"""Build the bundled Noto Serif SC heading font.

The desktop UI only uses a serif face for headings, document titles and dates.
Bundling the full CJK family would add tens of megabytes, so this script:

1. pins the variable font to weight 600;
2. keeps GB2312, ASCII, common punctuation and every character used by the
   React source files;
3. writes a WOFF2 subset to desktop/public/fonts.

Usage:
    python scripts/build-font-subset.py [source.ttf]

The default source is the Noto Serif SC variable font installed with Windows.
Font license: SIL Open Font License 1.1 (see public/fonts/OFL.txt).
"""

from __future__ import annotations

import pathlib
import sys

from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESKTOP = ROOT / "desktop"
OUTPUT = DESKTOP / "public" / "fonts" / "NotoSerifSC-SemiBold-subset.woff2"
DEFAULT_SOURCE = pathlib.Path(r"C:\Windows\Fonts\NotoSerifSC-VF.ttf")


def gb2312_characters() -> set[str]:
    characters: set[str] = set()
    for high in range(0xB0, 0xF8):
        for low in range(0xA1, 0xFF):
            try:
                characters.add(bytes((high, low)).decode("gb2312"))
            except UnicodeDecodeError:
                pass
    return characters


def source_characters() -> set[str]:
    characters: set[str] = set()
    for pattern in ("*.tsx", "*.ts"):
        for path in (DESKTOP / "src").rglob(pattern):
            characters.update(path.read_text(encoding="utf-8"))
    return characters


def main() -> int:
    source = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SOURCE
    if not source.exists():
        print(f"Font source not found: {source}", file=sys.stderr)
        return 1
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)

    font = TTFont(source)
    if "fvar" in font:
        instantiateVariableFont(font, {"wght": 600}, inplace=True)

    characters = gb2312_characters() | source_characters()
    characters.update(chr(code) for code in range(0x20, 0x7F))
    characters.update("，。！？；：“”‘’（）《》〈〉【】、·—…￥％＋－×÷＝《》「」『』")
    text = "".join(sorted(characters))
    text_file = OUTPUT.with_suffix(".chars.txt")
    text_file.write_text(text, encoding="utf-8")

    from fontTools import subset

    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.notdef_outline = True
    options.recommended_glyphs = True
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text=text)
    subsetter.subset(font)
    font.flavor = "woff2"
    font.save(OUTPUT)
    text_file.unlink(missing_ok=True)
    print(f"Wrote {OUTPUT} ({OUTPUT.stat().st_size / 1024 / 1024:.2f} MiB, {len(characters)} chars)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())