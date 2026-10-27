"""Map inspected official-site fields onto the local markdown profile."""

from __future__ import annotations

import re
from typing import Any, Iterable

from officialapply.constants import SENSITIVE_ROLES
from officialapply.profile import Profile, is_blank, is_uncertain, normalize_label, parse_date, split_cities

SEMANTIC_DISPLAY: dict[str, str] = {
    "name": "姓名",
    "english_name": "英文名",
    "gender": "性别",
    "birth_date": "出生日期",
    "age": "年龄",
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
    "emergency_contact_name": "紧急联系人",
    "emergency_contact_phone": "紧急联系人电话",
    "emergency_contact_relation": "紧急联系人关系",
    "expected_position": "期望岗位",
    "applied_position": "应聘岗位",
    "applied_company": "应聘公司",
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
    "education_school": "学校",
    "education_college": "学院",
    "education_major": "专业",
    "education_degree": "学历",
    "education_degree_title": "学位",
    "education_start": "入学时间",
    "education_end": "毕业时间",
    "education_gpa": "GPA",
    "education_average_score": "平均成绩",
    "education_weighted_average": "加权平均成绩",
    "education_rank": "专业排名",
    "education_courses": "主修课程",
    "education_student_id": "学号",
    "education_full_time": "学习形式",
    "internship_company": "实习公司",
    "internship_title": "实习职位",
    "internship_department": "实习部门",
    "internship_start": "实习开始时间",
    "internship_end": "实习结束时间",
    "internship_location": "实习地点",
    "internship_description": "实习工作内容",
    "internship_tech": "实习技术栈",
    "internship_achievements": "实习成果",
    "internship_current": "是否仍在职",
    "project_name": "项目名称",
    "project_role": "项目角色",
    "project_start": "项目开始时间",
    "project_end": "项目结束时间",
    "project_background": "项目背景",
    "project_description": "项目描述",
    "project_duties": "项目职责",
    "project_tech": "项目技术栈",
    "project_achievements": "项目成果",
    "project_link": "项目链接",
    "skill_english": "英语等级",
    "skill_english_score": "英语分数",
    "skill_certificates": "证书",
    "skills_text": "专业技能",
    "awards_text": "获奖荣誉",
    "projects_text": "项目经历",
    "internships_text": "实习经历",
    "self_evaluation": "自我评价",
    "cover_letter": "求职信",
    "extra_answer": "资料补充",
}

for _base, _base_label in (("education", "教育"), ("internship", "实习"), ("project", "项目")):
    for _side, _side_label in (("start", "开始"), ("end", "结束")):
        for _part, _part_label in (("year", "年"), ("month", "月"), ("day", "日")):
            SEMANTIC_DISPLAY[f"{_base}_{_side}_{_part}"] = f"{_base_label}{_side_label}{_part_label}"

SENSITIVE_SEMANTICS = {
    "otp",
    "id_number",
    "hukou",
    "address",
    "hometown",
    "emergency_contact_name",
    "emergency_contact_phone",
    "emergency_contact_relation",
}

_YES = {"是", "有", "接受", "同意", "愿意", "服从", "可接受", "true", "1", "yes", "y"}
_NO = {"否", "无", "不接受", "不同意", "不愿意", "不服从", "false", "0", "no", "n"}
_PLACEHOLDER_VALUES = {"", "0", "-1", "请选择", "请选择...", "请输入", "select", "none", "null"}

_MONTH_SEMANTICS = {
    "education_start",
    "education_end",
    "internship_start",
    "internship_end",
    "project_start",
    "project_end",
}

_DATE_PART_RE = re.compile(r"^(education|internship|project)_(start|end)_(year|month|day)$")

_DEFAULT_IGNORE_SECTIONS = (
    r"家庭(成员|状况|信息|关系)?",
    r"亲属|亲友|社会关系",
    r"推荐人|内推人|证明人",
)

_LOGIN_SECTION_RE = re.compile(r"登录|注册|验证码|密码|账号登录|手机号登录|邮箱登录|sign\s*in|log\s*in", re.I)

_DATE_PART_LABELS = {
    "year": re.compile(r"^(?:请选择|选择)?(?:年|年份|入学年份|毕业年份|yyyy|yy)$", re.I),
    "month": re.compile(r"^(?:请选择|选择)?(?:月|月份|mm)$", re.I),
    "day": re.compile(r"^(?:请选择|选择)?(?:日|dd)$", re.I),
}


def plan_mapping(
    profile: Profile,
    page: dict[str, Any],
    config: dict[str, Any],
    *,
    overrides: dict[str, str] | None = None,
    job_title: str = "",
    job_company: str = "",
    accept_terms: bool = False,
    include_uploaded: bool = True,
    template: dict[str, Any] | None = None,
) -> dict[str, Any]:
    apply_cfg = config["apply"]
    fields = list(page.get("fields") or [])
    posting = page.get("job_posting") or {}
    title = str(job_title or posting.get("title") or "").strip()
    company = str(job_company or posting.get("company") or "").strip()
    override_map = {str(key): str(value) for key, value in (overrides or {}).items()}
    normalized_overrides = {
        normalize_label(key): value for key, value in override_map.items() if normalize_label(key)
    }
    actions: list[dict[str, Any]] = []
    uploads: list[dict[str, Any]] = []
    unknown_required: list[dict[str, Any]] = []
    missing_known: list[dict[str, Any]] = []
    optional_unmapped: list[dict[str, Any]] = []
    skipped: list[dict[str, Any]] = []
    consent_pending: list[dict[str, Any]] = []
    warnings: list[str] = []
    mapped_semantics: dict[tuple[str, str], str] = {}
    claimed_override_ids: set[str] = set()
    semantic_override_used: set[str] = set()
    auto_roles = set(str(item) for item in apply_cfg.get("auto_upload_documents") or [])
    fill_sensitive = bool(apply_cfg.get("fill_sensitive_fields", True))
    ignore_patterns = _ignore_section_patterns(template)
    date_parts = _date_part_semantics(fields, template)

    for field in fields:
        if not isinstance(field, dict):
            continue
        field_id = str(field.get("id") or "")
        if not field_id:
            continue
        label = _field_label(field)
        section = _field_section(field)
        if field.get("hidden"):
            continue
        if _is_login_field(field):
            skipped.append(_brief(field, "login_field", "登录/验证码弹窗字段，需本人操作"))
            continue
        ignore_reason = _ignored_section(section, ignore_patterns)
        if ignore_reason:
            if bool(field.get("required")) and not _field_has_value(field) and str(field.get("kind") or "") != "file":
                unknown_required.append(_unknown(field, ignore_reason))
            else:
                skipped.append(_brief(field, "ignored_section", ignore_reason))
            continue
        if field.get("disabled"):
            skipped.append(_brief(field, "disabled", "字段被禁用"))
            continue
        if field.get("readonly") and field.get("kind") != "radio":
            skipped.append(_brief(field, "readonly", "页面只读字段"))
            continue

        if _is_consent(field):
            if field.get("checked"):
                skipped.append(_brief(field, "already_checked", "同意项已勾选"))
            elif accept_terms or apply_cfg.get("accept_terms_default"):
                actions.append({
                    "id": field_id,
                    "semantic": "consent",
                    "label": "同意/隐私条款",
                    "screen_label": label,
                    "kind": "checkbox",
                    "value": "是",
                    "alternatives": ["true", "是", "同意"],
                    "custom_select": False,
                    "source": "accept_terms",
                    "required": bool(field.get("required")),
                    "current": "",
                    "already_correct": False,
                    "will_overwrite": False,
                })
            else:
                consent_pending.append({
                    "id": field_id,
                    "label": label,
                    "required": bool(field.get("required")),
                    "reason": "需要本人确认条款后才能提交",
                })
            continue

        if str(field.get("kind") or "") == "file":
            upload = _plan_upload(field, profile, auto_roles, include_uploaded)
            if upload.get("status") == "planned":
                uploads.append(upload)
            else:
                skipped.append(_brief(field, str(upload.get("reason") or "upload_skipped"), str(upload.get("message") or "")))
            already_uploaded = upload.get("reason") == "already_uploaded"
            if bool(field.get("required")) and upload.get("status") != "planned" and not already_uploaded:
                unknown_required.append(_unknown(field, str(upload.get("message") or "需要上传附件")))
            elif upload.get("status") != "planned" and not already_uploaded:
                optional_unmapped.append(_brief(field, "file", "附件字段未自动上传"))
            continue

        semantic = date_parts.get(field_id) or detect_semantic(field)
        source = "profile"
        explicit = ""

        override_value = None
        if field_id in override_map:
            override_value = override_map[field_id]
            explicit = field_id
            claimed_override_ids.add(field_id)
        elif semantic and semantic in override_map:
            override_value = override_map[semantic]
            explicit = semantic
            semantic_override_used.add(semantic)
        else:
            label_key = normalize_label(label)
            if label_key and label_key in normalized_overrides:
                override_value = normalized_overrides[label_key]
                explicit = label_key

        if override_value is not None:
            source = "explicit"
        else:
            if not semantic:
                fallback = _extra_answer(profile, label)
                if fallback:
                    semantic = "extra_answer"
                    override_value = fallback
                    source = "common_questions"
            if not semantic:
                if _field_has_value(field):
                    skipped.append(_brief(field, "unmapped_filled", "页面已有内容但未匹配到资料"))
                else:
                    optional_unmapped.append(_brief(field, str(field.get("kind") or "input"), "未匹配到资料字段"))
                if bool(field.get("required")) and not _field_has_value(field):
                    unknown_required.append(_unknown(field, "必填字段未匹配到资料"))
                continue

        if source != "explicit":
            value = _resolve_value(profile, semantic, title, company)
            if is_blank(value):
                fallback = _extra_answer(profile, label)
                if fallback:
                    value = fallback
                    source = "common_questions"
            if is_blank(value):
                missing_known.append({
                    "id": field_id,
                    "semantic": semantic,
                    "label": SEMANTIC_DISPLAY.get(semantic, label),
                    "screen_label": label,
                    "required": bool(field.get("required")),
                })
                if bool(field.get("required")):
                    unknown_required.append(_unknown(field, f"资料中没有“{SEMANTIC_DISPLAY.get(semantic, label)}”"))
                continue
        else:
            value = str(override_value or "")
            if is_blank(value):
                continue

        if is_uncertain(value):
            missing_known.append({
                "id": field_id,
                "semantic": semantic,
                "label": SEMANTIC_DISPLAY.get(semantic, label),
                "screen_label": label,
                "required": bool(field.get("required")),
                "reason": "资料中该字段仍待确认",
            })
            if bool(field.get("required")):
                unknown_required.append(_unknown(field, f"资料中“{SEMANTIC_DISPLAY.get(semantic, label)}”仍待确认，未填写"))
            continue

        if semantic in SENSITIVE_SEMANTICS and not fill_sensitive and source != "explicit":
            skipped.append(_brief(field, "sensitive_skipped", "按配置不自动填写敏感信息"))
            if bool(field.get("required")):
                unknown_required.append(_unknown(field, "敏感信息自动填写已关闭"))
            continue

        primary, alternatives = _adapt_value(semantic, value, field)
        if not primary:
            if bool(field.get("required")):
                unknown_required.append(_unknown(field, "没有可用的字段值"))
            continue
        scope = normalize_label(section)
        semantic_key = (semantic, scope)
        if semantic_key in mapped_semantics:
            skipped.append(_brief(
                field,
                "duplicate_semantic",
                f"“{SEMANTIC_DISPLAY.get(semantic, semantic)}”在本区块已映射到字段 {mapped_semantics[semantic_key]}",
            ))
            warnings.append(
                f"“{section or '当前页面'}”存在多个“{SEMANTIC_DISPLAY.get(semantic, semantic)}”字段，默认只填第一个"
            )
            continue
        if semantic != "extra_answer":
            mapped_semantics[semantic_key] = field_id

        max_length = int(field.get("max_length") or 0)
        if max_length > 0 and len(primary) > max_length and field.get("kind") in {"input", "textarea", "contenteditable"}:
            warnings.append(f"“{SEMANTIC_DISPLAY.get(semantic, label)}”超过页面 {max_length} 字限制，已截断")
            primary = primary[:max_length].rstrip()
            alternatives = [item[:max_length].rstrip() for item in alternatives if item]

        current = _current_value(field)
        already_correct = _matches_value(current, primary, alternatives)
        action = {
            "id": field_id,
            "semantic": semantic,
            "label": SEMANTIC_DISPLAY.get(semantic, label),
            "screen_label": label,
            "section": section,
            "group_label": str(field.get("group_label") or ""),
            "kind": "custom_select" if field.get("custom_select") else str(field.get("kind") or "input"),
            "input_type": str(field.get("input_type") or ""),
            "value": primary,
            "alternatives": alternatives,
            "custom_select": bool(field.get("custom_select")),
            "searchable": bool(field.get("searchable")),
            "source": source,
            "required": bool(field.get("required")),
            "current": current,
            "has_current_value": _field_has_value(field),
            "already_correct": already_correct,
            "will_overwrite": bool(current.strip()) and not already_correct,
            "options": [
                str(option.get("label") or option.get("value") or "")
                for option in (field.get("options") or [])[:30]
                if isinstance(option, dict)
            ],
        }
        if explicit and source == "explicit":
            action["explicit"] = explicit
        actions.append(action)

    remaining_overrides = [
        key for key in override_map
        if key not in claimed_override_ids and key not in semantic_override_used
        and normalize_label(key) not in {normalize_label(item.get("screen_label") or "") for item in actions}
    ]
    if remaining_overrides:
        warnings.append(f"以下显式赋值未匹配到页面字段: {', '.join(remaining_overrides[:10])}")

    if consent_pending:
        warnings.append("页面存在待确认的同意/隐私条款，提交前需要 accept_terms=true")
    if unknown_required:
        warnings.append(f"有 {len(unknown_required)} 个必填字段未处理，按配置会阻止提交")

    return {
        "url": str(page.get("url") or ""),
        "page_title": str(page.get("title") or ""),
        "template": template or {},
        "job": {"title": title, "company": company},
        "actions": actions,
        "uploads": uploads,
        "unknown_required": _dedupe(unknown_required),
        "missing_known": _dedupe(missing_known),
        "optional_unmapped": _dedupe(optional_unmapped)[:100],
        "skipped": _dedupe(skipped)[:100],
        "consent_pending": consent_pending,
        "signals": page.get("signals") or {},
        "warnings": _dedupe_strings(warnings),
        "summary": {
            "field_count": len(fields),
            "action_count": len(actions),
            "upload_count": len(uploads),
            "unknown_required_count": len(_dedupe(unknown_required)),
            "missing_known_count": len(_dedupe(missing_known)),
            "consent_pending_count": len(consent_pending),
            "cannot_submit": bool(consent_pending) or bool(
                apply_cfg.get("require_no_unknown_required", True) and _dedupe(unknown_required)
            ),
        },
    }


def detect_semantic(field: dict[str, Any]) -> str | None:
    label = str(field.get("label") or "")
    aria = str(field.get("aria_label") or "")
    placeholder = str(field.get("placeholder") or "")
    name = _humanize_name(str(field.get("name") or ""))
    context = str(field.get("section") or field.get("context") or "")
    text = " ".join(part for part in (label, aria, placeholder, name) if part)
    n = normalize_label(text)
    ln = normalize_label(label)
    cn = normalize_label(context)
    if not n:
        return None
    ctx = _context_kind(cn)

    if "紧急联系人" in n or "emergencycontact" in n or "emergency" in n:
        if re.search(r"电话|手机|phone|mobile|tel|number", n):
            return "emergency_contact_phone"
        if re.search(r"关系|relation|relationship", n):
            return "emergency_contact_relation"
        return "emergency_contact_name"

    if re.search(r"验证码|verificationcode|securitycode|短信码", n):
        return None

    if re.search(r"电子邮箱|邮箱|电子邮件|email|e-mail|mailaddress|emailaddress", n):
        return "email"
    if re.search(r"手机|联系电话|联系方式|电话号码|mobile|phone|telephone|contactnumber|cellphone", n):
        return "phone"
    if re.search(r"身份证|证件号码|证件号|idcard|idnumber|nationalid|identitycard|identitynumber", n):
        return "id_number"

    if re.search(r"英文名|英文姓名|英文名字|englishname|preferredname|nameinenglish", n):
        return "english_name"
    if re.search(r"姓名|真实姓名|申请人姓名|本人姓名|fullname|yourname|applicantname|chinesename", n):
        if not re.search(r"公司|学校|项目|文件|用户名|昵称|联系人|用户名|名称", label):
            return "name"
    if ln in {"name", "full name", "applicant name", "your name"} or n in {"name", "fullname", "applicantname", "yourname"}:
        return "name"

    if re.search(r"性别|gender|^sex$|\bsex\b", n):
        return "gender"
    if re.search(r"出生日期|出生年月|出生时间|生日|dateofbirth|birthdate|birthday|dob", n):
        return "birth_date"
    if re.search(r"年龄|age", n) and not re.search(r"年龄要求|年龄范围", n):
        return "age"
    if re.search(r"民族|ethnicity|ethnicgroup|nation", n) and not re.search(r"国籍|nationality", n):
        return "ethnicity"
    if re.search(r"政治面貌|politicalstatus", n):
        return "political_status"
    if re.search(r"婚姻状况|婚姻状态|maritalstatus|marriage", n):
        return "marital_status"
    if re.search(r"身高|height", n):
        return "height"
    if re.search(r"体重|weight", n):
        return "weight"

    if re.search(r"户籍|户口|hukou|householdregistration", n):
        return "hukou"
    if re.search(r"现居|当前城市|所在城市|居住城市|currentcity|cityofresidence|residencecity|livingcity", n):
        return "current_city"
    if re.search(r"通讯地址|联系地址|详细地址|家庭地址|现住址|住址|mailingaddress|homeaddress|contactaddress", n):
        return "address"
    if re.search(r"^address$|\baddress\b", n) and not re.search(r"邮箱|email|网址|website|url", n):
        return "address"
    if re.search(r"籍贯|家乡|hometown|nativeplace", n):
        return "hometown"

    if re.search(r"应聘|申请|报考|投递", n) and re.search(r"岗位|职位|职务|position|job", n):
        return "applied_position"
    if re.search(r"期望|意向|目标|求职意向", n) and re.search(r"岗位|职位|职务|position|job", n):
        return "expected_position"
    if re.search(r"应聘公司|申请公司|目标公司|companyapplied|appliedcompany", n):
        return "applied_company"
    if re.search(r"期望城市|意向城市|期望工作地点|意向工作地点|期望工作城市|preferredlocation|desiredlocation|preferredcity|desiredcity", n):
        return "expected_city"
    if re.search(r"期望薪资|期望薪酬|期望月薪|薪资要求|薪资期望|期望年薪|expectedsalary|salaryexpectation|desiredsalary", n):
        return "expected_salary"
    if re.search(r"到岗|可入职|最快入职|入职日期|最早入职|available|availability|earlieststart|startdate", n):
        return "available_date"
    if re.search(r"求职类型|工作性质|招聘类型|应聘类型|jobtype|employmenttype", n):
        return "job_type"
    if re.search(r"服从调剂|接受调剂|工作地点调剂|relocation", n):
        return "accept_relocation"
    if re.search(r"接受出差|出差|business\s*trip|travel", n):
        return "accept_travel"
    if re.search(r"接受加班|加班|overtime", n):
        return "accept_overtime"
    if re.search(r"接受实习|实习意愿|accept\s*internship", n):
        return "accept_internship"
    if re.search(r"接受倒班|倒班|shift\s*work", n):
        return "accept_shift"
    if re.search(r"接受远程|远程办公|remote\s*work", n):
        return "accept_remote"

    if re.search(r"毕业院校|毕业学校|就读学校|学校名称|学校全称|school|university|institution", n):
        return "education_school"
    if re.search(r"学院|院系|所在院系|faculty|schoolof", n) and not re.search(r"学校|school", label):
        return "education_college"
    if re.search(r"所学专业|专业名称|主修专业|^专业$|(?:^|[^a-z])major(?:[^a-z]|$)|fieldofstudy|discipline", n):
        if not re.search(r"专业技能|技能", label):
            return "education_major"
    if re.search(r"学位类型|学位|degree\s*title", n):
        return "education_degree_title"
    if re.search(r"学历|最高学历|培养层次|educationlevel|highestdegree|academicdegree", n):
        return "education_degree"
    if re.search(r"入学时间|入学日期|入学年月|enrollmentdate|admissiondate", n):
        return "education_start"
    if re.search(r"毕业时间|毕业日期|预计毕业|graduationdate|expectedgraduation", n):
        return "education_end"
    if re.search(r"gpa|绩点|平均学分绩点|gradepoint", n):
        return "education_gpa"
    if re.search(r"加权平均|加权成绩|weightedaverage|weightedgpa", n):
        return "education_weighted_average"
    if re.search(r"平均成绩|平均分|均分|averagescore|averagegrade", n):
        return "education_average_score"
    if re.search(r"专业排名|班级排名|年级排名|排名|rank", n) and not re.search(r"排名要求", n):
        return "education_rank"
    if re.search(r"主修课程|主要课程|核心课程|courses|majorcourses", n):
        return "education_courses"
    if re.search(r"学号|studentid|studentnumber", n):
        return "education_student_id"
    if re.search(r"学习形式|培养方式|是否全日制|全日制|fulltime|studyform", n):
        return "education_full_time"

    if re.search(r"公司规模|公司性质|公司行业|公司地址|公司电话|公司网站|公司简介", n):
        return None
    if re.search(r"公司名称|公司|工作单位|单位名称|employer|company", n):
        if ctx == "internship" or re.search(r"实习|工作单位|单位名称", n) or ln in {"company", "employer", "companyname"}:
            return "internship_company"
    if ctx == "internship" and re.search(r"部门|department|division", n):
        return "internship_department"
    if ctx == "internship" and re.search(r"职位|岗位|职务|title|position|role", n):
        return "internship_title"
    if ctx == "internship" and re.search(r"工作地点|实习地点|location|city", n):
        return "internship_location"
    if ctx == "internship" and re.search(r"工作内容|实习内容|职责|描述|responsibilit|description", n):
        return "internship_description"
    if ctx == "internship" and re.search(r"技术栈|使用技术|技术|tech", n):
        return "internship_tech"
    if ctx == "internship" and re.search(r"工作成果|业绩|成果|achievement", n):
        return "internship_achievements"
    if ctx == "internship" and re.search(r"是否在职|仍在职|至今|目前在职|在职状态|current", n):
        return "internship_current"
    if ctx == "internship" and re.search(r"入职时间|开始时间|起始时间|startdate|from", n):
        return "internship_start"
    if ctx == "internship" and re.search(r"离职时间|结束时间|截止时间|enddate|to", n):
        return "internship_end"

    if re.search(r"项目名称|项目名|projectname", n):
        return "project_name"
    if re.search(r"项目角色|担任角色|项目职务|projectrole", n):
        return "project_role"
    if re.search(r"项目背景|projectbackground", n):
        return "project_background"
    if re.search(r"项目描述|项目简介|项目内容|projectdescription", n):
        return "project_description"
    if re.search(r"项目职责|个人职责|主要工作|projectduties|responsibilities", n) and ctx != "internship":
        return "project_duties"
    if re.search(r"项目技术栈|使用技术|技术栈|techstack", n) and ctx != "internship":
        return "project_tech"
    if re.search(r"项目成果|项目业绩|projectachievement", n):
        return "project_achievements"
    if re.search(r"项目链接|项目地址|github|gitee|portfoliolink", n):
        return "project_link"
    if re.search(r"项目开始|项目起始|projectstart", n):
        return "project_start"
    if re.search(r"项目结束|项目截止|projectend", n):
        return "project_end"

    if re.search(r"自我评价|自我介绍|个人评价|个人优势|个人简介|个人总结|selfevaluation|selfintroduction|aboutyourself|personalsummary", n):
        return "self_evaluation"
    if re.search(r"求职信|自荐信|coverletter|motivationletter", n):
        return "cover_letter"
    if re.search(r"获奖|荣誉|奖项|awards|honors", n):
        return "awards_text"
    if re.search(r"项目经历|项目经验|项目介绍|projectexperience", n):
        return "projects_text"
    if re.search(r"实习经历|工作经历|实习经验|工作履历|internshipexperience|workexperience", n):
        return "internships_text"
    if re.search(r"编程语言|开发语言|专业技能|技术能力|技能|skills|technicalskills", n):
        return "skills_text"
    if re.search(r"英语等级|英语水平|外语水平|英语能力|englishlevel|englishproficiency|cet", n):
        if re.search(r"成绩|分数|score", n):
            return "skill_english_score"
        return "skill_english"
    if re.search(r"证书|资格证|资格|执业|computerrank|certificate", n):
        return "skill_certificates"

    if re.search(r"毕业时间|毕业年份", n):
        return "education_end"
    if re.search(r"入学时间|入学年份", n):
        return "education_start"
    if re.search(r"开始时间|开始日期|起始时间|起始日期|startdate|from", n) and ctx in {"education", "internship", "project"}:
        return f"{ctx}_start"
    if re.search(r"结束时间|结束日期|截止时间|截止日期|enddate|to", n) and ctx in {"education", "internship", "project"}:
        return f"{ctx}_end"
    if re.search(r"离职时间", n):
        return "internship_end"
    if re.search(r"入职时间", n):
        return "internship_start" if ctx == "internship" else "available_date"
    if re.search(r"工作地点|工作城市|意向地点|location|worklocation", n):
        return "internship_location" if ctx == "internship" else "expected_city"
    if re.search(r"职位名称|岗位名称|^职位$|^岗位$|position|jobtitle", n):
        return "internship_title" if ctx == "internship" else "applied_position"
    if re.search(r"职务", n):
        return "internship_title"
    if re.search(r"工作内容|工作描述|工作职责|responsibilities|jobdescription", n):
        return "internship_description"
    if re.search(r"公司|employer|company", n) and not re.search(r"项目|学校", n):
        return "internship_company"
    if re.search(r"技术栈|使用技术|techstack|technologies", n):
        return "skills_text"
    if re.search(r"爱好|兴趣|hobbies|interests", n):
        return None
    return None


def _adapt_value(semantic: str, value: str, field: dict[str, Any]) -> tuple[str, list[str]]:
    text = str(value or "").strip()
    alternatives: list[str] = []

    def add(*items: Any) -> None:
        for item in items:
            item_text = str(item or "").strip()
            if item_text and item_text != text and item_text not in alternatives:
                alternatives.append(item_text)

    if semantic == "age":
        return _clean_age(text), []
    date_part = _DATE_PART_RE.match(semantic)
    if date_part:
        part = date_part.group(3)
        if part == "year":
            add(f"{text}年")
        elif part == "month":
            stripped = text.lstrip("0") or text
            add(stripped, f"{text}月", f"{stripped}月")
        elif part == "day":
            stripped = text.lstrip("0") or text
            add(stripped, f"{text}日", f"{stripped}日")
        return text, alternatives
    if semantic == "birth_date":
        return _date_value(text, field, month_only=False), _date_variants(text, field, month_only=False)
    if semantic in _MONTH_SEMANTICS:
        primary = _date_value(text, field, month_only=True)
        return primary, _date_variants(text, field, month_only=True)
    if semantic == "available_date":
        primary = _date_value(text, field, month_only=False) if parse_date(text) else text
        if parse_date(text):
            add(*_date_variants(text, field, month_only=False))
        add(*_available_variants(text))
        return primary, alternatives
    if semantic == "expected_city":
        cities = split_cities(text)
        primary = cities[0] if cities else text
        add(*cities[1:])
        add(*[city.removesuffix("市") for city in cities])
        add("、".join(cities), "，".join(cities), "/".join(cities))
        return primary, alternatives
    if semantic == "gender":
        if text in {"男", "男性", "male", "m"}:
            add("男性", "男", "male", "M")
        elif text in {"女", "女性", "female", "f"}:
            add("女性", "女", "female", "F")
        return text, alternatives
    if semantic == "education_degree":
        add("本科", "大学本科", "本科及以上", "全日制本科", "学士")
        return text, alternatives
    if semantic == "education_degree_title":
        add("学士", "学士学位", "工学学士")
        return text, alternatives
    if semantic == "education_full_time":
        bool_value = _bool_value(text)
        if bool_value is True:
            add("全日制", "普通全日制", "是")
        elif bool_value is False:
            add("非全日制", "否")
        return text, alternatives
    if semantic == "political_status":
        if "共青团员" in text:
            add("团员", "中国共产主义青年团团员")
        return text, alternatives
    if semantic == "marital_status":
        if "未婚" in text:
            add("未婚", "单身")
        return text, alternatives
    if semantic in {"accept_relocation", "accept_travel", "accept_overtime", "accept_internship", "accept_shift", "accept_remote"}:
        bool_value = _bool_value(text)
        if bool_value is True:
            add("是", "接受", "同意", "愿意", "服从", "true", "1", "yes")
        elif bool_value is False:
            add("否", "不接受", "不同意", "不愿意", "不服从", "false", "0", "no")
        return text, alternatives
    if semantic == "internship_current":
        bool_value = _bool_value(text)
        if bool_value is False:
            add("否", "已离职", "已结束", "不在职", "false")
        elif bool_value is True:
            add("是", "在职", "true")
        return text, alternatives
    if semantic == "job_type":
        if "校招" in text or "校园" in text:
            add("校园招聘", "校招", "应届生", "全职")
        elif "实习" in text:
            add("实习", "实习生")
        elif "全职" in text:
            add("全职", "社会招聘", "社招")
        return text, alternatives
    if semantic == "skill_english":
        match = re.search(r"CET[- ]?([46])", text, re.I)
        if match:
            level = match.group(1)
            add(f"CET-{level}", f"CET{level}", f"英语{level}级", f"大学英语{level}级", f"四级" if level == "4" else "六级")
        if re.search(r"四级|CET-?4", text, re.I):
            add("是", "有")
        score = _english_score(text)
        if score:
            add(score)
        return text, alternatives
    if semantic == "skill_english_score":
        score = _english_score(text) or text
        return score, alternatives
    if semantic == "skill_certificates":
        add(text)
        return text, alternatives
    if semantic == "expected_salary":
        add(*_salary_variants(text))
        return text, alternatives
    if semantic == "internship_company":
        add(text.replace("有限公司", ""), text.replace("（中国）", ""))
        return text, alternatives
    return text, alternatives


def public_plan(plan: dict[str, Any]) -> dict[str, Any]:
    payload = {
        key: value for key, value in plan.items()
        if key not in {"actions", "uploads"}
    }
    actions = []
    for action in plan.get("actions") or []:
        item = dict(action)
        item.pop("current", None)
        if action.get("semantic") in SENSITIVE_SEMANTICS or action.get("semantic") in {"phone", "email", "birth_date"}:
            item["value"] = _mask(action.get("value") or "")
            item["alternatives"] = []
            item["value_preview"] = item["value"]
        else:
            item["value_preview"] = _truncate(str(action.get("value") or ""), 120)
            item.pop("value", None)
        item.pop("alternatives", None)
        actions.append(item)
    uploads = []
    for upload in plan.get("uploads") or []:
        uploads.append({
            key: value for key, value in upload.items()
            if key not in {"path", "_path"}
        })
    payload["actions"] = actions
    payload["uploads"] = uploads
    return payload


def _plan_upload(
    field: dict[str, Any],
    profile: Profile,
    auto_roles: set[str],
    include_uploaded: bool,
) -> dict[str, Any]:
    role = detect_document_role(field)
    label = _field_label(field)
    if not role:
        return {"status": "unmatched", "reason": "unknown_document", "message": f"未识别附件用途: {label}"}
    if role in SENSITIVE_ROLES and role not in auto_roles:
        return {
            "status": "disabled",
            "reason": "sensitive_upload_disabled",
            "message": f"敏感证件 {label} 未开启自动上传，请人工确认后用 official_upload 上传",
        }
    if role not in auto_roles:
        return {
            "status": "disabled",
            "reason": "upload_disabled",
            "message": f"材料角色 {role} 未在 auto_upload_documents 中",
        }
    path = profile.document_for_role(role)
    if not path:
        if role == "resume":
            path = profile.resume_path()
        elif role == "photo":
            path = profile.photo_path()
    if not path:
        return {"status": "missing", "reason": "document_missing", "message": f"资料中未找到可上传的{label}"}
    if include_uploaded and field.get("value"):
        return {"status": "disabled", "reason": "already_uploaded", "message": f"页面已选择{label}"}
    return {
        "status": "planned",
        "id": str(field.get("id") or ""),
        "label": label,
        "role": role,
        "filename": path.name,
        "path": str(path),
        "exists": path.exists(),
        "required": bool(field.get("required")),
        "sensitive": role in SENSITIVE_ROLES,
    }


def detect_document_role(field: dict[str, Any]) -> str | None:
    text = " ".join(
        str(field.get(key) or "")
        for key in ("label", "aria_label", "placeholder", "name", "accept", "context")
    )
    n = normalize_label(text)
    if re.search(r"身份证反面|身份证背面|身份证国徽|国徽面|idback", n):
        return "id_back"
    if re.search(r"身份证正面|身份证人像|人像面|idfront", n):
        return "id_front"
    if re.search(r"简历图片|简历截图", n):
        return "resume_image"
    if re.search(r"简历|cv|resume", n):
        return "resume"
    if re.search(r"方形证件照|方形照片", n):
        return "photo_square"
    if re.search(r"头像小图|头像照片|avatar", n):
        return "photo_avatar"
    if re.search(r"证件照|一寸照|一寸照片|个人照片|照片|photo|photograph", n):
        return "photo"
    if re.search(r"均分证明|gpa证明", n):
        return "gpa_proof"
    if re.search(r"学籍验证|学籍在线|教育部学籍", n):
        return "enrollment_report"
    if re.search(r"在校证明|在读证明|录取证明|enrollmentproof", n):
        return "enrollment_proof"
    if re.search(r"四级|CET-?4|cet4", n):
        return "cet4"
    if re.search(r"成绩单|transcript", n):
        return "transcript"
    if re.search(r"实习证明|工作证明|internshipproof", n):
        return "internship_proof"
    if re.search(r"作品集|作品|附件|其他材料|other", n):
        return "other"
    return None


def _resolve_value(profile: Profile, semantic: str, job_title: str, job_company: str) -> str:
    date_part = _DATE_PART_RE.match(semantic)
    if date_part:
        base, side, part = date_part.groups()
        section = {"education": "education", "internship": "internships", "project": "projects"}[base]
        parsed = parse_date(profile.value(section, side))
        if not parsed:
            return ""
        return parsed.strftime({"year": "%Y", "month": "%m", "day": "%d"}[part])
    if semantic == "applied_position":
        return job_title or profile.value("job_intent", "expected_position")
    if semantic == "applied_company":
        return job_company
    if semantic == "age":
        age = profile.age()
        return str(age) if age is not None else ""
    if semantic == "extra_answer":
        return ""
    if semantic == "skills_text":
        return profile.skills_text()
    if semantic == "awards_text":
        return profile.awards_text()
    if semantic == "projects_text":
        return profile.combined_text("projects")
    if semantic == "internships_text":
        return profile.combined_text("internships")
    if semantic == "self_evaluation":
        return profile.self_evaluation
    if semantic == "cover_letter":
        return profile.cover_letter
    for prefix, section in (
        ("education_", "education"),
        ("internship_", "internships"),
        ("project_", "projects"),
        ("skill_", "skills"),
    ):
        if semantic.startswith(prefix):
            key = semantic[len(prefix):]
            return profile.value(section, key)
    for section, keys in (
        ("basic", {"name", "english_name", "gender", "birth_date", "phone", "email", "ethnicity",
                   "political_status", "marital_status", "height", "weight", "id_number", "hukou",
                   "current_city", "address", "hometown", "emergency_contact_name",
                   "emergency_contact_phone", "emergency_contact_relation"}),
        ("job_intent", {"expected_position", "expected_city", "expected_salary", "available_date", "job_type",
                        "accept_relocation", "accept_travel", "accept_overtime", "accept_internship",
                        "accept_shift", "accept_remote"}),
    ):
        if semantic in keys:
            return profile.value(section, semantic)
    return ""


def _extra_answer(profile: Profile, label: str) -> str:
    target = normalize_label(label)
    if not target:
        return ""
    candidates: list[tuple[str, str]] = []
    for mapping in (
        profile.common_questions,
        profile.extra_basic,
        profile.extra_skills,
        profile.preferences,
    ):
        for key, value in mapping.items():
            if not is_blank(value) and not is_uncertain(value):
                candidates.append((str(key), str(value)))
    best: tuple[int, str] | None = None
    for key, value in candidates:
        normalized = normalize_label(key)
        if not normalized:
            continue
        score = 0
        if target == normalized:
            score = 1000 + len(normalized)
        elif len(normalized) >= 2 and normalized in target:
            score = 100 + len(normalized)
        elif len(target) >= 2 and target in normalized:
            score = 80 + len(target)
        if score and (best is None or score > best[0]):
            best = (score, value)
    return best[1] if best else ""


def _date_value(value: str, field: dict[str, Any], *, month_only: bool) -> str:
    parsed = parse_date(value)
    if not parsed:
        return value
    pattern = _date_pattern(field, month_only=month_only)
    return parsed.strftime(pattern)


def _date_variants(value: str, field: dict[str, Any], *, month_only: bool) -> list[str]:
    parsed = parse_date(value)
    if not parsed:
        return []
    patterns = [
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%Y.%m.%d",
        "%Y年%m月%d日",
        "%Y-%m",
        "%Y/%m",
        "%Y.%m",
        "%Y年%m月",
    ]
    primary = _date_pattern(field, month_only=month_only)
    result = []
    for pattern in patterns:
        if pattern == primary:
            continue
        item = parsed.strftime(pattern)
        if item not in result:
            result.append(item)
    return result


def _date_pattern(field: dict[str, Any], *, month_only: bool) -> str:
    input_type = str(field.get("input_type") or "").lower()
    if input_type == "date":
        return "%Y-%m-%d"
    if input_type == "month":
        return "%Y-%m"
    placeholder = str(field.get("placeholder") or "")
    if re.search(r"YYYY[-/.]MM[-/.]DD|yyyy[-/.]mm[-/.]dd|年.*月.*日", placeholder):
        if "." in placeholder:
            return "%Y.%m.%d"
        if "/" in placeholder:
            return "%Y/%m/%d"
        if "年" in placeholder:
            return "%Y年%m月%d日"
        return "%Y-%m-%d"
    if re.search(r"YYYY[-/.]MM|yyyy[-/.]mm|年.*月", placeholder):
        if "." in placeholder:
            return "%Y.%m"
        if "/" in placeholder:
            return "%Y/%m"
        if "年" in placeholder:
            return "%Y年%m月"
        return "%Y-%m"
    return "%Y-%m" if month_only else "%Y-%m-%d"


def _available_variants(value: str) -> list[str]:
    if value in {"随时", "随时到岗", "立即", "立即到岗", "即刻"}:
        return ["随时到岗", "随时", "立即到岗", "即刻到岗", "ASAP"]
    if value in {"毕业后", "毕业之后"}:
        return ["毕业后", "毕业之后", "拿到毕业证后"]
    return []


def _salary_variants(value: str) -> list[str]:
    result: list[str] = []
    compact = value.replace(" ", "")
    if compact != value:
        result.append(compact)
    match = re.search(r"(\d+(?:\.\d+)?)\s*[-~—至]\s*(\d+(?:\.\d+)?)\s*([kK千万])", value)
    if match:
        low, high, unit = match.group(1), match.group(2), match.group(3)
        if unit in {"k", "K"}:
            result.extend([
                f"{low}-{high}K",
                f"{low}k-{high}k",
                f"{int(float(low) * 1000)}-{int(float(high) * 1000)}元/月",
                f"{low}-{high}K·13薪",
            ])
        elif unit == "万":
            result.extend([f"{low}-{high}万", f"{low}-{high}万/年", f"{low}万-{high}万/年"])
    return result


def _clean_age(value: str) -> str:
    match = re.search(r"\d{2}", value)
    return match.group(0) if match else value


def _english_score(value: str) -> str:
    matches = re.findall(r"(?<!\d)(\d{3})(?!\d)", value)
    return matches[-1] if matches else ""


def _field_section(field: dict[str, Any]) -> str:
    return str(field.get("section") or field.get("context") or "").strip()


def _is_login_field(field: dict[str, Any]) -> bool:
    if field.get("dialog_login"):
        return True
    section = _field_section(field)
    if _LOGIN_SECTION_RE.search(section):
        return True
    text = " ".join(
        str(field.get(key) or "") for key in ("label", "placeholder", "aria_label", "name")
    )
    return bool(re.search(r"验证码|短信码|登录密码|登录账号", text))


def _ignore_section_patterns(template: dict[str, Any] | None) -> list[re.Pattern[str]]:
    patterns = list(_DEFAULT_IGNORE_SECTIONS)
    if template:
        patterns.extend(str(item) for item in template.get("ignore_sections") or [] if str(item).strip())
    compiled = []
    for pattern in patterns:
        try:
            compiled.append(re.compile(pattern))
        except re.error:
            compiled.append(re.compile(re.escape(pattern)))
    return compiled


def _ignored_section(section: str, patterns: list[re.Pattern[str]]) -> str:
    if not section:
        return ""
    for pattern in patterns:
        if pattern.search(section):
            return f"“{section}”区块不自动填写，需本人补充"
    return ""


def _date_part_semantics(
    fields: list[dict[str, Any]],
    template: dict[str, Any] | None,
) -> dict[str, str]:
    if template and template.get("hints", {}).get("split_dates") is False:
        return {}
    buckets: dict[tuple[str, str, bool, bool], list[tuple[dict[str, Any], str]]] = {}
    for field in fields:
        if not isinstance(field, dict) or not field.get("id"):
            continue
        if field.get("hidden") or _is_login_field(field):
            continue
        kind = str(field.get("kind") or "")
        if kind not in {"input", "select"}:
            continue
        if kind == "input" and not field.get("custom_select"):
            if str(field.get("input_type") or "") not in {"", "text", "tel", "number"}:
                continue
        part = _date_part_of(field)
        if not part:
            continue
        section = _field_section(field)
        group = str(field.get("group_label") or "").strip()
        key = (
            normalize_label(section),
            normalize_label(group or section),
            bool(field.get("in_iframe")),
            bool(field.get("in_shadow")),
        )
        buckets.setdefault(key, []).append((field, part))

    result: dict[str, str] = {}
    for items in buckets.values():
        if len(items) < 2:
            continue
        items.sort(key=lambda item: int(item[0].get("order") or 0))
        sample = items[0][0]
        group_text = f"{sample.get('group_label') or ''} {_field_section(sample)}"
        base = _date_group_base(group_text)
        if not base:
            continue
        side_hint = _date_group_side(group_text)
        two_ranges = len(items) >= 4 and not side_hint
        ranges = 2 if two_ranges and len(items) >= 4 else 1
        size = len(items) // ranges
        for range_index in range(ranges):
            slice_items = items[range_index * size:(range_index + 1) * size]
            if not slice_items:
                continue
            side = side_hint or ("start" if range_index == 0 else "end")
            for (field, _), part in zip(slice_items, _date_parts_for_slice(slice_items)):
                result[str(field.get("id"))] = f"{base}_{side}_{part}"
    return result


def _date_part_of(field: dict[str, Any]) -> str:
    texts = [str(field.get("placeholder") or "")]
    for key in ("label", "aria_label"):
        value = str(field.get(key) or "")
        texts.extend(part.strip() for part in value.split("|"))
    for text in texts:
        normalized = normalize_label(text)
        if not normalized:
            continue
        for part, pattern in _DATE_PART_LABELS.items():
            if pattern.search(normalized):
                return part
    options = [
        normalize_label(str(option.get("label") or option.get("value") or ""))
        for option in field.get("options") or [] if isinstance(option, dict)
    ]
    options = [item for item in options if item and item not in _PLACEHOLDER_VALUES][:8]
    if options:
        if all(re.fullmatch(r"20\d{2}(年)?", item) for item in options):
            return "year"
        if all(re.fullmatch(r"0?[1-9]|1[0-2](月)?", item) for item in options):
            return "month"
    return ""


def _date_parts_for_slice(items: list[tuple[dict[str, Any], str]]) -> list[str]:
    explicit = [part for _, part in items]
    fallback = {1: ["year"], 2: ["year", "month"], 3: ["year", "month", "day"]}.get(len(items), [])
    return [explicit[index] or (fallback[index] if index < len(fallback) else "year") for index in range(len(items))]


def _date_group_base(text: str) -> str:
    normalized = normalize_label(text)
    if "项目" in normalized:
        return "project"
    if any(token in normalized for token in ("教育", "就读", "学校", "学历", "在校", "毕业")):
        return "education"
    if any(token in normalized for token in ("实习", "工作经历", "工作履历", "任职", "在职")):
        return "internship"
    return ""


def _date_group_side(text: str) -> str:
    normalized = normalize_label(text)
    start = bool(re.search(r"开始|起始|入学|入职", normalized))
    end = bool(re.search(r"结束|毕业|截止|离职|终止", normalized))
    if start and not end:
        return "start"
    if end and not start:
        return "end"
    return ""


def _context_kind(context: str) -> str:
    if not context:
        return ""
    if "项目" in context:
        return "project"
    if "教育" in context or "学校" in context or "学历" in context or "就读" in context:
        return "education"
    if "实习" in context or "工作经历" in context or "工作履历" in context or "职业" in context:
        return "internship"
    if "家庭" in context or "亲属" in context or "亲友" in context:
        return "family"
    if "登录" in context or "注册" in context or "验证码" in context:
        return "login"
    if "声明" in context or "授权" in context or "隐私" in context or "条款" in context:
        return "consent"
    return ""


def _is_consent(field: dict[str, Any]) -> bool:
    if str(field.get("kind") or "") != "checkbox":
        return False
    text = f"{field.get('label') or ''} {field.get('context') or ''} {field.get('section') or ''}"
    if "同步更新在线简历" in text:
        return False
    if re.search(r"同意|隐私|条款|授权|承诺|真实|协议|我已经阅读|已阅读|确认以上", text):
        return True
    section = normalize_label(str(field.get("section") or field.get("context") or ""))
    label = normalize_label(str(field.get("label") or ""))
    if re.search(r"声明|授权|隐私|条款|协议|承诺", section):
        return bool(re.search(r"确认|同意|已阅读|已勾选|本人", label)) or len(label) <= 4
    return False


def _field_label(field: dict[str, Any]) -> str:
    for key in ("label", "aria_label", "placeholder", "name"):
        value = str(field.get(key) or "").strip()
        if value:
            return value[:160]
    return "(未命名字段)"


def _humanize_name(value: str) -> str:
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", value).replace("_", " ").replace("-", " ")


def _current_value(field: dict[str, Any]) -> str:
    if field.get("kind") in {"checkbox", "radio"}:
        return "是" if field.get("checked") else ""
    return str(field.get("value") or "")


def _field_has_value(field: dict[str, Any]) -> bool:
    if field.get("kind") in {"checkbox", "radio"}:
        return bool(field.get("checked"))
    if field.get("kind") == "file":
        return bool(field.get("value"))
    value = str(field.get("value") or "").strip()
    if value.lower() in _PLACEHOLDER_VALUES:
        return False
    if field.get("kind") == "select":
        options = field.get("options") or []
        for option in options:
            if str(option.get("value")) == value and option.get("selected"):
                label = str(option.get("label") or "").strip().lower()
                if label in _PLACEHOLDER_VALUES:
                    return False
    return bool(value)


def _matches_value(current: str, primary: str, alternatives: list[str]) -> bool:
    now = normalize_label(current)
    if not now or now in _PLACEHOLDER_VALUES:
        return False
    for candidate in (primary, *alternatives):
        wanted = normalize_label(candidate)
        if wanted and (now == wanted or wanted in now or now in wanted):
            return True
    return False


def _bool_value(value: str) -> bool | None:
    text = normalize_label(value)
    if text in _YES:
        return True
    if text in _NO:
        return False
    return None


def _unknown(field: dict[str, Any], reason: str) -> dict[str, Any]:
    return {
        "id": str(field.get("id") or ""),
        "label": _field_label(field),
        "kind": str(field.get("kind") or "input"),
        "required": bool(field.get("required")),
        "placeholder": str(field.get("placeholder") or ""),
        "context": str(field.get("context") or ""),
        "section": _field_section(field),
        "options": [
            str(option.get("label") or option.get("value") or "")
            for option in (field.get("options") or [])[:20]
            if isinstance(option, dict)
        ],
        "reason": reason,
    }


def _brief(field: dict[str, Any], reason: str, message: str) -> dict[str, Any]:
    return {
        "id": str(field.get("id") or ""),
        "label": _field_label(field),
        "kind": str(field.get("kind") or "input"),
        "section": _field_section(field),
        "reason": reason,
        "message": message,
    }


def _dedupe(items: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: set[tuple[Any, ...]] = set()
    result = []
    for item in items:
        key = (item.get("id"), item.get("label"), item.get("reason"), item.get("semantic"))
        if key in seen:
            continue
        seen.add(key)
        result.append(item)
    return result


def _dedupe_strings(items: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    result = []
    for item in items:
        text = str(item)
        if text and text not in seen:
            seen.add(text)
            result.append(text)
    return result


def _mask(value: str) -> str:
    text = str(value or "")
    if not text:
        return ""
    if re.fullmatch(r"\d{17}[\dXx]", text):
        return f"{text[:3]}********{text[-2:]}"
    if re.fullmatch(r"1[3-9]\d{9}", text):
        return f"{text[:3]}****{text[-4:]}"
    if "@" in text and re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", text):
        local, _, domain = text.partition("@")
        masked = local[:2] + "***" + local[-2:] if len(local) > 2 else local[:1] + "*"
        return f"{masked}@{domain}"
    return f"{text[:2]}***{text[-2:]}" if len(text) > 4 else "***"


def _truncate(value: str, limit: int) -> str:
    return value if len(value) <= limit else value[:limit].rstrip() + "..."
