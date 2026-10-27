"""构建百科扩充资产：编码规范 + 后端八股 + Python 进阶，并补齐 java_ecosystem 训练画像。

运行 `python scripts/encyclopedia-extras/build.py`（仅重建本次新增资产与画像文件，不写用户数据库）。
校验规则对齐 src/reference.rs Entry::validate 与 src/study.rs Card::validate。
"""
import json
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
sys.path.insert(0, str(Path(__file__).parent))

from coding_standards import ROWS as CS_ROWS, SCOPE as CS_SCOPE, SOURCE as CS_SOURCE
from backend_interview import ROWS as BI_ROWS, SCOPE as BI_SCOPE
from python_pack import ROWS as PY_ROWS, SCOPE as PY_SCOPE
from more_languages import ROWS as ML_ROWS, SCOPE as ML_SCOPE

AREAS = ["编程与算法", "Java 与并发", "数据库", "操作系统与 Linux", "网络与 Web", "工程可靠性", "测试与排障", "性能与交付"]

ADV = "https://github.com/doocs/advanced-java/blob/main/docs/high-concurrency/"
PATTERNS_URL = "https://github.com/iluwatar/java-design-patterns/tree/master/"

# 后端八股包逐条来源（模式目录已与仓库索引核对，Redis/MQ 为官方文档或 advanced-java 已核对文档路径）
BI_SOURCES = {
    **{f"pattern-{p}": PATTERNS_URL + d for p, d in {
        "singleton": "singleton", "factory-method": "factory-method", "abstract-factory": "abstract-factory",
        "builder": "builder", "prototype": "prototype", "adapter": "adapter", "bridge": "bridge",
        "composite": "composite", "decorator": "decorator", "facade": "facade", "flyweight": "flyweight",
        "proxy": "proxy", "chain": "chain-of-responsibility", "command": "command", "interpreter": "interpreter",
        "iterator": "iterator", "mediator": "mediator", "memento": "memento", "observer": "observer",
        "state": "state", "strategy": "strategy", "template-method": "template-method", "visitor": "visitor",
    }.items()},
    "redis-why-cache": ADV + "why-cache.md",
    "redis-data-types": "https://redis.io/docs/latest/develop/data-types/",
    "redis-structures": "https://github.com/redis/redis",
    "redis-rehash": "https://github.com/redis/redis",
    "redis-expiration": "https://redis.io/docs/latest/commands/expire/",
    "redis-persistence": "https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/",
    "redis-replication": "https://redis.io/docs/latest/operate/oss_and_stack/management/replication/",
    "redis-sentinel": "https://redis.io/docs/latest/operate/oss_and_stack/management/sentinel/",
    "redis-cluster": "https://redis.io/docs/latest/operate/oss_and_stack/management/scaling/",
    "redis-avalanche": ADV + "redis-caching-avalanche-and-caching-penetration.md",
    "redis-penetration": ADV + "redis-caching-avalanche-and-caching-penetration.md",
    "redis-breakdown": ADV + "redis-caching-avalanche-and-caching-penetration.md",
    "redis-consistence": ADV + "redis-consistence.md",
    "redis-cas": ADV + "redis-cas.md",
    "redis-bigkey": ADV + "redis-production-environment.md",
    "redis-bloom": "https://redis.io/docs/latest/develop/data-types/probabilistic/bloom-filter/",
    "redis-transaction": "https://redis.io/docs/latest/commands/eval/",
    "redis-vs-memcached": ADV + "redis-single-thread-model.md",
    "redis-slowlog": ADV + "redis-production-environment.md",
    "mq-why": ADV + "why-mq.md",
    "mq-selection": ADV + "why-mq.md",
    "mq-reliable": ADV + "how-to-ensure-the-reliable-transmission-of-messages.md",
    "mq-idempotent": ADV + "how-to-ensure-that-messages-are-not-repeatedly-consumed.md",
    "mq-order": ADV + "how-to-ensure-the-order-of-messages.md",
    "mq-ha": ADV + "how-to-ensure-high-availability-of-message-queues.md",
    "mq-backlog": ADV + "mq-time-delay-and-expired-failure.md",
    "mq-delay": ADV + "mq-time-delay-and-expired-failure.md",
    "mq-transaction": "https://rocketmq.apache.org/docs/",
    "mq-kafka-arch": "https://kafka.apache.org/documentation/#design",
    "mq-rocket-arch": "https://rocketmq.apache.org/docs/",
    "mq-push-pull": "https://kafka.apache.org/documentation/#theconsumer",
    "mq-dlq": "https://www.rabbitmq.com/docs/dlx",
    "mq-design-self": "https://kafka.apache.org/documentation/#design",
}

# Python 包逐条来源（官方文档与 PEP）
PY_SOURCES = {    "gil": "https://docs.python.org/3/glossary.html#term-global-interpreter-lock",
    "mutable-default": "https://docs.python.org/3/faq/programming.html#why-are-default-values-evaluated-at-definition-time",
    "decorator": "https://docs.python.org/3/glossary.html#term-decorator",
    "generator-yield": "https://docs.python.org/3/howto/functional.html#generators",
    "context-manager": "https://docs.python.org/3/library/contextlib.html",
    "copy-shallow": "https://docs.python.org/3/library/copy.html",
    "is-vs-equals": "https://docs.python.org/3/reference/datamodel.html#customization",
    "args-kwargs": "https://docs.python.org/3/tutorial/controlflow.html#more-on-defining-functions",
    "exception-best-practice": "https://docs.python.org/3/tutorial/errors.html",
    "asyncio-basic": "https://docs.python.org/3/library/asyncio.html",
    "slots": "https://docs.python.org/3/reference/datamodel.html#slots",
    "dataclass": "https://docs.python.org/3/library/dataclasses.html",
    "typing": "https://docs.python.org/3/library/typing.html",
    "string-fstring": "https://docs.python.org/3/tutorial/inputoutput.html#formatted-string-literals",
    "errors-eafp": "https://docs.python.org/3/glossary.html#term-EAFP",
    "walrus": "https://peps.python.org/pep-0572/",
    "modules-import": "https://docs.python.org/3/reference/import.html",
}

# 更多语言包的逐条来源与语言归属（前缀决定语言与文档入口）
ML_LANGUAGES = {"ts": "TypeScript", "php": "PHP", "kotlin": "Kotlin", "cs": "C#", "lua": "Lua"}
ML_DOCS = {
    "TypeScript": "https://www.typescriptlang.org/docs/handbook/intro.html",
    "PHP": "https://www.php.net/manual/en/langref.php",
    "Kotlin": "https://kotlinlang.org/docs/home.html",
    "C#": "https://learn.microsoft.com/en-us/dotnet/csharp/",
    "Lua": "https://www.lua.org/manual/5.4/",
}
ML_LEVEL3 = {
    "ts-generics-constraint", "ts-declaration-module", "kotlin-coroutines", "kotlin-dsl",
    "cs-async-await", "cs-di-lifetime", "lua-redis-script", "lua-performance",
}


def ml_language(slug):
    return ML_LANGUAGES[slug.split("-")[0]]


ML_SOURCES = {}
for _line in ML_ROWS.strip().splitlines():
    _slug = _line.split("｜")[0].strip()
    if _slug and "｜" in _line and not _slug.startswith(" "):
        first = _slug.split("-")[0]
        if first in ML_LANGUAGES and _slug.count("-") >= 1:
            ML_SOURCES[_slug] = ML_DOCS[ml_language(_slug)]

# 408 拆分为四个独立学科包（语言即学科名，条目与练习 ID 保持不变）
CS408_SUBJECTS = {
    "数据结构": "data_structures",
    "计算机组成原理": "computer_organization",
    "操作系统": "operating_systems",
    "计算机网络": "computer_networks",
}

# 每个包的静态配置：主题行、条目类型、学习范围、标签前缀、版本前提、topic、section 到 (area, level) 的映射
PACKS = {
    "coding_standards": {
        "rows": CS_ROWS, "scope": CS_SCOPE, "entry_type": "规范", "topic": "八股题",
        "language": "Java", "category": "编码规范", "tag": "Java", "version": "Java 17+ / 《Java 开发手册》黄山版",
        "default_source": CS_SOURCE,
        "section_area": {
            "命名": ("工程可靠性", 1), "常量": ("工程可靠性", 1), "OOP": ("工程可靠性", 2),
            "集合": ("Java 与并发", 2), "线程": ("Java 与并发", 2), "控制": ("工程可靠性", 2),
            "注释": ("工程可靠性", 1), "异常": ("工程可靠性", 2), "日志": ("测试与排障", 2),
            "MySQL": ("数据库", 2), "设计": ("工程可靠性", 3), "工程": ("工程可靠性", 2), "项目": ("工程可靠性", 2),
        },
        "section_of": lambda slug: {"naming": "命名", "constant": "常量", "oop": "OOP", "collection": "集合",
                                     "thread": "线程", "control": "控制", "comment": "注释", "exception": "异常",
                                     "log": "日志", "mysql": "MySQL", "design": "设计", "project": "项目"}.get(
            slug.split("-")[0], "工程"),
    },
    "backend_interview": {
        "rows": BI_ROWS, "scope": BI_SCOPE, "entry_type": "原理", "topic": "八股题",
        "language": "后端", "category": "", "tag": "后端", "version": None, "default_source": None,
        "source_map": BI_SOURCES,
        "section_area": {
            "pattern": ("工程可靠性", 2), "redis-why": ("性能与交付", 1), "redis-data": ("数据库", 1),
            "redis-structures": ("数据库", 2), "redis-rehash": ("数据库", 2), "redis-expiration": ("数据库", 2),
            "redis-persistence": ("数据库", 2), "redis-replication": ("数据库", 2), "redis-sentinel": ("数据库", 2),
            "redis-cluster": ("数据库", 2), "redis-avalanche": ("性能与交付", 3), "redis-penetration": ("性能与交付", 2),
            "redis-breakdown": ("性能与交付", 2), "redis-consistence": ("性能与交付", 3), "redis-cas": ("性能与交付", 3),
            "redis-bigkey": ("性能与交付", 2), "redis-bloom": ("性能与交付", 2), "redis-transaction": ("数据库", 2),
            "redis-vs": ("数据库", 1), "redis-slowlog": ("测试与排障", 2),
            "mq-": ("工程可靠性", 2),
        },
    },
    "python": {
        "rows": PY_ROWS, "scope": PY_SCOPE, "entry_type": "概念", "topic": "八股题",
        "language": "Python", "category": "进阶专题", "tag": "Python", "version": "CPython 3.10+",
        "default_source": None, "source_map": PY_SOURCES,
        "section_area": {"__all__": ("编程与算法", 2)},
    },
    "more_languages": {
        "rows": ML_ROWS, "scope": ML_SCOPE, "entry_type": "概念", "topic": "八股题",
        "language": None, "language_of": ml_language, "tag_is_language": True,
        "category": "进阶专题", "version": None, "default_source": None,
        "source_map": ML_SOURCES,
        "version_of": {
            "TypeScript": "TypeScript 5.x", "PHP": "PHP 8.2+", "Kotlin": "Kotlin 2.0+",
            "C#": "C# 12 / .NET 8", "Lua": "Lua 5.4",
        },
        "level_of": lambda slug: 3 if slug in ML_LEVEL3 else 2,
        "section_area": {"__all__": ("编程与算法", 2)},
    },
}


SLUG = re.compile(r"^[a-z][a-z0-9]*(\.[a-z0-9]+|-[a-z0-9]+)*｜")


def parse_rows(raw):
    """逻辑行以 slug｜ 开头（允许跨物理行），字段 7 个（来源走包默认）或 8 个（行内给来源）。"""
    blocks, buf = [], ""
    for line in raw.strip("\n").split("\n"):
        if buf and SLUG.match(line):
            blocks.append(buf)
            buf = line
        else:
            buf = line if not buf else buf + "\n" + line
    if buf.strip():
        blocks.append(buf)
    rows = []
    for b in blocks:
        parts = [p.strip() for p in b.split("｜")]
        if len(parts) == 7:
            parts.append("")  # 来源字段缺省，构建时取包默认
        if len(parts) != 8:
            raise ValueError(f"字段数不是 7/8：{b[:60]}...（{len(parts)}）")
        rows.append(parts)
    return rows


def section_key(slug):
    for key in ("redis-why", "redis-data", "redis-structures", "redis-rehash", "redis-expiration",
                "redis-persistence", "redis-replication", "redis-sentinel", "redis-cluster",
                "redis-avalanche", "redis-penetration", "redis-breakdown", "redis-consistence",
                "redis-cas", "redis-bigkey", "redis-bloom", "redis-transaction", "redis-vs",
                "redis-slowlog", "mq-", "pattern-"):
        if slug.startswith(key.rstrip("-") if key == "pattern-" else key):
            return key
    return slug.split("-")[0]


def area_level(pack, slug):
    cfg = PACKS[pack]
    mapping = cfg["section_area"]
    if "section_of" in cfg:
        section = cfg["section_of"](slug)
        if section in mapping:
            return mapping[section]
        raise ValueError(f"未定义 area 映射：{pack}:{slug}（{section}）")
    key = section_key(slug)
    if key in mapping:
        return mapping[key]
    for k, v in mapping.items():
        if k != "__all__" and slug.startswith(k.rstrip("-")):
            return v
    if "__all__" in mapping:
        return mapping["__all__"]
    raise ValueError(f"未定义 area 映射：{pack}:{slug}")


def build_pack(pack):
    cfg = PACKS[pack]
    entries, cards = [], []
    seen = set()
    for slug, title, summary, example, notes, question, answer, source in parse_rows(cfg["rows"]):
        if slug in seen:
            raise ValueError(f"重复 slug：{pack}:{slug}")
        seen.add(slug)
        entry_id = f"ref:{pack}:{slug}"
        lang = cfg["language_of"](slug) if "language_of" in cfg else cfg["language"]
        src = source or cfg.get("source_map", {}).get(slug) or cfg.get("default_source") or ""
        if not src:
            raise ValueError(f"{entry_id} 缺少来源")
        boundary = cfg["scope"]
        category = cfg["category"] or ("设计模式" if slug.startswith("pattern-")
                                        else "Redis" if slug.startswith("redis-")
                                        else "消息队列" if slug.startswith("mq-") else "其他")
        if slug.startswith("pattern-"):
            group = {"singleton": "创建型", "factory": "创建型", "abstract": "创建型", "builder": "创建型",
                     "prototype": "创建型", "adapter": "结构型", "bridge": "结构型", "composite": "结构型",
                     "decorator": "结构型", "facade": "结构型", "flyweight": "结构型", "proxy": "结构型",
                     "chain": "行为型", "command": "行为型", "interpreter": "行为型", "iterator": "行为型",
                     "mediator": "行为型", "memento": "行为型", "observer": "行为型", "state": "行为型",
                     "strategy": "行为型", "template": "行为型", "visitor": "行为型"}[slug.split("-")[1]]
        elif slug.startswith("redis-"):
            group = "缓存场景" if slug in ("redis-avalanche", "redis-penetration", "redis-breakdown",
                                          "redis-consistence", "redis-cas", "redis-bigkey", "redis-bloom",
                                          "redis-why") else "原理与运维"
        elif pack == "backend_interview":
            group = "消息队列"
        elif pack == "coding_standards":
            group = cfg["section_of"](slug)
        else:
            group = "进阶专题"
        entry_tags = list(dict.fromkeys([lang if cfg.get("tag_is_language") else cfg["tag"], category, group, slug]))
        if "version_of" in cfg:
            version = cfg["version_of"][lang]
        else:
            version = cfg["version"] or {"redis-": "Redis 7.2", "mq-": "Kafka 3.x / RocketMQ 5.x / RabbitMQ 3.13",
                                         "pattern-": "Java 17+"}[
                next((k for k in ("redis-", "mq-", "pattern-") if slug.startswith(k)), "")]
        entries.append({
            "id": entry_id, "language": lang, "category": category, "title": title,
            "summary": summary, "syntax": example, "example": example,
            "notes": f"{boundary}\n\n原理与边界：{notes}\n\n自检：{question}\n参考结论：{answer}",
            "source": src, "entry_type": cfg["entry_type"], "tags": entry_tags,
        })
        area, level = area_level(pack, slug)
        if "level_of" in cfg:
            level = cfg["level_of"](slug)
        prompt = (f"知识点：{title}\n版本：{version}\n示例或场景：\n{example}\n\n"
                  f"任务：{question}\n写出结论、关键原因，并指出一个与本场景有关的边界或失败条件。"
                  f"接受语义等价的正确回答，不要求复述原文。")
        cards.append({
            "id": f"knowledge:{entry_id}", "kind": "engineering", "topic": cfg["topic"],
            "title": f"{title} · 推演练习", "prompt": prompt, "answer": answer,
            "rubric": [
                {"id": "e1", "text": f"正确解释本题涉及的核心机制或规范：{title}", "points": 35},
                {"id": "e2", "text": f"完成本题具体任务：{question}", "points": 45},
                {"id": "e3", "text": "指出一个相关边界或失败条件，接受合理的其他案例", "points": 20},
            ],
            "source": src, "knowledge_ids": [entry_id], "tags": entry_tags,
            "training": {"area": area, "level": level, "goal": summary, "deliverable": question},
        })
    return entries, cards


# java_ecosystem 训练画像：按条目分类映射能力方向，goal 取条目摘要，deliverable 取卡题任务句
ECO_AREA = [
    (("并发",), "Java 与并发", 2), (("Java核心",), "Java 与并发", 1), (("Java生态",), "Java 与并发", 2),
    (("数据访问",), "数据库", 3), (("中间件",), "数据库", 2), (("Spring Boot",), "工程可靠性", 2),
    (("Spring",), "工程可靠性", 2), (("分布式",), "工程可靠性", 3), (("工程",), "工程可靠性", 2),
    (("Java AI",), "工程可靠性", 2), (("安全",), "网络与 Web", 2), (("Web",), "网络与 Web", 2),
    (("JVM",), "性能与交付", 3),
]
ECO_LEVEL3 = ("锁与内存模型", "一致性", "事务", "运行时与诊断", "注册与RPC")


def eco_area_level(category):
    for keys, area, level in ECO_AREA:
        if any(category.startswith(k) for k in keys):
            for frag in ECO_LEVEL3:
                if frag in category:
                    return area, 3
            return area, level
    raise ValueError(f"未映射的 java_ecosystem 分类：{category}")


def build_profiles():
    entries = {e["id"]: e for e in json.loads((ASSETS / "java_ecosystem.json").read_text(encoding="utf-8"))}
    cards = json.loads((ASSETS / "java_ecosystem_practice.json").read_text(encoding="utf-8"))
    profiles = json.loads((ASSETS / "curriculum_profiles.json").read_text(encoding="utf-8"))
    known = {p["id"] for p in profiles}
    added = 0
    for card in cards:
        if card["id"] in known or not card["id"].startswith("knowledge:ref:java-eco:"):
            continue
        entry_id = card["id"][len("knowledge:"):]
        entry = entries[entry_id]
        area, level = eco_area_level(entry["category"])
        match = re.search(r"任务：(.*?)(?:\n|$)", card["prompt"])
        task = match.group(1).strip() if match else card["title"]
        profiles.append({
            "id": card["id"], "area": area, "level": level,
            "goal": entry["summary"][:400],
            "deliverable": f"{task}（写出结论、关键原因与一个边界条件。）"[:800],
        })
        added += 1
    raw = (ASSETS / "curriculum_profiles.json").read_text(encoding="utf-8")
    newline = "\n" if raw.endswith("\n") else ""
    profiles.sort(key=lambda p: p["id"])
    (ASSETS / "curriculum_profiles.json").write_text(
        json.dumps(profiles, ensure_ascii=False, indent=2) + newline, encoding="utf-8")
    return added, len(profiles)


def validate(entries, cards):
    errors = []
    entry_ids = set()
    for e in entries:
        entry_ids.add(e["id"])
        for field, limit in [("id", 128), ("language", 40), ("category", 80), ("title", 160), ("entry_type", 40)]:
            if not e[field].strip() or len(e[field]) > limit:
                errors.append(f"{e['id']} 字段 {field} 超限")
        if len(e["summary"]) > 800 or len(e["notes"]) > 32768 or len(e["source"]) > 2048:
            errors.append(f"{e['id']} 内容超长")
        u = urlparse(e["source"])
        if u.scheme not in ("http", "https") or not u.netloc:
            errors.append(f"{e['id']} 来源不是合法 URL：{e['source']}")
        if len(e["tags"]) > 24 or any(not t.strip() or len(t) > 80 for t in e["tags"]):
            errors.append(f"{e['id']} 标签不合规")
    for c in cards:
        if sum(r["points"] for r in c["rubric"]) != 100:
            errors.append(f"{c['id']} 评分合计不为 100")
        ids = [r["id"] for r in c["rubric"]]
        if len(set(ids)) != len(ids):
            errors.append(f"{c['id']} 评分点编号重复")
        if not c["knowledge_ids"] or any(k not in entry_ids for k in c["knowledge_ids"]):
            errors.append(f"{c['id']} 知识点关联缺失")
        t = c["training"]
        if t["area"] not in AREAS or t["level"] not in (1, 2, 3) or not t["goal"].strip() or not t["deliverable"].strip():
            errors.append(f"{c['id']} training 不合规：{t['area']}/{t['level']}")
        if len(t["goal"]) > 400 or len(t["deliverable"]) > 800:
            errors.append(f"{c['id']} training 超长")
        if c["kind"] not in ("engineering", "word"):
            errors.append(f"{c['id']} kind 非法")
    return errors


def split_cs408():
    """把 408 聚合包拆成四个独立学科包：语言=学科名，条目与练习 ID 保持不变。"""
    entries = json.loads((ASSETS / "cs408.json").read_text(encoding="utf-8"))
    cards = json.loads((ASSETS / "cs408_practice.json").read_text(encoding="utf-8"))
    card_by_id = {c["id"]: c for c in cards}
    total = 0
    for subject, name in CS408_SUBJECTS.items():
        es = [dict(e) for e in entries if e["category"] == subject]
        if len(es) != 24:
            raise ValueError(f"408 拆分：{subject} 条目数 {len(es)} != 24")
        cs = []
        for e in es:
            slug = e["tags"][3] if len(e["tags"]) > 3 else e["id"].rsplit(":", 1)[-1]
            e["language"] = subject
            e["tags"] = list(dict.fromkeys([subject, "考研408", slug]))
            card = card_by_id.get(f"knowledge:{e['id']}")
            if card is None:
                raise ValueError(f"408 拆分：缺少练习 {e['id']}")
            card["tags"] = list(dict.fromkeys([subject, "考研408", slug]))
            cs.append(card)
        errs = validate(es, cs)
        if errs:
            for err in errs:
                print(f"[FAIL] {name}: {err}")
            sys.exit(1)
        (ASSETS / f"{name}.json").write_text(
            json.dumps(es, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        (ASSETS / f"{name}_practice.json").write_text(
            json.dumps(cs, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"{name}: entries={len(es)} cards={len(cs)} (language={subject})")
        total += len(es)
    if total != len(entries):
        raise ValueError(f"408 拆分：总条目 {total} != {len(entries)}")


def main():
    all_entries, all_cards = [], []
    for pack in PACKS:
        entries, cards = build_pack(pack)
        errs = validate(entries, cards)
        if errs:
            for e in errs:
                print(f"[FAIL] {pack}: {e}")
            sys.exit(1)
        (ASSETS / f"{pack}.json").write_text(
            json.dumps(entries, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        (ASSETS / f"{pack}_practice.json").write_text(
            json.dumps(cards, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        areas = {}
        for c in cards:
            key = (c["training"]["area"], c["training"]["level"])
            areas[key] = areas.get(key, 0) + 1
        print(f"{pack}: entries={len(entries)} cards={len(cards)}")
        for k in sorted(areas):
            print(f"    {k[0]} / L{k[1]}: {areas[k]}")
        all_entries += entries
        all_cards += cards
    split_cs408()
    added, total = build_profiles()
    print(f"curriculum_profiles: 新增 {added} 条，总计 {total} 条")


if __name__ == "__main__":
    main()
