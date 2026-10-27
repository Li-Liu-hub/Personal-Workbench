"""Shared constants."""

from __future__ import annotations

DOCUMENT_ROLES: tuple[str, ...] = (
    "resume",
    "resume_image",
    "photo",
    "photo_square",
    "photo_avatar",
    "transcript",
    "enrollment_proof",
    "gpa_proof",
    "enrollment_report",
    "cet4",
    "internship_proof",
    "id_front",
    "id_back",
    "other",
)

SENSITIVE_ROLES = {"id_front", "id_back"}

# Labels in the profile markdown. Order matters: the longest / most specific first.
DOCUMENT_LABELS: tuple[tuple[str, str], ...] = (
    ("简历PDF", "resume"),
    ("简历 PDF", "resume"),
    ("简历文件", "resume"),
    ("简历图片", "resume_image"),
    ("方形证件照", "photo_square"),
    ("头像小图", "photo_avatar"),
    ("证件照", "photo"),
    ("成绩单", "transcript"),
    ("在校证明", "enrollment_proof"),
    ("均分证明", "gpa_proof"),
    ("学籍验证报告", "enrollment_report"),
    ("学籍在线验证报告", "enrollment_report"),
    ("CET4成绩单", "cet4"),
    ("CET-4成绩单", "cet4"),
    ("四级成绩单", "cet4"),
    ("实习证明", "internship_proof"),
    ("身份证正面", "id_front"),
    ("身份证反面", "id_back"),
    ("其他附件", "other"),
)
