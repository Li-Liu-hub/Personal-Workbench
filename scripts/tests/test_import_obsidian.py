"""Temporary fixtures only; never open a personal vault or database."""
import hashlib
import contextlib
import io
import importlib.util
from pathlib import Path
import tempfile
import json
import sqlite3
import sys
import unittest
from unittest import mock
import zipfile

SPEC = importlib.util.spec_from_file_location("import_obsidian", Path(__file__).parents[1] / "import-obsidian.py")
importer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(importer)


class ObsidianImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.vault = self.root / "vault"
        self.data = self.root / "data"
        self.vault.mkdir()

    def note(self, name, body):
        path = self.vault / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body.encode("utf-8"))
        return path

    def pages(self, books):
        return [page for group in books for page in group["pages"]]

    def test_nested_output_is_rejected_before_creating_any_files(self):
        self.note("note.md", "source")
        for output in (self.vault, self.vault / "nested/data"):
            with self.assertRaisesRegex(ValueError, "数据目录"):
                importer.prepare(self.vault, output)
        self.assertEqual([p.name for p in self.vault.iterdir()], ["note.md"])

    def test_replay_has_stable_ids_sources_and_copies_and_keeps_originals(self):
        source = self.note("日记/2026-10-24.md", "日记正文")
        before = source.read_bytes()
        first, report = importer.prepare(self.vault, self.data)
        second, replay = importer.prepare(self.vault, self.data)
        self.assertEqual(first, second)
        self.assertEqual(report["documents"], 1)
        self.assertEqual(report["stored_files"], replay["stored_files"])
        self.assertEqual(source.read_bytes(), before)
        self.assertIn("生活 · 日记", report["categories"])
        self.assertTrue(Path(report["manifest"][0]["original_copy"]).is_file())

    def test_hidden_tooling_and_dependency_trees_are_pruned(self):
        for name in [".obsidian/config.md", ".trash/deleted.md", "node_modules/pkg/note.md", "copilot/chat.md"]:
            self.note(name, "excluded")
        self.note("keep.md", "keep")
        books, report = importer.prepare(self.vault, self.data)
        self.assertEqual(report["documents"], 1)
        self.assertEqual(self.pages(books)[0]["title"], "keep")

    def test_links_resolve_relative_paths_and_ambiguous_names_stay_unresolved(self):
        self.note("a/same.md", "A")
        self.note("b/same.md", "B")
        self.note("main.md", "[[a/same|精确]]\n[[same]]\n[链接](b/same.md)\n")
        books, report = importer.prepare(self.vault, self.data)
        body = next(p["body"] for p in self.pages(books) if p["title"] == "main")
        self.assertIn("[精确](file:", body)
        self.assertIn("[链接](file:", body)
        self.assertIn("[[same]]", body)
        self.assertEqual(report["unresolved_wikilinks"][0]["target"], "same")

    def test_fenced_examples_preserve_shorter_fences_and_internal_wikilinks(self):
        self.note("target.md", "target")
        self.note("main.md", "````markdown\n```\n[[target]]\n```\n````\n[[target]]\n")
        books, _ = importer.prepare(self.vault, self.data)
        body = next(p["body"] for p in self.pages(books) if p["title"] == "main")
        self.assertIn("````markdown\n```\n[[target]]\n```\n````", body)
        self.assertIn("[target](file:", body)

    def test_text_encodings_are_explicit_and_invalid_or_large_text_fails(self):
        path = self.vault / "text.txt"
        for encoding in ("utf-8-sig", "utf-16", "gb18030"):
            path.write_bytes("中文正文".encode(encoding))
            self.assertEqual(importer.read_text(path), "中文正文")
        path.write_bytes(b"\xff")
        with self.assertRaises(ValueError):
            importer.read_text(path)
        path.write_bytes(b"x" * 9)
        with mock.patch.object(importer, "MAX_DOCUMENT_BYTES", 8):
            with self.assertRaisesRegex(ValueError, "8 MiB"):
                importer.read_text(path)

    def test_word_paragraphs_and_tables_keep_order_and_escape_cell_delimiters(self):
        path = self.vault / "example.docx"
        xml = '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>标题</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>a|b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>结尾</w:t></w:r></w:p></w:body></w:document>'
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("word/document.xml", xml)
        body = importer.extract_word(path)
        self.assertTrue(body.startswith("## 标题"))
        self.assertIn(r"a\|b", body)
        self.assertTrue(body.endswith("结尾"))
        with mock.patch.object(importer, "MAX_DOCUMENT_BYTES", 8):
            with self.assertRaisesRegex(ValueError, "Word 正文"):
                importer.extract_word(path)

    def test_corrupt_attachment_copy_is_rejected_and_not_overwritten(self):
        self.note("note.md", "source")
        _, report = importer.prepare(self.vault, self.data)
        copy = Path(report["manifest"][0]["original_copy"])
        copy.write_bytes(b"user changed copy")
        with self.assertRaisesRegex(ValueError, "附件副本"):
            importer.prepare(self.vault, self.data)
        self.assertEqual(copy.read_bytes(), b"user changed copy")

    def test_digest_streams_large_files_without_read_bytes(self):
        path = self.note("note.md", "x" * 150000)
        with mock.patch.object(Path, "read_bytes", side_effect=AssertionError("must stream")):
            self.assertEqual(importer.digest(path), hashlib.sha256(b"x" * 150000).hexdigest())

    def test_directory_links_are_pruned_without_descending_into_them(self):
        self.note("keep.md", "keep")
        self.note("linked/secret.md", "outside")
        original = Path.is_symlink
        with mock.patch.object(Path, "is_symlink", lambda p: p.name == "linked" or original(p)):
            books, report = importer.prepare(self.vault, self.data)
        self.assertEqual(report["documents"], 1)
        self.assertEqual(self.pages(books)[0]["title"], "keep")

    def run_main(self, process):
        app = self.root / "fixture.exe"
        app.write_bytes(b"fixture, never executed")
        argv = ["import-obsidian.py", "--vault", str(self.vault), "--data", str(self.data), "--app", str(app)]
        with mock.patch.object(sys, "argv", argv), mock.patch.object(importer.subprocess, "run", side_effect=process), contextlib.redirect_stdout(io.StringIO()):
            importer.main()

    def test_main_verifies_sources_before_removing_bundle_and_supports_fast_replays(self):
        self.note("note.md", "note")
        def process(args, **kwargs):
            books = json.loads(Path(args[2]).read_text(encoding="utf-8"))
            with contextlib.closing(sqlite3.connect(self.data / "desk.db")) as db, db:
                db.execute("create table if not exists writing_imports(source text primary key)")
                db.executemany("insert or ignore into writing_imports values(?)", [(p["source"],) for p in self.pages(books)])
            return mock.Mock(returncode=0)
        self.run_main(process)
        self.run_main(process)
        reports = list((self.data / "import-reports").iterdir())
        self.assertEqual(len(reports), 2)
        self.assertTrue(any((p / "before-import.db").is_file() for p in reports))
        for folder in reports:
            report = json.loads((folder / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["database_integrity"], "ok")
            self.assertEqual(report["imported_sources_verified"], 1)
            self.assertFalse((folder / "writing-bundle.json").exists())

    def test_main_failed_process_keeps_prepared_bundle_for_retry(self):
        self.note("note.md", "note")
        with self.assertRaisesRegex(RuntimeError, "导入失败"):
            self.run_main(lambda *args, **kwargs: mock.Mock(returncode=1))
        self.assertEqual(len(list(self.data.rglob("writing-bundle.json"))), 1)

    def test_main_missing_import_sources_keeps_bundle_even_when_process_reports_success(self):
        self.note("note.md", "note")
        def process(*args, **kwargs):
            with contextlib.closing(sqlite3.connect(self.data / "desk.db")) as db, db:
                db.execute("create table writing_imports(source text)")
            return mock.Mock(returncode=0)
        with self.assertRaisesRegex(RuntimeError, "来源核对"):
            self.run_main(process)
        self.assertEqual(len(list(self.data.rglob("writing-bundle.json"))), 1)


if __name__ == "__main__":
    unittest.main()
