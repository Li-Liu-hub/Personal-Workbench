"""Dynamic decoding for BOSS's obfuscated salary font.

BOSS maps digits onto private-use codepoints and can reshuffle that mapping
between font loads. The mapping is rebuilt by comparing the PUA glyph outlines
from the loaded salary font with ASCII digit outlines from either the same font
or the page's regular text font (kanzhun-Regular), because the salary font
often contains only the PUA glyphs. No static codepoint table is used, because
a stale table silently produces wrong digits.
"""

from __future__ import annotations

import base64
import json
import re
from io import BytesIO
from typing import Any
from urllib.parse import urljoin

import httpx

_PUA_MIN = 0xE000
_PUA_MAX = 0xF8FF
_PUA_GLYPH = re.compile(r"[\ue000-\uf8ff]")
_ASCII_DIGITS = "0123456789"

_RANGE_CODE = re.compile(r"[Uu]\+([0-9A-Fa-f]{4})")
_FONT_URL = re.compile(r"url\(\s*['\"]?([^'\")]+)['\"]?\s*\)")
_FONT_FACE_BLOCK = re.compile(r"@font-face\s*\{([^}]*)\}", re.IGNORECASE | re.DOTALL)
_FONT_FAMILY = re.compile(r"font-family\s*:\s*(['\"]?)([^;'\"}]+)", re.IGNORECASE)
_FONT_URL_ALL = re.compile(r"url\(\s*(['\"]?)([^)'\"]+)\1\s*\)", re.IGNORECASE)
_STYLESHEET_URL = re.compile(r"^https?://")

_STYLESHEET_CACHE: dict[str, str] = {}
_REFERENCE_FONT_CACHE: dict[str, bytes] = {}
_MAX_STYLESHEETS = 24
_MAX_REFERENCE_FONTS = 4

_FONT_FETCH_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
        "(KHTML, like Gecko) Chrome/126.0 Safari/537.36"
    ),
    "Referer": "https://www.zhipin.com/",
}

JS_COLLECT_FONT_SOURCES = """
(() => {
	const faces = [];
	try {
		document.fonts.forEach((face) => {
			faces.push({src: String(face.src || ''), range: String(face.unicodeRange || '')});
		});
	} catch (err) {}
	const sheets = [];
	try {
		document.querySelectorAll('link[rel="stylesheet"][href]').forEach((link) => sheets.push(link.href));
	} catch (err) {}
	const inline = [];
	try {
		document.querySelectorAll('style').forEach((tag) => inline.push(String(tag.textContent || '')));
	} catch (err) {}
	return JSON.stringify({faces, sheets, inline, href: location.href});
})()
"""


def has_private_glyph(value: Any) -> bool:
    return bool(_PUA_GLYPH.search(str(value or "")))


def decode_text(value: Any, mapping: dict[str, str]) -> str:
    text = str(value or "")
    if mapping:
        text = text.translate({ord(code): digit for code, digit in mapping.items()})
    return text.strip()


def _covers_private_use(range_text: str) -> bool:
    for hex_code in _RANGE_CODE.findall(range_text or ""):
        if _PUA_MIN <= int(hex_code, 16) <= _PUA_MAX:
            return True
    return False


def _face_font_urls(faces: Any) -> list[str]:
    urls: list[str] = []
    for face in faces if isinstance(faces, list) else []:
        if not isinstance(face, dict) or not _covers_private_use(str(face.get("range") or "")):
            continue
        match = _FONT_URL.search(str(face.get("src") or ""))
        if match and match.group(1) not in urls:
            urls.append(match.group(1))
    return urls


def parse_font_face_urls(css: str) -> list[str]:
    urls: list[str] = []
    for face in parse_font_faces(css):
        if not face["covers_pua"]:
            continue
        for url in face["urls"]:
            if url not in urls:
                urls.append(url)
    return urls


def _font_url_score(url: str) -> int:
    path = url.lower().split("#", 1)[0].split("?", 1)[0]
    if path.endswith(".woff2") or path.startswith("data:font/woff2"):
        return 0
    if path.endswith(".woff") or path.startswith("data:font/woff"):
        return 1
    if path.endswith((".ttf", ".otf")) or path.startswith(("data:font/ttf", "data:font/otf")):
        return 2
    if path.endswith(".eot"):
        return 90
    return 50


def parse_font_faces(css: str) -> list[dict[str, Any]]:
    faces: list[dict[str, Any]] = []
    for match in _FONT_FACE_BLOCK.finditer(css or ""):
        block = match.group(1)
        family_match = _FONT_FAMILY.search(block)
        family = family_match.group(2).strip().lower() if family_match else ""
        urls: list[str] = []
        for font_url in _FONT_URL_ALL.finditer(block):
            url = font_url.group(2)
            if url and url not in urls:
                urls.append(url)
        urls.sort(key=_font_url_score)
        faces.append({"family": family, "urls": urls, "covers_pua": _covers_private_use(block)})
    return faces


def _font_candidates(css: str, face_urls: list[str]) -> tuple[list[str], list[str], list[str]]:
    mix_urls: list[str] = []
    reference_urls: list[str] = []
    fallback_urls = list(face_urls)
    for face in parse_font_faces(css):
        family = face["family"]
        for url in face["urls"]:
            if "kanzhun-mix" in family:
                if url not in mix_urls:
                    mix_urls.append(url)
            elif "kanzhun-regular" in family:
                if url not in reference_urls:
                    reference_urls.append(url)
            elif face["covers_pua"] and url not in fallback_urls:
                fallback_urls.append(url)
    return mix_urls, reference_urls, fallback_urls


def collect_font_sources(value: Any) -> tuple[list[str], list[str], str]:
    data: Any = value
    if isinstance(data, str):
        try:
            data = json.loads(data)
        except json.JSONDecodeError:
            return [], [], ""
    if not isinstance(data, dict):
        return [], [], ""
    urls = _face_font_urls(data.get("faces"))
    css_sources = [str(text) for text in data.get("inline") or [] if isinstance(text, str)]
    for href in data.get("sheets") or []:
        if isinstance(href, str) and _STYLESHEET_URL.match(href):
            css_sources.append(href)
    return urls, css_sources, str(data.get("href") or "")


def _glyph_signature(glyph_set: Any, glyph_name: str) -> tuple | None:
    try:
        from fontTools.pens.recordingPen import DecomposingRecordingPen
    except ImportError:
        return None
    pen = DecomposingRecordingPen(glyph_set)
    try:
        glyph_set[glyph_name].draw(pen)
    except Exception:
        return None
    points: list[tuple[float, float]] = []
    for _operator, args in pen.value:
        for arg in args:
            if isinstance(arg, tuple) and len(arg) == 2:
                points.append((float(arg[0]), float(arg[1])))
    if len(points) < 3:
        return None
    min_x = min(point[0] for point in points)
    min_y = min(point[1] for point in points)
    span = max(max(point[0] for point in points) - min_x, max(point[1] for point in points) - min_y) or 1.0
    ops: list[tuple[str, tuple]] = []
    for operator, args in pen.value:
        normalized: list[tuple[float, float]] = []
        for arg in args:
            if not (isinstance(arg, tuple) and len(arg) == 2):
                return None
            normalized.append((
                round((float(arg[0]) - min_x) / span, 3),
                round((float(arg[1]) - min_y) / span, 3),
            ))
        ops.append((operator, tuple(normalized)))
    return tuple(ops)


def _digit_signatures(font: Any) -> dict[str, tuple]:
    cmap = font.getBestCmap() or {}
    glyph_set = font.getGlyphSet()
    signatures: dict[str, tuple] = {}
    for digit in _ASCII_DIGITS:
        glyph_name = cmap.get(ord(digit))
        if not glyph_name:
            continue
        signature = _glyph_signature(glyph_set, glyph_name)
        if signature is not None:
            signatures[digit] = signature
    return signatures


def build_boss_digit_map(font_bytes: bytes, reference_bytes: bytes | None = None) -> dict[str, str]:
    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        return {}
    font = None
    reference = None
    try:
        font = TTFont(BytesIO(font_bytes), fontNumber=0, lazy=True)
        cmap = font.getBestCmap()
        if not cmap:
            return {}
        glyph_set = font.getGlyphSet()
        digit_signatures = _digit_signatures(font)
        if reference_bytes:
            reference = TTFont(BytesIO(reference_bytes), fontNumber=0, lazy=True)
            for digit, signature in _digit_signatures(reference).items():
                digit_signatures.setdefault(digit, signature)
        if not digit_signatures:
            return {}
        mapping: dict[str, str] = {}
        used_digits: set[str] = set()
        for code, glyph_name in cmap.items():
            if not (_PUA_MIN <= code <= _PUA_MAX):
                continue
            signature = _glyph_signature(glyph_set, glyph_name)
            if signature is None:
                continue
            for digit, digit_signature in digit_signatures.items():
                if digit in used_digits:
                    continue
                if signature == digit_signature:
                    mapping[chr(code)] = digit
                    used_digits.add(digit)
                    break
        return mapping
    except Exception:
        return {}
    finally:
        for item in (font, reference):
            if item is not None:
                try:
                    item.close()
                except Exception:
                    pass


class SalaryDecoder:
    """Rebuild and hold the digit mapping for one loaded page."""

    def __init__(self, tab: Any, client: httpx.Client | None = None) -> None:
        self.tab = tab
        self.mapping: dict[str, str] = {}
        self.error = ""
        self._client = client or httpx.Client(trust_env=False, timeout=15)

    def refresh(self) -> bool:
        try:
            sources = self.tab.evaluate(JS_COLLECT_FONT_SOURCES)
            face_urls, css_sources, href = collect_font_sources(sources)
            page_url = href or self.tab.current_url() or "https://www.zhipin.com/"
            css = "\n".join(source for source in css_sources if not source.startswith("http"))
            mix_urls: list[str] = []
            reference_urls: list[str] = []
            for stylesheet in [source for source in css_sources if source.startswith("http")][:20]:
                text = self._stylesheet(stylesheet)
                if not text:
                    continue
                css += "\n" + text
                part_mix, part_reference, _fallback = _font_candidates(text, [])
                mix_urls.extend(url for url in part_mix if url not in mix_urls)
                reference_urls.extend(url for url in part_reference if url not in reference_urls)
                if mix_urls and reference_urls:
                    break
            css_mix, css_reference, fallback_urls = _font_candidates(css, face_urls)
            mix_urls = mix_urls or css_mix
            reference_urls = reference_urls or css_reference
            candidates = mix_urls[:3] or fallback_urls[:3]
            for raw_url in candidates:
                font_bytes = self._fetch_font(urljoin(page_url, raw_url), page_url)
                if not font_bytes:
                    continue
                mapping = build_boss_digit_map(font_bytes)
                if not mapping and reference_urls:
                    reference_bytes = self._reference_font(reference_urls, page_url)
                    if reference_bytes:
                        mapping = build_boss_digit_map(font_bytes, reference_bytes)
                if mapping:
                    self.mapping = mapping
                    self.error = ""
                    return True
            self.error = "font_mapping_not_found"
            return False
        except Exception as exc:
            self.error = f"font_refresh_failed: {exc}"
            return False

    def _stylesheet(self, url: str) -> str:
        cached = _STYLESHEET_CACHE.get(url)
        if cached is not None:
            return cached
        try:
            response = self._client.get(url, headers=_FONT_FETCH_HEADERS)
            if response.status_code != 200:
                return ""
            text = response.text
        except httpx.HTTPError:
            return ""
        if len(_STYLESHEET_CACHE) >= _MAX_STYLESHEETS:
            _STYLESHEET_CACHE.pop(next(iter(_STYLESHEET_CACHE)))
        _STYLESHEET_CACHE[url] = text
        return text

    def _reference_font(self, urls: list[str], page_url: str) -> bytes | None:
        for raw_url in urls[:3]:
            url = urljoin(page_url, raw_url)
            cached = _REFERENCE_FONT_CACHE.get(url)
            if cached:
                return cached
            data = self._fetch_font(url, page_url)
            if data:
                if len(_REFERENCE_FONT_CACHE) >= _MAX_REFERENCE_FONTS:
                    _REFERENCE_FONT_CACHE.pop(next(iter(_REFERENCE_FONT_CACHE)))
                _REFERENCE_FONT_CACHE[url] = data
                return data
        return None

    def _fetch_font(self, url: str, page_url: str) -> bytes | None:
        if url.startswith("data:"):
            try:
                header, payload = url.split(",", 1)
                return base64.b64decode(payload) if ";base64" in header else payload.encode()
            except (ValueError, TypeError):
                return None
        try:
            response = self._client.get(url, headers={**_FONT_FETCH_HEADERS, "Referer": page_url})
            if response.status_code == 200 and response.content:
                return response.content
        except httpx.HTTPError:
            pass
        encoded = json.dumps(url)
        try:
            data = self.tab.evaluate(
                f"""
                (async () => {{
                    try {{
                        const response = await fetch({encoded});
                        const buffer = await response.arrayBuffer();
                        const bytes = new Uint8Array(buffer);
                        let binary = '';
                        const chunk = 0x8000;
                        for (let index = 0; index < bytes.length; index += chunk) {{
                            binary += String.fromCharCode.apply(null, bytes.subarray(index, index + chunk));
                        }}
                        return btoa(binary);
                    }} catch (error) {{
                        return '';
                    }}
                }})()
                """,
                await_promise=True,
                timeout=20,
            )
            if isinstance(data, str) and data:
                return base64.b64decode(data)
        except Exception:
            return None
        return None
