"""Copy a local Obsidian vault into editable, classified PersonalDesk books.

Originals stay unchanged. Hidden tooling, assistant plugins and trash are excluded.
Only the prepared JSON goes through PersonalDesk's transactional import command.
"""
import argparse
from contextlib import closing
from collections import Counter, defaultdict
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import uuid
from urllib.parse import unquote
import xml.etree.ElementTree as ET
import zipfile

MAX_DOCUMENT_BYTES = 8 * 1024 * 1024


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(64 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def category(relative):
    parts = relative.parts
    name = relative.stem
    top = parts[0]
    if top == "日记":
        return "生活 · 日记", ["生活", "日记", name[:7]]
    if top == "示例公司":
        if "项目一" in parts:
            return "工作 · 项目一", ["工作", "示例公司", "项目一"]
        if "资产负债" in parts:
            return "工作 · 项目二", ["工作", "示例公司", "项目二"]
        return "工作 · 项目三", ["工作", "示例公司", "项目三"]
    if top in ("语法错误集合", "Spring框架源码阅读"):
        return "学习 · Java 与 Spring", ["学习", "Java", top]
    if top == "开源项目拆解":
        return "学习 · 开源项目拆解", ["学习", "开源项目", "Codex"]
    if top == "示例项目一":
        return "项目 · 示例项目一", ["项目", "AI Agent"]
    if top == "示例项目二":
        return "项目 · 示例项目二", ["项目", "示例项目二"]
    if name in ("大模型Api-key", "手机号", "游戏账号", "账号密码"):
        return "私人 · 账号资料", ["私人", "账号资料"]
    if name in ("简历", "面试题库"):
        return "求职 · 简历与面试", ["求职", "面试"]
    if name == "关联对账":
        return "工作 · 项目三", ["工作", "对账"]
    if name.startswith("双线客户渗透率报表"):
        return "工作 · 项目一", ["工作", "项目一"]
    if name == "记账本":
        return "生活 · 记账", ["生活", "记账"]
    if name == "常用指令":
        return "工具 · 常用指令", ["工具", "指令"]
    return "其他笔记", ["待整理"]


def read_text(path):
    with path.open("rb") as source:
        raw = source.read(MAX_DOCUMENT_BYTES + 1)
    if len(raw) > MAX_DOCUMENT_BYTES:
        raise ValueError(f"文档超过 8 MiB {path.name}")
    encodings = ("utf-16",) if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else ("utf-8-sig", "gb18030")
    for encoding in encodings:
        try:
            return raw.decode(encoding)
        except UnicodeDecodeError:
            pass
    raise ValueError(f"无法解码文档 {path.name}")


def extract_word(path):
    # Preserve paragraph/table order; the copied Word original retains full layout.
    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    tag = lambda name: "{" + ns["w"] + "}" + name
    with zipfile.ZipFile(path) as archive:
        if archive.getinfo("word/document.xml").file_size > MAX_DOCUMENT_BYTES:
            raise ValueError(f"Word 正文超过 8 MiB {path.name}")
        root = ET.fromstring(archive.read("word/document.xml"))
        body = root.find("w:body", ns)
        if body is None:
            raise ValueError(f"Word 正文不存在 {path.name}")
        def text(element):
            return "".join((node.text or "") if node.tag == tag("t") else ("\n" if node.tag in (tag("br"), tag("cr")) else "\t") for node in element.iter() if node.tag in (tag("t"), tag("br"), tag("cr"), tag("tab")))
        blocks = []
        for child in body:
            if child.tag == tag("p"):
                value = text(child)
                style = child.find("w:pPr/w:pStyle", ns)
                if style is not None:
                    match = re.search(r"(?:Heading|标题)([1-6])", style.get(tag("val"), ""), re.I)
                    if match:
                        value = "#" * int(match[1]) + " " + value
                blocks.append(value)
            elif child.tag == tag("tbl"):
                rows = [[text(cell).replace("|", "\\|").replace("\n", "<br>") for cell in row.findall("w:tc", ns)] for row in child.findall("w:tr", ns)]
                width = max(map(len, rows), default=0)
                if width:
                    lines = ["| " + " | ".join(row + [""] * (width - len(row))) + " |" for row in rows]
                    lines.insert(1, "| " + " | ".join(["---"] * width) + " |")
                    blocks.append("\n".join(lines))
        return "\n\n".join(blocks)


def locations(vault, data):
    vault, data = vault.resolve(), data.resolve()
    if not vault.is_dir():
        raise ValueError("来源仓库不存在")
    if data.is_relative_to(vault):
        raise ValueError("数据目录不能位于来源仓库内，请选择独立目录，避免重复导入附件副本")
    return vault, data


def prepare(vault, data):
    vault, data = locations(vault, data)

    def included(path):
        return not path.name.startswith(".") and path.name not in ("node_modules", "copilot") and not path.is_symlink() and not getattr(path, "is_junction", lambda: False)() and path.resolve().is_relative_to(vault)

    files = []
    def scan_error(error):
        raise error
    for folder, directories, names in os.walk(vault, followlinks=False, onerror=scan_error):
        base = Path(folder)
        directories[:] = sorted(name for name in directories if included(base / name))
        files.extend(base / name for name in names if included(base / name) and (base / name).is_file())
    files.sort()
    docs = [p for p in files if p.suffix.lower() in (".md", ".markdown", ".txt", ".docx")]
    supported = set(docs) | {p for p in files if p.suffix.lower() in (".png", ".jpg", ".jpeg", ".webp", ".pdf", ".gif", ".svg")}
    hashes = {p: digest(p) for p in supported}
    vault_key = hashlib.sha256(str(vault).casefold().encode()).hexdigest()[:16]
    asset_root = data / "writing-assets" / ("obsidian-" + vault_key)
    copies = {}
    for original in sorted(supported):
        target = asset_root / hashes[original][:16] / original.name
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists() and digest(target) != hashes[original]:
            raise ValueError("附件副本与来源不一致，已停止")
        if not target.exists():
            shutil.copy2(original, target)
        if digest(target) != hashes[original]:
            raise ValueError("附件复制校验失败，已停止")
        copies[original] = target

    unresolved = []
    def resolve(current, target):
        target = unquote(target.split("#", 1)[0]).replace("\\", "/")
        if not target:
            return None
        for base in (current.parent, vault):
            candidate = (base / target).resolve()
            for path in (candidate, candidate.with_suffix(".md") if not candidate.suffix else candidate):
                if path in copies:
                    return copies[path]
        matches = [p for p in copies if p.name == Path(target).name or (not Path(target).suffix and p.stem == Path(target).name)]
        return copies[matches[0]] if len(matches) == 1 else None

    def convert_links(body, current):
        fence = None
        result = []
        for line in body.splitlines(keepends=True):
            marker = re.match(r" {0,3}(`{3,}|~{3,})(.*)", line)
            if marker:
                if fence is None:
                    fence = (marker[1][0], len(marker[1]))
                elif marker[1][0] == fence[0] and len(marker[1]) >= fence[1] and not marker[2].strip():
                    fence = None
                result.append(line)
                continue
            if fence:
                result.append(line)
                continue
            def wiki(match):
                destination, _, alias = match[2].partition("|")
                target = resolve(current, destination)
                label = alias or destination.split("#", 1)[0]
                if target:
                    return f"[{('图片：' if match[1] else '') + label}]({target.as_uri()})"
                unresolved.append({"source": str(current.relative_to(vault)), "target": destination})
                return match[0]
            line = re.sub(r"(!?)\[\[([^]\n]+)\]\]", wiki, line)
            def markdown(match):
                destination = match[3].strip("<>")
                if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", destination) or destination.startswith("#"):
                    return match[0]
                target = resolve(current, destination)
                return f"[{('图片：' if match[1] else '') + match[2]}]({target.as_uri()})" if target else match[0]
            result.append(re.sub(r"(!?)\[([^]\n]*)\]\(([^)\n]+)\)", markdown, line))
        return "".join(result)

    groups = defaultdict(list)
    manifest = []
    for path in docs:
        relative = path.relative_to(vault)
        book, tags = category(relative)
        content = extract_word(path) if path.suffix.lower() == ".docx" else convert_links(read_text(path), path)
        note = "\n\n---\n\n" + f"来源：Obsidian / {relative.as_posix()}\n\n[查看原文件副本]({copies[path].as_uri()})\n"
        source = "obsidian:" + path.as_uri()
        groups[book].append({"source": source, "title": path.stem, "body": content + note, "tags": ["Obsidian", *tags]})
        manifest.append({"source": source, "relative": relative.as_posix(), "book": book, "sha256": hashes[path], "characters": len(content), "original_copy": str(copies[path])})
    books = [{"book": {"id": str(uuid.uuid5(uuid.NAMESPACE_URL, vault.as_uri() + "#" + name)), "title": name, "tags": ["Obsidian", name.split(" · ")[0]]}, "pages": pages} for name, pages in sorted(groups.items())]
    if not all(digest(p) == value for p, value in hashes.items()):
        raise ValueError("源仓库发生变化，请重试")
    return books, {"documents": len(docs), "copied_files": len(copies), "stored_files": len(set(copies.values())), "categories": {k: len(v) for k, v in sorted(groups.items())}, "unresolved_wikilinks": unresolved, "manifest": manifest, "source_hashes_verified": len(hashes), "other_file_types": dict(Counter(p.suffix for p in files if p not in supported))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vault", required=True, type=Path)
    parser.add_argument("--data", required=True, type=Path)
    parser.add_argument("--app", required=True, type=Path)
    args = parser.parse_args()
    vault, data = locations(args.vault, args.data)
    if not vault.is_dir() or not args.app.is_file():
        raise ValueError("来源仓库或程序不存在")
    data.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    report_dir = data / "import-reports" / ("obsidian-" + stamp + "-" + uuid.uuid4().hex[:8])
    report_dir.mkdir(parents=True)
    database = data / "desk.db"
    if database.exists():
        with closing(sqlite3.connect(database)) as source, closing(sqlite3.connect(report_dir / "before-import.db")) as backup:
            source.backup(backup)
            if backup.execute("pragma integrity_check").fetchone()[0] != "ok":
                raise RuntimeError("导入前备份完整性检查失败，已停止")
    books, report = prepare(vault, data)
    bundle = report_dir / "writing-bundle.json"
    bundle.write_text(json.dumps(books, ensure_ascii=False), encoding="utf-8")
    result = subprocess.run([str(args.app.resolve()), "--import-writing-bundle", str(bundle), str(data)], capture_output=True)
    if result.returncode:
        raise RuntimeError("办公台导入失败，原库保留，文档包保留在本地导入记录目录")
    with closing(sqlite3.connect(database)) as db:
        if db.execute("pragma integrity_check").fetchone()[0] != "ok":
            raise RuntimeError("导入后数据库完整性检查失败，导入文档包保留")
        new_sources = {row[0] for row in db.execute("select source from writing_imports")}
        if not all(page["source"] in new_sources for group in books for page in group["pages"]):
            raise RuntimeError("来源核对失败，导入文档包保留")
        report["database_integrity"] = "ok"
        report["imported_sources_verified"] = sum(len(group["pages"]) for group in books)
    bundle.unlink()  # The data is in the database; avoid another plaintext content copy.
    (report_dir / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k not in ("manifest", "unresolved_wikilinks")}, ensure_ascii=False, indent=2))
    print("Unresolved links:", len(report["unresolved_wikilinks"]))
    print("Report:", report_dir)


if __name__ == "__main__":
    main()
