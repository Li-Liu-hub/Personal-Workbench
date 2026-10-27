"""Build the open-source deep-reading pack and safe in-place upgrades.

The pack is authored as original Chinese explanations. Sources are linked per
topic; no upstream text is copied into the assets. Run from the repository
root: `python scripts/encyclopedia-deep/build.py`.
"""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
AREAS = {
    "编程与算法",
    "Java 与并发",
    "数据库",
    "操作系统与 Linux",
    "网络与 Web",
    "工程可靠性",
    "测试与排障",
    "性能与交付",
}
ENTRY_FIELDS = [
    "id",
    "language",
    "category",
    "title",
    "summary",
    "syntax",
    "example",
    "notes",
    "source",
    "entry_type",
    "tags",
]
# Tags are merged rather than replaced, so they are excluded from the
# fingerprint: the legacy 408 pack and its split packs tag entries differently.
ENTRY_HASH_FIELDS = [
    field for field in ENTRY_FIELDS if field not in ("language", "tags")
]
CARD_HASH_FIELDS = [
    "id",
    "kind",
    "topic",
    "title",
    "prompt",
    "answer",
    "rubric",
    "source",
    "knowledge_ids",
    "training",
]
URL = re.compile(r"https?://[^\s]+")


def load_topics():
    sys.path.insert(0, str(Path(__file__).parent))
    from topics_backend import TOPICS as BACKEND_TOPICS
    from topics_java import TOPICS as JAVA_TOPICS
    from topics_upgrade import UPGRADES

    return JAVA_TOPICS + BACKEND_TOPICS, UPGRADES


def unique_tags(values):
    seen, result = set(), []
    for value in values:
        value = value.strip()
        if value and value not in seen:
            seen.add(value)
            result.append(value)
    return result


def entry_from_topic(topic):
    entry = {
        "id": f"ref:deep:{topic['slug']}",
        "language": topic["language"],
        "category": topic["category"],
        "title": topic["title"],
        "summary": topic["summary"],
        "syntax": topic.get("syntax", ""),
        "example": topic["example"],
        "notes": topic["notes"],
        "source": topic["source"],
        "entry_type": "精读",
        "tags": unique_tags(
            [topic["language"], topic["category"], *topic.get("tags", [])]
        ),
    }
    return entry


def card_from_topic(topic, entry):
    card = topic["card"]
    rubric = [
        {"id": rid, "text": text, "points": points}
        for rid, text, points in card["rubric"]
    ]
    training = dict(card["training"])
    return {
        "id": f"knowledge:{entry['id']}",
        "kind": "engineering",
        "title": card.get("title", f"{entry['title']} · 精读推演"),
        "topic": card.get("topic", "八股题"),
        "prompt": card["prompt"],
        "answer": card["answer"],
        "rubric": rubric,
        "source": entry["source"],
        "knowledge_ids": [entry["id"]],
        "tags": list(entry["tags"]),
        "training": training,
    }


def upgrade_entry(topic, old):
    entry = dict(old)
    for field in ("summary", "syntax", "example", "notes", "source"):
        entry[field] = topic[field]
    entry["entry_type"] = topic.get("entry_type", "精读")
    entry["tags"] = unique_tags(
        [old["language"], old["category"], *old.get("tags", []), *topic.get("tags", []), "精读"]
    )
    entry["id"] = old["id"]
    entry["language"] = old["category"] if old["language"] == "408" else old["language"]
    entry["category"] = old["category"]
    entry["title"] = old["title"]
    return entry


def upgrade_card(topic, old, entry):
    card = topic["card"]
    result = dict(old)
    result["title"] = card.get("title", old["title"])
    result["topic"] = card.get("topic", old["topic"])
    result["prompt"] = card["prompt"]
    result["answer"] = card["answer"]
    result["rubric"] = [
        {"id": rid, "text": text, "points": points}
        for rid, text, points in card["rubric"]
    ]
    result["source"] = entry["source"]
    result["knowledge_ids"] = list(old.get("knowledge_ids") or [entry["id"]])
    result["tags"] = unique_tags(
        [entry["language"], entry["category"], *old.get("tags", []), *topic.get("tags", []), "精读"]
    )
    result["training"] = dict(card["training"])
    return result


def validate_entry(entry):
    errors = []
    for field, limit in [
        ("id", 128),
        ("language", 40),
        ("category", 80),
        ("title", 160),
        ("entry_type", 40),
    ]:
        value = entry.get(field, "")
        if not value.strip() or len(value) > limit:
            errors.append(f"{entry.get('id')} 字段 {field} 为空或超长")
    if len(entry.get("summary", "")) > 800:
        errors.append(f"{entry['id']} summary 超长")
    if len(entry.get("syntax", "")) > 32768 or len(entry.get("example", "")) > 65536:
        errors.append(f"{entry['id']} syntax/example 超长")
    if len(entry.get("notes", "")) > 32768 or len(entry.get("source", "")) > 2048:
        errors.append(f"{entry['id']} notes/source 超长")
    parsed = urlparse(entry.get("source", ""))
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        errors.append(f"{entry['id']} source 不是合法 URL")
    tags = entry.get("tags", [])
    if len(tags) > 24 or any(not tag.strip() or len(tag) > 80 for tag in tags):
        errors.append(f"{entry['id']} tags 不合规")
    return errors


def validate_card(card, entry_ids):
    errors = []
    rubric = card.get("rubric", [])
    if sum(item["points"] for item in rubric) != 100:
        errors.append(f"{card['id']} 评分合计不为 100")
    if len({item["id"] for item in rubric}) != len(rubric):
        errors.append(f"{card['id']} 评分点编号重复")
    if any(item["points"] <= 0 or not item["text"].strip() for item in rubric):
        errors.append(f"{card['id']} 评分点为空或零分")
    for field in ("id", "title", "prompt", "answer", "source"):
        if not str(card.get(field, "")).strip():
            errors.append(f"{card['id']} {field} 为空")
    if not card.get("knowledge_ids") or any(
        item not in entry_ids for item in card["knowledge_ids"]
    ):
        errors.append(f"{card['id']} 知识点关联缺失")
    training = card.get("training") or {}
    if (
        training.get("area") not in AREAS
        or training.get("level") not in (1, 2, 3)
        or not training.get("goal", "").strip()
        or not training.get("deliverable", "").strip()
        or len(training.get("goal", "")) > 400
        or len(training.get("deliverable", "")) > 800
    ):
        errors.append(f"{card['id']} training 不合规")
    return errors


def canonical_hash(value):
    """Stable JSON hash: sorted keys and UTF-8 match Rust's BTreeMap serialization."""
    text = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    )
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def entry_content(entry):
    return {field: entry.get(field, "") for field in ENTRY_HASH_FIELDS}


def card_content(card):
    return {field: card.get(field) for field in CARD_HASH_FIELDS}


def load_asset_entries():
    index = {}
    for path in ASSETS.glob("*.json"):
        if path.name.endswith("_practice.json") or path.name in {
            "deep_readings.json",
            "deep_upgrades.json",
            "deep_upgrade_entry_hashes.json",
            "deep_upgrade_card_hashes.json",
            "cs408.json",
            "release_history.json",
            "curriculum_profiles.json",
            "reference_practice_legacy_hashes.json",
        }:
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if not isinstance(data, list) or not data or "id" not in data[0] or "language" not in data[0]:
            continue
        for item in data:
            index.setdefault(item["id"], []).append((path, item))
    return index


def load_asset_cards():
    index = {}
    for path in ASSETS.glob("*_practice.json"):
        if path.name in {"cs408_practice.json", "deep_readings_practice.json", "deep_upgrades_practice.json"}:
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if not isinstance(data, list):
            continue
        for item in data:
            index.setdefault(item["id"], []).append((path, item))
    return index


def git_history(path, item_id, kind):
    """Collect previous built-in fingerprints so older installs upgrade safely."""
    try:
        commits = subprocess.run(
            ["git", "-C", str(ROOT), "log", "--format=%H", "--", str(path.relative_to(ROOT))],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout.split()
    except Exception:
        return set()
    hashes = set()
    for commit in commits:
        try:
            raw = subprocess.run(
                ["git", "-C", str(ROOT), "show", f"{commit}:{path.relative_to(ROOT).as_posix()}"],
                check=True,
                capture_output=True,
                text=True,
                encoding="utf-8",
            ).stdout
            data = json.loads(raw)
        except Exception:
            continue
        if not isinstance(data, list):
            continue
        for item in data:
            if item.get("id") != item_id:
                continue
            value = entry_content(item) if kind == "entry" else card_content(item)
            hashes.add(canonical_hash(value))
    return hashes


def main():
    new_topics, upgrades = load_topics()
    entries = [entry_from_topic(topic) for topic in new_topics]
    cards = [card_from_topic(topic, entry) for topic, entry in zip(new_topics, entries)]

    entry_index = load_asset_entries()
    card_index = load_asset_cards()
    new_ids = {entry["id"] for entry in entries}
    duplicate = new_ids.intersection(entry_index)
    if duplicate:
        raise SystemExit(f"新增 ID 与已有资产冲突：{sorted(duplicate)[:3]}")

    upgrade_entries, upgrade_cards = [], []
    entry_hashes, card_hashes = {}, {}
    for topic in upgrades:
        old_entries = entry_index.get(topic["entry_id"], [])
        if len(old_entries) != 1:
            raise SystemExit(f"升级条目必须唯一存在：{topic['entry_id']}（{len(old_entries)}）")
        old_path, old_entry = old_entries[0]
        old_cards = card_index.get(topic["card_id"], [])
        if len(old_cards) != 1:
            raise SystemExit(f"升级练习必须唯一存在：{topic['card_id']}（{len(old_cards)}）")
        old_card_path, old_card = old_cards[0]
        if old_entry["id"] != old_card["knowledge_ids"][0]:
            raise SystemExit(f"升级条目与练习关联不一致：{topic['entry_id']}")
        entry = upgrade_entry(topic, old_entry)
        card = upgrade_card(topic, old_card, entry)
        upgrade_entries.append(entry)
        upgrade_cards.append(card)
        entry_hash = {canonical_hash(entry_content(old_entry))}
        entry_hash.update(git_history(old_path, entry["id"], "entry"))
        entry_hashes[entry["id"]] = sorted(entry_hash)
        card_hash = {canonical_hash(card_content(old_card))}
        card_hash.update(git_history(old_card_path, card["id"], "card"))
        card_hashes[card["id"]] = sorted(card_hash)

    errors = []
    entry_ids = new_ids | {item["id"] for item in upgrade_entries}
    for entry in entries + upgrade_entries:
        errors.extend(validate_entry(entry))
    for card in cards + upgrade_cards:
        errors.extend(validate_card(card, entry_ids))
    if errors:
        print("\n".join(f"[FAIL] {error}" for error in errors))
        raise SystemExit(1)

    def write(name, value):
        (ASSETS / name).write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

    write("deep_readings.json", entries)
    write("deep_readings_practice.json", cards)
    write("deep_upgrades.json", upgrade_entries)
    write("deep_upgrades_practice.json", upgrade_cards)
    write("deep_upgrade_entry_hashes.json", entry_hashes)
    write("deep_upgrade_card_hashes.json", card_hashes)
    print(
        f"deep_readings: {len(entries)} 条 / {len(cards)} 题；"
        f"deep_upgrades: {len(upgrade_entries)} 条 / {len(upgrade_cards)} 题"
    )
    print(
        "upgrade fingerprints: "
        f"entries={sum(len(v) for v in entry_hashes.values())}, "
        f"cards={sum(len(v) for v in card_hashes.values())}"
    )


if __name__ == "__main__":
    main()
