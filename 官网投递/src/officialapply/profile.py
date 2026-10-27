"""Parse the Chinese markdown profile and report what is still missing."""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from typing import Any, Iterable

from officialapply.constants import DOCUMENT_LABELS, SENSITIVE_ROLES


SECTION_ALIASES: dict[str, str] = {
    "基本信息": "basic",
    "个人信息": "basic",
    "求职意向": "job_intent",
    "求职信息": "job_intent",
    "教育经历": "education",
    "教育背景": "education",
    "实习经历": "internships",
    "工作经历": "internships",
    "实习/工作经历": "internships",
    "项目经历": "projects",
    "项目经验": "projects",
    "专业技能": "skills",
    "技能证书": "skills",
    "技能": "skills",
    "获奖荣誉": "awards",
    "获奖情况": "awards",
    "荣誉奖项": "awards",
    "自我评价": "self_evaluation",
    "投递材料": "documents",
    "附件材料": "documents",
    "网申常见问题": "common_questions",
    "常见问题": "common_questions",
    "官网投递偏好": "preferences",
    "投递偏好": "preferences",
    "补充说明": "extra",
    "求职信": "cover_letter",
}

REPEATABLE_SECTIONS = {"education", "internships", "projects"}

BASIC_ALIASES: dict[str, tuple[str, ...]] = {
    "name": ("姓名", "真实姓名", "名字", "申请人姓名", "本人姓名", "中文姓名"),
    "english_name": ("英文名", "英文姓名", "englishname"),
    "gender": ("性别", "gender", "sex"),
    "birth_date": ("出生日期", "出生年月", "生日", "出生时间", "dateofbirth", "birthday", "dob"),
    "phone": ("手机号", "手机号码", "手机", "联系电话", "电话", "联系方式", "本人手机", "mobile", "phone", "tel", "telephone"),
    "email": ("邮箱", "电子邮箱", "邮件", "email", "e-mail", "mail"),
    "ethnicity": ("民族", "ethnicity", "nation"),
    "political_status": ("政治面貌", "politicalstatus"),
    "marital_status": ("婚姻状况", "婚姻状态", "maritalstatus"),
    "height": ("身高", "height"),
    "weight": ("体重", "weight"),
    "id_number": ("身份证号", "身份证号码", "证件号码", "身份证", "idcard", "idnumber"),
    "hukou": ("户籍所在地", "户籍", "户口所在地", "户口", "hukou"),
    "current_city": ("现居城市", "现居住地", "现居地", "当前城市", "所在城市", "居住城市", "现居住城市"),
    "address": ("通讯地址", "联系地址", "详细地址", "家庭地址", "现住址", "住址", "address"),
    "hometown": ("籍贯", "家乡", "hometown", "nativeplace"),
    "emergency_contact_name": ("紧急联系人姓名", "紧急联系人", "emergencycontact"),
    "emergency_contact_phone": ("紧急联系人电话", "紧急联系电话", "emergencyphone"),
    "emergency_contact_relation": ("与本人关系", "紧急联系人关系", "relationship"),
}

JOB_ALIASES: dict[str, tuple[str, ...]] = {
    "expected_position": ("期望岗位", "期望职位", "意向岗位", "求职意向", "目标岗位", "应聘岗位"),
    "expected_city": ("期望城市", "意向城市", "期望工作城市", "期望工作地点", "意向工作地点", "工作城市"),
    "expected_salary": ("期望薪资", "期望薪酬", "期望月薪", "薪资要求", "期望年薪"),
    "available_date": ("到岗时间", "入职时间", "可到岗时间", "最快到岗", "可入职时间", "预计入职时间"),
    "job_type": ("求职类型", "工作性质", "招聘类型", "应聘类型"),
    "accept_relocation": ("是否接受调剂", "是否服从调剂", "接受调剂", "服从调剂", "工作地点调剂"),
    "accept_travel": ("是否接受出差", "接受出差", "是否出差"),
    "accept_overtime": ("是否接受加班", "接受加班"),
    "accept_internship": ("是否接受实习", "接受实习"),
    "accept_shift": ("是否接受倒班", "接受倒班"),
    "accept_remote": ("是否接受远程", "接受远程"),
    "other": ("其他要求", "其他说明"),
}

EDUCATION_ALIASES: dict[str, tuple[str, ...]] = {
    "school": ("学校", "毕业院校", "学校名称", "院校", "就读学校", "学校全称"),
    "major": ("专业", "所学专业", "专业名称", "主修专业"),
    "degree": ("学历", "最高学历", "学历层次", "培养层次"),
    "degree_title": ("学位", "学位类型"),
    "college": ("学院", "院系", "所在学院"),
    "start": ("入学时间", "入学日期", "开始时间", "起始时间"),
    "end": ("毕业时间", "毕业日期", "预计毕业时间", "结束时间"),
    "gpa": ("GPA", "平均学分绩点", "绩点"),
    "average_score": ("平均成绩", "均分", "平均分"),
    "weighted_average": ("加权平均成绩", "加权平均分", "加权均分"),
    "rank": ("专业排名", "班级排名", "年级排名", "排名"),
    "courses": ("主修课程", "主要课程"),
    "full_time": ("学习形式", "是否全日制", "全日制"),
    "student_id": ("学号",),
}

INTERNSHIP_ALIASES: dict[str, tuple[str, ...]] = {
    "company": ("公司", "公司名称", "实习公司", "工作单位", "单位名称"),
    "title": ("职位", "岗位", "实习职位", "担任职位", "职务"),
    "department": ("部门", "所在部门"),
    "start": ("开始时间", "入职时间", "起始时间"),
    "end": ("结束时间", "离职时间"),
    "location": ("工作地点", "实习地点", "城市"),
    "description": ("工作内容", "实习内容", "工作描述", "职责描述", "主要职责", "工作职责", "个人职责"),
    "tech": ("技术栈", "使用技术"),
    "achievements": ("工作成果", "业绩", "成果"),
    "current": ("是否仍在职", "在职状态"),
    "proof": ("证明文件", "实习证明"),
}

PROJECT_ALIASES: dict[str, tuple[str, ...]] = {
    "name": ("项目名称", "项目名"),
    "role": ("项目角色", "担任角色", "项目职责", "角色"),
    "start": ("开始时间", "起始时间"),
    "end": ("结束时间",),
    "background": ("项目背景",),
    "description": ("项目描述", "项目内容", "项目简介"),
    "tech": ("技术栈", "使用技术"),
    "duties": ("个人职责", "主要工作", "职责"),
    "achievements": ("项目成果", "成果", "业绩"),
    "link": ("项目链接", "链接"),
}

SKILL_ALIASES: dict[str, tuple[str, ...]] = {
    "languages": ("编程语言", "开发语言"),
    "java": ("Java", "Java/JVM", "Java基础"),
    "frameworks": ("框架", "技术框架"),
    "databases": ("数据库",),
    "middleware": ("中间件",),
    "frontend": ("前端",),
    "engineering": ("工程与运维", "工程与测试", "运维", "开发工具", "工具"),
    "ai": ("AI 工程化", "人工智能", "AI"),
    "ai_tools": ("AI 辅助开发", "AI 工具"),
    "english": ("英语等级", "英语水平", "外语水平"),
    "certificates": ("证书", "其他证书", "资格证书"),
}

DOCUMENT_EXTRA_ALIASES: tuple[tuple[str, str], ...] = (
    ("简历", "resume"),
    ("个人简历", "resume"),
    ("证件照片", "photo"),
    ("一寸照片", "photo"),
    ("一寸证件照", "photo"),
    ("个人照片", "photo"),
    ("照片", "photo"),
    ("附件", "other"),
)

SENSITIVE_SEMANTIC = {"id_number", "hukou", "address", "emergency_contact_phone"}

REQUIRED_FIELDS: tuple[tuple[str, str, str], ...] = (
    ("basic", "name", "姓名"),
    ("basic", "gender", "性别"),
    ("basic", "birth_date", "出生日期"),
    ("basic", "phone", "手机号"),
    ("basic", "email", "邮箱"),
    ("basic", "current_city", "现居城市"),
    ("education", "school", "学校"),
    ("education", "major", "专业"),
    ("education", "degree", "学历"),
    ("education", "end", "毕业时间"),
    ("job_intent", "expected_position", "期望岗位"),
    ("job_intent", "expected_city", "期望城市"),
    ("job_intent", "available_date", "到岗时间"),
)

RECOMMENDED_FIELDS: tuple[tuple[str, str, str], ...] = (
    ("basic", "ethnicity", "民族"),
    ("basic", "political_status", "政治面貌"),
    ("basic", "hukou", "户籍所在地"),
    ("basic", "address", "通讯地址"),
    ("basic", "id_number", "身份证号"),
    ("education", "degree_title", "学位"),
    ("education", "courses", "主修课程"),
    ("job_intent", "expected_salary", "期望薪资"),
    ("job_intent", "job_type", "求职类型"),
    ("skills", "english", "英语等级"),
)

_BLANK_VALUES = {"", "-", "—", "无", "暂无", "不适用", "n/a", "na", "none"}
_UNCERTAIN_MARKERS = ("待确认", "待补充", "待定", "未知", "？", "?", "todo", "tbd")


def normalize_label(value: str) -> str:
    text = unicodedata.normalize("NFKC", str(value or "")).strip().lower()
    text = re.sub(r"[\s_\-*:：·•]+", "", text)
    return text


def label_core(value: str) -> str:
    text = unicodedata.normalize("NFKC", str(value or ""))
    text = re.sub(r"[（(][^）)]*[）)]", "", text)
    return normalize_label(text)


def is_blank(value: Any) -> bool:
    text = str(value or "").strip()
    if not text:
        return True
    return text.lower() in _BLANK_VALUES


def is_uncertain(value: Any) -> bool:
    text = str(value or "").strip().lower()
    return bool(text) and any(marker in text for marker in _UNCERTAIN_MARKERS)


def split_cities(value: str) -> list[str]:
    return [item.strip() for item in re.split(r"[、,，/;；|]+", str(value or "")) if item.strip()]


@dataclass
class Entry:
    title: str
    fields: dict[str, str] = field(default_factory=dict)
    semantic: dict[str, str] = field(default_factory=dict)

    def get(self, key: str, default: str = "") -> str:
        return str(self.semantic.get(key) or default)


@dataclass
class DocumentInfo:
    role: str
    label: str
    path: Path
    exists: bool

    def as_dict(self) -> dict[str, Any]:
        return {
            "role": self.role,
            "label": self.label,
            "filename": self.path.name,
            "exists": self.exists,
            "sensitive": self.role in SENSITIVE_ROLES,
        }


class Profile:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self.raw = ""
        self.sections: dict[str, dict[str, str]] = {}
        self.entries: dict[str, list[Entry]] = {"education": [], "internships": [], "projects": []}
        self.awards: list[str] = []
        self.body: dict[str, str] = {}
        self.warnings: list[str] = []
        self.exists = self.path.exists()
        if self.exists:
            self.raw = self.path.read_text(encoding="utf-8-sig")
            self._parse(self.raw)
        else:
            self.warnings.append(f"资料文件不存在: {self.path}")
        self._build()

    # ------------------------------------------------------------------ parse
    def _parse(self, text: str) -> None:
        text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
        section = ""
        subsection = ""
        entry_fields: dict[str, str] = {}
        last_target: tuple[str, str, str] | None = None  # (section, scope, field)
        for raw_line in text.splitlines():
            line = raw_line.rstrip()
            if not line.strip():
                last_target = None
                continue
            heading = re.match(r"^(#{1,6})\s+(.*)$", line)
            if heading:
                level = len(heading.group(1))
                title = heading.group(2).strip()
                if level == 2:
                    self._flush_entry(section, subsection, entry_fields)
                    entry_fields = {}
                    subsection = ""
                    section = _section_key(title)
                    self.sections.setdefault(section, {})
                elif level == 3 and section in REPEATABLE_SECTIONS:
                    self._flush_entry(section, subsection, entry_fields)
                    entry_fields = {}
                    subsection = title
                last_target = None
                continue
            if not section:
                continue
            if re.match(r"^\s*>\s?", line):
                continue
            bullet = re.match(r"^\s*[-*+]\s+(.*)$", line)
            if bullet:
                content = bullet.group(1).strip()
                if section == "awards":
                    if content:
                        self.awards.append(content)
                    last_target = None
                    continue
                split = _split_label_value(content)
                if section in REPEATABLE_SECTIONS:
                    if not subsection:
                        subsection = "未命名"
                    if split:
                        label, value = split
                        _put_field(entry_fields, label, value, self.warnings)
                        last_target = (section, "entry", label)
                    else:
                        self.body[section] = _append_body(self.body.get(section, ""), content)
                        last_target = (section, "body", "")
                elif split:
                    label, value = split
                    _put_field(self.sections[section], label, value, self.warnings)
                    last_target = (section, "section", label)
                else:
                    if section in {"self_evaluation", "extra", "cover_letter"}:
                        self.body[section] = _append_body(self.body.get(section, ""), content)
                        last_target = (section, "body", "")
                    else:
                        _put_field(self.sections[section], content, "", self.warnings)
                        last_target = (section, "section", content)
                continue
            if line[:1].isspace() and last_target:
                scope_section, scope, field_name = last_target
                if scope == "entry" and scope_section in REPEATABLE_SECTIONS:
                    current = entry_fields.get(field_name, "")
                    entry_fields[field_name] = _append_body(current, line.strip(), separator=" ")
                elif scope == "section":
                    current = self.sections.get(scope_section, {}).get(field_name, "")
                    self.sections.setdefault(scope_section, {})[field_name] = _append_body(
                        current, line.strip(), separator=" "
                    )
                elif scope == "body":
                    self.body[scope_section] = _append_body(self.body.get(scope_section, ""), line.strip())
                continue
            if section in {"self_evaluation", "extra", "cover_letter"}:
                self.body[section] = _append_body(self.body.get(section, ""), line.strip())
                last_target = (section, "body", "")
            else:
                last_target = None
        self._flush_entry(section, subsection, entry_fields)

    def _flush_entry(self, section: str, title: str, fields: dict[str, str]) -> None:
        if section not in REPEATABLE_SECTIONS or not fields:
            return
        aliases = _aliases_for(section)
        entry = Entry(title=title or "未命名", fields=dict(fields), semantic=_map_labels(fields, aliases))
        self.entries.setdefault(section, []).append(entry)

    # ------------------------------------------------------------------ build
    def _build(self) -> None:
        basic_raw = self.sections.get("basic", {})
        job_raw = self.sections.get("job_intent", {})
        skills_raw = self.sections.get("skills", {})
        self.basic = _map_labels(basic_raw, BASIC_ALIASES)
        self.job_intent = _map_labels(job_raw, JOB_ALIASES)
        self.skills = _map_labels(skills_raw, SKILL_ALIASES)
        self.extra_basic = {key: value for key, value in basic_raw.items() if key not in self.basic}
        self.extra_skills = {key: value for key, value in skills_raw.items() if key not in self.skills}
        self.common_questions = dict(self.sections.get("common_questions", {}))
        self.preferences = dict(self.sections.get("preferences", {}))
        self.cover_letter = (
            self.body.get("cover_letter")
            or self.sections.get("cover_letter", {}).get("内容")
            or self.sections.get("cover_letter", {}).get("求职信")
            or ""
        )
        self.self_evaluation = (
            self.body.get("self_evaluation")
            or self.sections.get("self_evaluation", {}).get("内容")
            or self.sections.get("self_evaluation", {}).get("自我评价")
            or next(iter(self.sections.get("self_evaluation", {}).values()), "")
        )
        self.extra = self.body.get("extra", "")
        self.documents = self._build_documents()

    def _build_documents(self) -> list[DocumentInfo]:
        result: list[DocumentInfo] = []
        aliases = DOCUMENT_LABELS + DOCUMENT_EXTRA_ALIASES
        for label, value in self.sections.get("documents", {}).items():
            if is_blank(value):
                continue
            role = _document_role(label, aliases)
            if not role:
                continue
            path_text = str(value).strip().strip("`").strip()
            if not path_text:
                continue
            path = Path(path_text).expanduser()
            if not path.is_absolute():
                path = (self.path.parent / path).resolve()
            result.append(DocumentInfo(role=role, label=label, path=path, exists=path.exists()))
        return result

    # ---------------------------------------------------------------- access
    def first(self, section: str) -> Entry | None:
        entries = self.entries.get(section) or []
        return entries[0] if entries else None

    def latest(self, section: str) -> Entry | None:
        entries = self.entries.get(section) or []
        return entries[-1] if entries else None

    def education_primary(self) -> dict[str, str]:
        entry = self.first("education")
        return dict(entry.semantic) if entry else {}

    def internship_primary(self) -> dict[str, str]:
        entry = self.first("internships")
        return dict(entry.semantic) if entry else {}

    def project_primary(self) -> dict[str, str]:
        entry = self.first("projects")
        return dict(entry.semantic) if entry else {}

    def value(self, section: str, key: str) -> str:
        if section == "basic":
            return str(self.basic.get(key) or "")
        if section == "job_intent":
            return str(self.job_intent.get(key) or "")
        if section == "education":
            return str(self.education_primary().get(key) or "")
        if section == "internships":
            return str(self.internship_primary().get(key) or "")
        if section == "projects":
            return str(self.project_primary().get(key) or "")
        if section == "skills":
            return str(self.skills.get(key) or "")
        return ""

    def document_path(self, role: str, *, require_exists: bool = True) -> Path | None:
        candidates = [item for item in self.documents if item.role == role]
        if require_exists:
            candidates = [item for item in candidates if item.exists]
        return candidates[0].path if candidates else None

    def resume_path(self) -> Path | None:
        return self.document_path("resume") or self.document_path("resume_image")

    def photo_path(self) -> Path | None:
        for role in ("photo", "photo_square", "photo_avatar"):
            path = self.document_path(role)
            if path:
                return path
        return None

    def document_for_role(self, role: str) -> Path | None:
        return self.document_path(role)

    def age(self) -> int | None:
        birth = parse_date(self.basic.get("birth_date", ""))
        if not birth:
            return None
        today = date.today()
        return today.year - birth.year - ((today.month, today.day) < (birth.month, birth.day))

    def expected_cities(self) -> list[str]:
        return split_cities(str(self.job_intent.get("expected_city") or ""))

    def combined_text(self, section: str) -> str:
        entries = self.entries.get(section) or []
        blocks: list[str] = []
        for entry in entries:
            lines = [f"{entry.title}"]
            for key, value in entry.semantic.items():
                if value:
                    lines.append(f"{_semantic_display(key)}：{value}")
            blocks.append("\n".join(lines))
        return "\n\n".join(blocks)

    def skills_text(self) -> str:
        lines = []
        for key, value in self.skills.items():
            if value:
                lines.append(f"{_semantic_display(key)}：{value}")
        return "\n".join(lines)

    def awards_text(self) -> str:
        return "\n".join(self.awards)

    def questions_text(self) -> str:
        return "\n".join(f"{key}：{value}" for key, value in self.common_questions.items() if not is_blank(value))

    def flat_values(self) -> dict[str, Any]:
        education = self.education_primary()
        internship = self.internship_primary()
        project = self.project_primary()
        values: dict[str, Any] = {}
        values.update({key: value for key, value in self.basic.items() if not is_blank(value)})
        values.update({key: value for key, value in self.job_intent.items() if not is_blank(value)})
        values.update({f"education_{key}": value for key, value in education.items() if not is_blank(value)})
        values.update({f"internship_{key}": value for key, value in internship.items() if not is_blank(value)})
        values.update({f"project_{key}": value for key, value in project.items() if not is_blank(value)})
        values.update({f"skill_{key}": value for key, value in self.skills.items() if not is_blank(value)})
        values["education_entries"] = [
            {"title": entry.title, **entry.semantic} for entry in self.entries.get("education", [])
        ]
        values["internship_entries"] = [
            {"title": entry.title, **entry.semantic} for entry in self.entries.get("internships", [])
        ]
        values["project_entries"] = [
            {"title": entry.title, **entry.semantic} for entry in self.entries.get("projects", [])
        ]
        values["skills_text"] = self.skills_text()
        values["awards_text"] = self.awards_text()
        values["self_evaluation"] = self.self_evaluation
        values["cover_letter"] = self.cover_letter
        values["expected_cities"] = self.expected_cities()
        values["age"] = self.age()
        values["common_questions"] = dict(self.common_questions)
        values["preferences"] = dict(self.preferences)
        return values

    # --------------------------------------------------------------- checks
    def check(self) -> dict[str, Any]:
        missing_required: list[dict[str, str]] = []
        for section, key, label in REQUIRED_FIELDS:
            value = self.value(section, key)
            if is_blank(value):
                missing_required.append({"section": _section_display(section), "field": label, "reason": "空"})
            elif is_uncertain(value):
                missing_required.append({"section": _section_display(section), "field": label, "reason": "待确认"})
        missing_recommended: list[dict[str, str]] = []
        for section, key, label in RECOMMENDED_FIELDS:
            value = self.value(section, key)
            if is_blank(value):
                missing_recommended.append({"section": _section_display(section), "field": label, "reason": "空"})
            elif is_uncertain(value):
                missing_recommended.append({"section": _section_display(section), "field": label, "reason": "待确认"})
        uncertain: list[dict[str, str]] = []
        for section, key, label in (*REQUIRED_FIELDS, *RECOMMENDED_FIELDS):
            value = self.value(section, key)
            if is_uncertain(value):
                uncertain.append({"section": _section_display(section), "field": label, "value": value})
        for group, display in (
            (self.basic, _BASIC_DISPLAY),
            (self.job_intent, _JOB_DISPLAY),
            (self.skills, _SEMANTIC_DISPLAY),
        ):
            for key, value in group.items():
                if is_uncertain(value):
                    item = {"field": display.get(key, key), "value": str(value)}
                    if item not in uncertain:
                        uncertain.append(item)
        documents = [item.as_dict() for item in self.documents]
        document_missing: list[dict[str, str]] = []
        seen_document_roles: set[str] = set()
        resume = self.resume_path()
        if not resume:
            document_missing.append({"role": "resume", "label": "简历PDF", "reason": "未配置或文件不存在"})
            seen_document_roles.add("resume")
        if not self.photo_path():
            document_missing.append({"role": "photo", "label": "证件照", "reason": "未配置或文件不存在"})
            seen_document_roles.add("photo")
        for item in self.documents:
            if not item.exists and item.role not in seen_document_roles:
                document_missing.append({
                    "role": item.role,
                    "label": item.label,
                    "reason": f"文件不存在: {item.path.name}",
                })
                seen_document_roles.add(item.role)
        summary = self.summary()
        return {
            "path": str(self.path),
            "exists": self.exists,
            "summary": summary,
            "missing_required": missing_required,
            "missing_recommended": missing_recommended,
            "uncertain": uncertain,
            "documents": documents,
            "missing_documents": document_missing,
            "warnings": list(self.warnings),
            "complete_required": not missing_required and not document_missing,
        }

    def summary(self) -> dict[str, Any]:
        education = self.education_primary()
        documents_ready = sum(1 for item in self.documents if item.exists)
        return {
            "name": str(self.basic.get("name") or ""),
            "gender": str(self.basic.get("gender") or ""),
            "phone": mask_phone(str(self.basic.get("phone") or "")),
            "email": mask_email(str(self.basic.get("email") or "")),
            "school": str(education.get("school") or ""),
            "major": str(education.get("major") or ""),
            "degree": str(education.get("degree") or ""),
            "graduation": str(education.get("end") or ""),
            "expected_position": str(self.job_intent.get("expected_position") or ""),
            "expected_cities": self.expected_cities(),
            "documents_ready": documents_ready,
            "documents_total": len(self.documents),
        }


def load_profile(config: dict[str, Any]) -> Profile:
    return Profile(config["_profile_path"])


def parse_date(value: str) -> date | None:
    text = str(value or "").strip()
    if not text:
        return None
    match = re.search(r"(\d{4})\s*[-/.年]\s*(\d{1,2})(?:\s*[-/.月]\s*(\d{1,2}))?", text)
    if not match:
        return None
    year, month, day = int(match.group(1)), int(match.group(2)), int(match.group(3) or 1)
    try:
        return date(year, month, day)
    except ValueError:
        return None


def format_month(value: str) -> str:
    parsed = parse_date(value)
    return parsed.strftime("%Y-%m") if parsed else str(value or "")


def format_date(value: str) -> str:
    parsed = parse_date(value)
    return parsed.strftime("%Y-%m-%d") if parsed else str(value or "")


def mask_phone(value: str) -> str:
    digits = re.sub(r"\D", "", value)
    if len(digits) < 7:
        return value
    return f"{digits[:3]}****{digits[-4:]}"


def mask_email(value: str) -> str:
    if "@" not in value:
        return value
    local, _, domain = value.partition("@")
    if len(local) <= 2:
        masked = local[:1] + "*"
    else:
        masked = local[:2] + "***" + local[-2:]
    return f"{masked}@{domain}"


def mask_id(value: str) -> str:
    text = str(value or "")
    if len(text) < 8:
        return "***"
    return f"{text[:3]}********{text[-2:]}"


def _section_key(title: str) -> str:
    clean = re.sub(r"[（(].*?[）)]", "", title).strip()
    if clean in SECTION_ALIASES:
        return SECTION_ALIASES[clean]
    return f"other:{normalize_label(clean)}"


def _aliases_for(section: str) -> dict[str, tuple[str, ...]]:
    if section == "education":
        return EDUCATION_ALIASES
    if section == "internships":
        return INTERNSHIP_ALIASES
    if section == "projects":
        return PROJECT_ALIASES
    return {}


def _map_labels(fields: dict[str, str], aliases: dict[str, tuple[str, ...]]) -> dict[str, str]:
    result: dict[str, str] = {}
    for label, value in fields.items():
        key = _match_alias(label, aliases)
        if key and key not in result:
            result[key] = value
    return result


def _match_alias(label: str, aliases: dict[str, tuple[str, ...]]) -> str | None:
    normalized = normalize_label(label)
    core = label_core(label)
    for key, values in aliases.items():
        for alias in values:
            alias_norm = normalize_label(alias)
            if normalized == alias_norm or core == alias_norm:
                return key
    return None


def _split_label_value(text: str) -> tuple[str, str] | None:
    match = re.match(r"^([^：:]{1,60})[：:]\s*(.*)$", text)
    if not match:
        return None
    label = match.group(1).strip()
    if label.lower().startswith(("http://", "https://", "www.")):
        return None
    return label, match.group(2).strip()


def _put_field(target: dict[str, str], label: str, value: str, warnings: list[str]) -> None:
    if label in target and target[label] != value:
        warnings.append(f"字段“{label}”重复，后一个值覆盖前一个")
    target[label] = value


def _append_body(current: str, addition: str, *, separator: str = "\n") -> str:
    current = current.strip()
    addition = addition.strip()
    if not addition:
        return current
    return f"{current}{separator}{addition}" if current else addition


def _document_role(label: str, aliases: Iterable[tuple[str, str]]) -> str | None:
    normalized = normalize_label(label)
    core = label_core(label)
    for alias, role in aliases:
        alias_norm = normalize_label(alias)
        if normalized == alias_norm or core == alias_norm:
            return role
    return None


_BASIC_DISPLAY = {
    "name": "姓名",
    "english_name": "英文名",
    "gender": "性别",
    "birth_date": "出生日期",
    "phone": "手机号",
    "email": "邮箱",
    "ethnicity": "民族",
    "political_status": "政治面貌",
    "marital_status": "婚姻状况",
    "height": "身高",
    "weight": "体重",
    "id_number": "身份证号",
    "hukou": "户籍所在地",
    "current_city": "现居城市",
    "address": "通讯地址",
    "hometown": "籍贯",
    "emergency_contact_name": "紧急联系人姓名",
    "emergency_contact_phone": "紧急联系人电话",
    "emergency_contact_relation": "紧急联系人关系",
}

_JOB_DISPLAY = {
    "expected_position": "期望岗位",
    "expected_city": "期望城市",
    "expected_salary": "期望薪资",
    "available_date": "到岗时间",
    "job_type": "求职类型",
    "accept_relocation": "是否接受调剂",
    "accept_travel": "是否接受出差",
    "accept_overtime": "是否接受加班",
    "accept_internship": "是否接受实习",
    "accept_shift": "是否接受倒班",
    "accept_remote": "是否接受远程",
    "other": "其他要求",
}

_SEMANTIC_DISPLAY = {
    "school": "学校",
    "major": "专业",
    "degree": "学历",
    "degree_title": "学位",
    "college": "学院",
    "start": "开始时间",
    "end": "结束时间",
    "gpa": "GPA",
    "average_score": "平均成绩",
    "weighted_average": "加权平均成绩",
    "rank": "排名",
    "courses": "主修课程",
    "full_time": "学习形式",
    "student_id": "学号",
    "company": "公司",
    "title": "职位",
    "department": "部门",
    "location": "工作地点",
    "description": "工作内容",
    "tech": "技术栈",
    "achievements": "成果",
    "current": "是否仍在职",
    "proof": "证明文件",
    "name": "项目名称",
    "role": "角色",
    "background": "项目背景",
    "duties": "个人职责",
    "link": "项目链接",
    "languages": "编程语言",
    "java": "Java",
    "frameworks": "框架",
    "databases": "数据库",
    "middleware": "中间件",
    "frontend": "前端",
    "engineering": "工程与运维",
    "ai": "AI 工程化",
    "ai_tools": "AI 辅助开发",
    "english": "英语等级",
    "certificates": "证书",
}


def _semantic_display(key: str) -> str:
    return _SEMANTIC_DISPLAY.get(key, key)


def _section_display(section: str) -> str:
    return {
        "basic": "基本信息",
        "job_intent": "求职意向",
        "education": "教育经历",
        "internships": "实习经历",
        "projects": "项目经历",
        "skills": "专业技能",
    }.get(section, section)
