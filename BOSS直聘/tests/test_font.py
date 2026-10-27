from __future__ import annotations

import io

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

from bosshunter.boss.font import (
    _font_candidates,
    build_boss_digit_map,
    decode_text,
    has_private_glyph,
    parse_font_faces,
)


def box(x0, y0, x1, y1):
    pen = TTGlyphPen(None)
    pen.moveTo((x0, y0))
    pen.lineTo((x0, y1))
    pen.lineTo((x1, y1))
    pen.lineTo((x1, y0))
    pen.closePath()
    return pen.glyph()


def _font_bytes(glyph_order, cmap, glyf):
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(glyph_order)
    builder.setupCharacterMap(cmap)
    builder.setupGlyf({name: glyf.get(name, TTGlyphPen(None).glyph()) for name in glyph_order})
    builder.setupHorizontalMetrics({name: (600, 0) for name in glyph_order})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({"familyName": "Test", "styleName": "Regular"})
    builder.setupOS2()
    builder.setupPost()
    builder.setupMaxp()
    buffer = io.BytesIO()
    builder.save(buffer)
    return buffer.getvalue()


def test_font_mapping():
    font = _font_bytes(
        [".notdef", "zero", "one", "pua_zero", "pua_one"],
        {ord("0"): "zero", ord("1"): "one", 0xE000: "pua_zero", 0xE001: "pua_one"},
        {
            "zero": box(100, 0, 500, 700),
            "one": box(200, 0, 300, 700),
            "pua_zero": box(100, 0, 500, 700),
            "pua_one": box(200, 0, 300, 700),
        },
    )
    mapping = build_boss_digit_map(font)
    assert mapping[chr(0xE000)] == "0"
    assert mapping[chr(0xE001)] == "1"


def test_font_mapping_uses_reference_font():
    reference = _font_bytes(
        [".notdef", "zero", "one"],
        {ord("0"): "zero", ord("1"): "one"},
        {"zero": box(100, 0, 500, 700), "one": box(200, 0, 300, 700)},
    )
    salary = _font_bytes(
        [".notdef", "pua_zero", "pua_one"],
        {0xE100: "pua_zero", 0xE101: "pua_one"},
        {"pua_zero": box(100, 0, 500, 700), "pua_one": box(200, 0, 300, 700)},
    )
    assert build_boss_digit_map(salary) == {}
    mapping = build_boss_digit_map(salary, reference)
    assert mapping == {chr(0xE100): "0", chr(0xE101): "1"}


def test_font_face_parser_collects_kanzhun_fonts_without_unicode_range():
    css = """
    @font-face{font-family:kanzhun-Regular;src:url(https://cdn.example/regular.ttf) format("truetype")}
    @font-face{font-family:kanzhun-mix;src:url(https://cdn.example/mix.eot);
      src:url(https://cdn.example/mix.eot?#iefix) format("embedded-opentype"),
         url(https://cdn.example/mix.woff2) format("woff2"),
         url(https://cdn.example/mix.woff) format("woff")}
    """
    faces = parse_font_faces(css)
    assert [face["family"] for face in faces] == ["kanzhun-regular", "kanzhun-mix"]
    assert faces[1]["urls"][0].endswith("mix.woff2")
    assert faces[1]["urls"][-1].startswith("https://cdn.example/mix.eot")
    mix, reference, fallback = _font_candidates(css, [])
    assert mix[0].endswith("mix.woff2")
    assert reference[0].endswith("regular.ttf")
    assert fallback == []


def test_decode_text_and_private_glyph_detection():
    mapping = {chr(0xE000): "5"}
    assert decode_text(" 1\ue000K ", mapping) == "15K"
    assert has_private_glyph("\ue000")
    assert not has_private_glyph("15K")
