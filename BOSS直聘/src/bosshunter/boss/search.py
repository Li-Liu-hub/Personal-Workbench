"""BOSS Zhipin job search, list pagination and detail extraction."""

from __future__ import annotations

import json
import random
import re
import time
from typing import Any
from urllib.parse import quote, urlencode, urljoin

from bosshunter.browser import Browser, BrowserError, CDPError, CDPTab
from bosshunter.boss.font import SalaryDecoder, decode_text, has_private_glyph
from bosshunter.config import city_code_for
from bosshunter.errors import ToolError
from bosshunter.safety import SafetyError, SafetyState

SEARCH_URL = "https://www.zhipin.com/web/geek/job?query={keyword}&city={city_code}"
CHAT_URL = "https://www.zhipin.com/web/geek/chat"

_FILTER_SEPARATOR = re.compile(r"[,，、;；]")

BOSS_FILTER_OPTIONS: dict[str, dict[str, str]] = {
    "job_type": {"全职": "1901", "兼职": "1903", "实习": "1902"},
    "experience": {
        "经验不限": "101", "应届生": "102", "1年以内": "103", "1-3年": "104",
        "3-5年": "105", "5-10年": "106", "10年以上": "107", "在校生": "108",
    },
    "degree": {
        "学历不限": "201", "大专": "202", "本科": "203", "硕士": "204",
        "博士": "205", "高中": "206", "中专/中技": "208", "初中及以下": "209",
    },
    "scale": {
        "0-20人": "301", "20-99人": "302", "100-499人": "303",
        "500-999人": "304", "1000-9999人": "305", "10000人以上": "306",
    },
    "salary": {
        "3K以下": "402", "3-5K": "403", "5-10K": "404",
        "10-20K": "405", "20-50K": "406", "50K以上": "407",
    },
}
BOSS_FILTER_PARAMS = {
    "job_type": "jobType",
    "experience": "experience",
    "degree": "degree",
    "scale": "scale",
    "salary": "salary",
    "industry": "industry",
}

JS_IS_SCROLL_LIST = "Boolean(document.querySelector('.page-jobs .job-list-container .rec-job-list, .rec-job-list'))"

JS_SCROLL_LIST = """
(() => {
    const list = document.querySelector('.rec-job-list') || document.querySelector('.job-list-container');
    if (list) list.scrollTop = list.scrollHeight;
    const scroller = document.scrollingElement || document.documentElement;
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
    window.scrollTo(0, scroller ? scroller.scrollHeight : document.body.scrollHeight);
    window.dispatchEvent(new Event('scroll'));
    if (list) list.dispatchEvent(new Event('scroll'));
    return true;
})()
"""

JS_EXTRACT_LIST = """
(() => {
    const wraps = document.querySelectorAll('.job-card-wrap');
    const jobs = [];
    wraps.forEach((wrap) => {
        const box = wrap.querySelector('.job-card-box') || wrap;
        const nameEl = box.querySelector('.job-name');
        const salaryEl = box.querySelector('.job-salary');
        const tags = box.querySelectorAll('.tag-list li');
        const companyEl = box.querySelector('.boss-name') || box.querySelector('.company-name');
        const locationEl = box.querySelector('.company-location');
        if (!nameEl || !nameEl.href) return;
        jobs.push({
            title: nameEl.textContent.trim(),
            salary: salaryEl ? salaryEl.textContent.trim() : '',
            experience: tags[0] ? tags[0].textContent.trim() : '',
            education: tags[1] ? tags[1].textContent.trim() : '',
            company: companyEl ? companyEl.textContent.trim() : '',
            location: locationEl ? locationEl.textContent.trim() : '',
            url: nameEl.href.split('#')[0]
        });
    });
    return JSON.stringify(jobs);
})()
"""

JS_EXTRACT_DETAIL = """
(() => {
    const info = {};
    info.title = document.querySelector('.info-primary .name h1')?.textContent?.trim()
        || document.querySelector('.name h1')?.textContent?.trim()
        || document.title.split('-')[0]?.trim();
    const salaryEl = document.querySelector('.info-primary .salary') || document.querySelector('.salary');
    info.salary = salaryEl?.textContent?.trim() || '';
    const tagItems = document.querySelectorAll('.info-primary .tag-list span, .job-primary .tag-list span');
    const tagTexts = Array.from(tagItems).map(t => t.textContent.trim()).filter(Boolean);
    info.experience = tagTexts[0] || document.querySelector('.info-primary .text-experiece')?.textContent?.trim() || '';
    info.education = tagTexts[1] || document.querySelector('.info-primary .text-degree')?.textContent?.trim() || '';
    const jdEl = document.querySelector('.job-sec-text');
    info.jd = jdEl ? jdEl.textContent.trim() : '';
    const bodyText = document.body ? document.body.innerText : '';
    info.jd_truncated = /登录查看完整内容|登录后查看/.test(bodyText);
    const pageText = [info.title, info.experience, bodyText.slice(0, 3000)].join(' ');
    info.recruitment_type = /校招|校园招聘|应届|毕业生|管培生/.test(pageText)
        ? 'campus'
        : (/社招|社会招聘/.test(pageText) ? 'experienced' : 'unknown');
    info.company = '';
    const companyLinks = document.querySelectorAll('.sider-company .company-info a');
    for (const link of companyLinks) {
        const text = link.textContent.trim();
        if (text) { info.company = text; break; }
    }
    if (!info.company) {
        const bossMatch = (document.querySelector('.job-boss-info')?.innerText || '').match(/([^\\n·]+)·招聘/);
        if (bossMatch) info.company = bossMatch[1].trim();
    }
    if (!info.company) {
        const titleMatch = document.title.match(/_(.+?)招聘/);
        info.company = titleMatch ? titleMatch[1] : '';
    }
    const companyTags = document.querySelectorAll(
        '.sider-company .res-industry-item, .company-info-item, .company-info .company-tag'
    );
    info.company_size = '';
    info.company_industry = '';
    companyTags.forEach(tag => {
        const text = tag.textContent.trim();
        if (!text) return;
        if (text.includes('人')) info.company_size = text;
        else if (!info.company_industry) info.company_industry = text;
    });
    if (!info.company_size) {
        const sidebarText = document.querySelector('.sider-company')?.innerText || '';
        const sizeMatch = sidebarText.match(/(\\d+-\\d+人|\\d+人以上|少于\\d+人)/);
        if (sizeMatch) info.company_size = sizeMatch[1];
    }
    if (!info.company_industry) {
        info.company_industry = document.querySelector(
            '.sider-company a[ka="job-detail-brandindustry"], .sider-company .company-info a[href^="/i"]'
        )?.textContent?.trim() || '';
    }
    const bossSection = document.querySelector('.job-boss-info') || document.querySelector('.boss-info-attr')
        || document.querySelector('.boss-info');
    const hrNameEl = bossSection?.querySelector('h2.name, .name, .boss-name');
    info.hr_name = hrNameEl
        ? Array.from(hrNameEl.childNodes)
            .filter(n => n.nodeType === 3)
            .map(n => n.textContent)
            .join('')
            .trim()
        : '';
    const bossAttr = bossSection?.querySelector('.boss-info-attr')?.textContent?.trim() || '';
    info.hr_title = bossAttr ? bossAttr.split('·').pop().trim() : '';
    info.hr_active = bossSection?.querySelector('.boss-online-tag')?.textContent?.trim() || '';
    info.url = location.href;
    info.page_title = document.title;
    return JSON.stringify(info);
})()
"""

JS_DETECT_RISK = """
(() => {
    const text = (document.body?.innerText || '').slice(0, 12000);
    const url = String(location.href || '');
    const title = String(document.title || '');
    const hasExpectedContent = Boolean(document.querySelector(
        '.job-card-wrap, .job-card-box, .job-list-box, .job-sec-text, .job-detail, '
        + '.job-primary, .chat-record, .chat-editor, .chat-container'
    ));
    const captcha = document.querySelector(
        '.geetest_panel, .captcha, [class*="captcha"], [id*="captcha"], '
        + 'iframe[src*="captcha"], iframe[src*="verify"]'
    );
    if (captcha) return JSON.stringify({risk: 'captcha', evidence: 'captcha_element'});
    if (/captcha|security-check|\\/verify/i.test(url)) return JSON.stringify({risk: 'captcha', evidence: 'captcha_url'});
    if (/\\/web\\/user\\/(?:login)?(?:\\?|$)/i.test(url) || /\\/login(?:\\?|$)/i.test(url)) {
        return JSON.stringify({risk: 'login_required', evidence: 'login_url'});
    }
    const pageUrl = new URL(url);
    const blockedPath = /(?:^|\\/)(?:403|forbidden|access-denied)(?:\\/|$)/i.test(pageUrl.pathname);
    const blockedStatus = Array.from(pageUrl.searchParams).some(([key, value]) =>
        /^(?:code|status|error|error_code|status_code)$/i.test(key)
        && /^(?:403|forbidden|access-denied)$/i.test(value)
    );
    if (blockedPath || blockedStatus) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_url'});
    }
    if (/^(?:403(?:\\s+forbidden)?|forbidden|access denied|访问被拒绝|账号异常|账号受限)/i.test(title.trim())) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_title'});
    }
    if (!hasExpectedContent && /验证码|安全验证|完成验证/.test(text)) {
        return JSON.stringify({risk: 'captcha', evidence: 'captcha_page'});
    }
    if (!hasExpectedContent && /(?:^|\\n)\\s*403(?:\\s+forbidden)?\\s*(?:\\n|$)|访问被拒绝|账号异常|账号受限/i.test(text)) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_page'});
    }
    if (!hasExpectedContent && /操作频繁|访问频繁|请求频繁|稍后再试|频率限制/.test(text)) {
        return JSON.stringify({risk: 'rate_limit', evidence: 'rate_limit_page'});
    }
    return JSON.stringify({risk: null});
})()
"""

JS_VISIBLE_LOGIN = """
(() => {
    const path = location.pathname || '';
    const text = document.body ? document.body.innerText.slice(0, 8000) : '';
    const hasChatUi = Boolean(document.querySelector(
        '#chat-input, .chat-editor, .chat-record, '
        + '.user-nav .nav-figure, .user-nav .figure, .user-nav img'
    ));
    const loginForm = Boolean(document.querySelector(
        '.login-form, .login-dialog, .sign-in, form[class*="login"], [class*="login-form"]'
    ));
    const loginUrl = /\\/web\\/user\\/(?:login)?(?:\\?|$)/i.test(path) || /\\/login(?:\\?|$)/i.test(path);
    const loginText = /扫码登录|短信登录|密码登录|登录后查看|请先登录|登录状态已失效|当前登录状态/.test(text);
    return JSON.stringify({
        logged_in: !loginUrl && !loginForm && !loginText && hasChatUi,
        login_url: loginUrl,
        login_form: loginForm,
        has_chat_ui: hasChatUi,
        login_text: loginText,
        path: path,
        title: document.title || ''
    });
})()
"""

JOB_CLOSED_MARKERS = (
    "访问的页面不存在", "您访问的页面不存在",
    "职位已关闭", "职位已经关闭", "该职位已关闭", "此职位已关闭", "岗位已关闭",
    "该职位已下线", "该职位已下架", "职位已下线", "职位已下架",
    "该职位已暂停招聘", "职位已暂停招聘", "停止招聘", "已停止招聘",
    "暂停招聘", "招聘已暂停", "该职位已招满", "职位已招满", "已招满",
    "该职位暂不招人", "暂不招人",
)


def job_id_from_url(url: str) -> str:
    match = re.search(r"/job_detail/([^.?/#]+)", str(url or ""))
    if match:
        return match.group(1)
    return ""


def read_risk(tab: CDPTab) -> dict[str, str] | None:
    raw = tab.evaluate(JS_DETECT_RISK)
    try:
        value = json.loads(raw) if isinstance(raw, str) else raw
    except (json.JSONDecodeError, TypeError):
        return None
    if not isinstance(value, dict) or not value.get("risk"):
        return None
    return {
        "kind": str(value.get("risk")),
        "evidence": str(value.get("evidence") or "unspecified"),
    }


def inspect_risk(tab: CDPTab, sleeper: Any = time.sleep) -> dict[str, str] | None:
    first = read_risk(tab)
    if first is None:
        return None
    sleeper(0.8)
    second = read_risk(tab)
    if second is None or second["kind"] != first["kind"]:
        return None
    return second


def check_page_state(
    config: dict[str, Any],
    state: SafetyState,
    tab: CDPTab,
    *,
    allow_login: bool = False,
) -> None:
    risk = inspect_risk(tab)
    if risk is None:
        return
    if risk["kind"] == "login_required":
        if allow_login:
            return
        raise ToolError(
            "login_required",
            "BOSS 登录状态已失效，请先运行 `bosshunter login` 或调用 boss_open_login 扫码登录",
            login_url="https://www.zhipin.com/web/user/",
        )
    lock_minutes = int(config["safety"]["risk_lock_minutes"])
    state.lock(risk["kind"], lock_minutes)
    raise ToolError(
        "platform_risk",
        f"BOSS 返回了 {risk['kind']} 风险提示，已停止并冷却 {lock_minutes} 分钟",
        risk=risk["kind"],
        evidence=risk.get("evidence", ""),
        lock_minutes=lock_minutes,
    )


def is_closed_page(tab: CDPTab) -> bool:
    markers = json.dumps(list(JOB_CLOSED_MARKERS), ensure_ascii=False)
    raw = tab.evaluate(
        f"""
        (() => {{
            const text = document.body ? document.body.innerText : '';
            const title = document.title || '';
            const markers = {markers};
            return JSON.stringify({{
                closed: title.includes('访问的页面不存在') || markers.some((marker) => text.includes(marker))
            }});
        }})()
        """
    )
    try:
        value = json.loads(raw) if isinstance(raw, str) else raw
    except (json.JSONDecodeError, TypeError):
        return False
    return bool(isinstance(value, dict) and value.get("closed"))


def normalize_filters(filters: Any) -> dict[str, list[str]]:
    if not isinstance(filters, dict):
        return {}
    normalized: dict[str, list[str]] = {}
    for key in BOSS_FILTER_PARAMS:
        raw = filters.get(key)
        values = raw if isinstance(raw, list) else _FILTER_SEPARATOR.split(raw) if isinstance(raw, str) else []
        cleaned: list[str] = []
        for item in values:
            text = str(item or "").strip()
            if not text or text in cleaned:
                continue
            if key == "industry":
                if re.fullmatch(r"\d{1,12}", text):
                    cleaned.append(text)
            elif text in BOSS_FILTER_OPTIONS[key]:
                cleaned.append(text)
        if cleaned:
            normalized[key] = cleaned
    return normalized


def build_filter_query(filters: Any) -> str:
    query: dict[str, str] = {}
    for key, values in normalize_filters(filters).items():
        encoded = values if key == "industry" else [BOSS_FILTER_OPTIONS[key][value] for value in values]
        query[BOSS_FILTER_PARAMS[key]] = ",".join(encoded)
    return urlencode(query)


class BossSearcher:
    def __init__(self, config: dict[str, Any], state: SafetyState, browser: Browser | None = None) -> None:
        self.config = config
        self.state = state
        self.browser = browser or Browser(config)
        self._random = random.SystemRandom()
        self.warnings: list[str] = []
        self.stats = {
            "search_pages": 0,
            "detail_pages": 0,
            "list_items": 0,
            "detail_failures": 0,
        }

    def search(
        self,
        *,
        keywords: list[str] | None = None,
        cities: list[str] | None = None,
        max_pages: int | None = None,
        filters: dict[str, Any] | None = None,
        sort: str | None = None,
        include_details: bool = False,
        max_details: int = 60,
    ) -> dict[str, Any]:
        search_cfg = self.config["search"]
        keywords = [item.strip() for item in (keywords or search_cfg["keywords"]) if str(item).strip()]
        cities = [item.strip() for item in (cities or search_cfg["cities"]) if str(item).strip()]
        max_pages = max(1, min(int(max_pages or search_cfg["max_pages"]), 50))
        filters = filters if filters is not None else search_cfg.get("filters") or {}
        sort = sort or search_cfg["sort"]
        if not keywords:
            raise ToolError("no_keywords", "没有配置搜索关键词")
        if not cities:
            raise ToolError("no_cities", "没有配置搜索城市")

        combos: list[tuple[str, str, str]] = []
        for city in cities:
            code = city_code_for(city, self.config)
            if not code:
                self.warnings.append(f"未识别的 BOSS 城市：{city}")
                continue
            for keyword in keywords:
                combos.append((city, code, keyword))
        if not combos:
            raise ToolError("no_valid_city", "没有可用的 BOSS 城市编码")

        self.state.ensure_unlocked()
        jobs: dict[str, dict[str, Any]] = {}
        diagnostics: dict[str, Any] = {}
        try:
            for city, code, keyword in combos:
                self._collect_combo(city, code, keyword, max_pages, filters, sort, jobs, diagnostics)
        except SafetyError as exc:
            raise exc
        finally:
            pass

        result_jobs = list(jobs.values())
        if include_details:
            self._attach_details(result_jobs, max_details=max_details)
        return {
            "count": len(result_jobs),
            "jobs": result_jobs,
            "queries": [{"city": city, "keyword": keyword} for city, _code, keyword in combos],
            "warnings": self.warnings,
            "stats": self.stats,
            "diagnostics": diagnostics,
        }

    def detail(self, url: str) -> dict[str, Any]:
        job = self._fetch_detail(url)
        return {"job": job}

    def _collect_combo(
        self,
        city: str,
        city_code: str,
        keyword: str,
        max_pages: int,
        filters: Any,
        sort: str,
        jobs: dict[str, dict[str, Any]],
        diagnostics: dict[str, Any],
    ) -> None:
        list_tab: CDPTab | None = None
        loaded_ids: set[str] = set()
        try:
            for page in range(1, max_pages + 1):
                if page == 1:
                    url = SEARCH_URL.format(keyword=quote(keyword), city_code=city_code)
                    query = build_filter_query(filters)
                    if query:
                        url += f"&{query}"
                    if sort == "newest":
                        url += "&sortType=2"
                    list_tab = self._open_search_page(url)
                    batch = self._read_list(list_tab)
                    if not batch:
                        check_page_state(self.config, self.state, list_tab, allow_login=False)
                    if batch is None:
                        self.warnings.append(f"{city} · {keyword} 首页列表解析失败")
                        diagnostics[f"{city}|{keyword}"] = self._page_diagnostics(list_tab)
                        return
                    loaded_ids = {job_id_from_url(item.get("url", "")) for item in batch if item.get("url")}
                    self._add_batch(jobs, batch, city, keyword, keyword)
                    if not batch:
                        diagnostics[f"{city}|{keyword}"] = self._page_diagnostics(list_tab)
                        self.warnings.append(f"{city} · {keyword} 首页未读取到岗位")
                else:
                    if list_tab is None:
                        return
                    scroll_list = list_tab.evaluate(JS_IS_SCROLL_LIST) is True
                    if scroll_list:
                        batch = self._scroll_next_batch(list_tab, loaded_ids)
                    else:
                        url = SEARCH_URL.format(keyword=quote(keyword), city_code=city_code)
                        query = build_filter_query(filters)
                        if query:
                            url += f"&{query}"
                        if sort == "newest":
                            url += "&sortType=2"
                        url += f"&page={page}"
                        self._reserve_page("search")
                        if not list_tab.navigate(url) or not list_tab.wait_for_load(15):
                            self.warnings.append(f"{city} · {keyword} 第 {page} 页加载失败")
                            break
                        check_page_state(self.config, self.state, list_tab, allow_login=False)
                        decoder = SalaryDecoder(list_tab)
                        decoder.refresh()
                        batch = self._read_list(list_tab)
                    if not batch:
                        check_page_state(self.config, self.state, list_tab, allow_login=False)
                        break
                    new_ids = {
                        job_id_from_url(item.get("url", ""))
                        for item in batch
                        if job_id_from_url(item.get("url", "")) not in loaded_ids
                    }
                    if not new_ids:
                        self.warnings.append(f"{city} · {keyword} 第 {page} 页没有新岗位，已停止该组合翻页")
                        break
                    loaded_ids.update(new_ids)
                    self._add_batch(jobs, batch, city, keyword, keyword)
                if page < max_pages:
                    self._sleep(1.5, 3.0)
        finally:
            if list_tab is not None:
                self.browser.close(list_tab)

    def _open_search_page(self, url: str) -> CDPTab:
        self._reserve_page("search")
        try:
            tab = self.browser.open(url, background=True)
        except CDPError as exc:
            raise ToolError("open_page_failed", f"无法打开 BOSS 搜索页: {exc}") from exc
        try:
            if not tab.wait_for_load(20):
                self._sleep(2.0, 3.0)
            check_page_state(self.config, self.state, tab, allow_login=False)
            self._sleep(1.0, 2.0)
            return tab
        except Exception:
            self.browser.close(tab)
            raise

    def _read_list(self, tab: CDPTab, decoder: SalaryDecoder | None = None) -> list[dict[str, Any]] | None:
        decoder = decoder or SalaryDecoder(tab)
        if not decoder.mapping:
            decoder.refresh()
        raw = tab.evaluate(JS_EXTRACT_LIST)
        try:
            items = json.loads(raw) if isinstance(raw, str) else raw
        except (json.JSONDecodeError, TypeError):
            return None
        if not isinstance(items, list):
            return None
        result = self._decode_items(items, decoder)
        if any(item.get("salary_unreadable") for item in result) and decoder.refresh():
            result = self._decode_items(items, decoder)
        self.stats["list_items"] += len(result)
        return result

    @staticmethod
    def _decode_items(items: list[Any], decoder: SalaryDecoder) -> list[dict[str, Any]]:
        result = []
        for item in items:
            if not isinstance(item, dict) or not item.get("url"):
                continue
            decoded = {key: decode_text(value, decoder.mapping) if isinstance(value, str) else value
                       for key, value in item.items()}
            decoded["job_id"] = job_id_from_url(decoded["url"])
            if not decoded["job_id"]:
                continue
            salary = str(decoded.get("salary") or "")
            if has_private_glyph(salary):
                decoded["salary_unreadable"] = True
                decoded["salary_raw"] = salary
                decoded["salary"] = ""
            else:
                decoded["salary_unreadable"] = False
            result.append(decoded)
        return result

    def _scroll_next_batch(self, tab: CDPTab, loaded_ids: set[str]) -> list[dict[str, Any]]:
        self._reserve_page("search")
        decoder = SalaryDecoder(tab)
        decoder.refresh()
        for _ in range(20):
            tab.evaluate(JS_SCROLL_LIST)
            self._sleep(0.5, 1.0)
            batch = self._read_list(tab, decoder)
            if batch is None:
                continue
            ids = {job_id_from_url(item.get("url", "")) for item in batch}
            if ids - loaded_ids:
                return batch
        return []

    def _add_batch(
        self,
        jobs: dict[str, dict[str, Any]],
        batch: list[dict[str, Any]],
        city: str,
        keyword: str,
        label: str,
    ) -> None:
        for item in batch:
            job_id = str(item.get("job_id") or "")
            if not job_id:
                continue
            existing = jobs.get(job_id)
            if existing is not None:
                existing.setdefault("matched_keywords", [])
                if label not in existing["matched_keywords"]:
                    existing["matched_keywords"].append(label)
                continue
            item = dict(item)
            item["city"] = city
            item["keyword"] = keyword
            item["matched_keywords"] = [label]
            jobs[job_id] = item

    def _attach_details(self, jobs: list[dict[str, Any]], *, max_details: int) -> None:
        if not jobs:
            return
        if len(jobs) > max_details:
            self.warnings.append(f"岗位数 {len(jobs)} 超过本次详情上限 {max_details}，仅补全前 {max_details} 个")
        detail_tab: CDPTab | None = None
        try:
            for job in jobs[:max_details]:
                if job.get("detail_error"):
                    continue
                detail_tab = self._fetch_detail_into(job, detail_tab)
                self._sleep(1.5, 3.0)
        finally:
            if detail_tab is not None:
                self.browser.close(detail_tab)

    def _fetch_detail(self, url: str) -> dict[str, Any]:
        if not str(url or "").startswith("http"):
            raise ToolError("invalid_url", "岗位链接必须是完整的 https://www.zhipin.com/job_detail/... 地址")
        job = {
            "job_id": job_id_from_url(url),
            "url": url,
            "title": "",
            "salary": "",
            "salary_unreadable": False,
            "company": "",
        }
        detail_tab: CDPTab | None = None
        try:
            detail_tab = self._fetch_detail_into(job, None)
        finally:
            if detail_tab is not None:
                self.browser.close(detail_tab)
        return job

    def _fetch_detail_into(self, job: dict[str, Any], detail_tab: CDPTab | None) -> CDPTab | None:
        url = str(job.get("url") or "")
        self._reserve_page("detail")
        opened_new = False
        try:
            if detail_tab is None:
                detail_tab = self.browser.open(url, background=True)
                opened_new = True
                if not detail_tab.wait_for_load(20):
                    self._sleep(1.0, 2.0)
            else:
                if not detail_tab.navigate(url, wait=False, timeout=20):
                    raise ToolError("open_page_failed", f"无法打开岗位详情页: {url}")
                if not detail_tab.wait_for_load(20):
                    self._sleep(1.0, 2.0)
            self._sleep(0.8, 1.5)
            check_page_state(self.config, self.state, detail_tab, allow_login=False)
            if is_closed_page(detail_tab):
                job["detail_error"] = "job_closed"
                job["closed"] = True
                self.stats["detail_failures"] += 1
                return detail_tab
            decoder = SalaryDecoder(detail_tab)
            decoder.refresh()
            raw = detail_tab.evaluate(JS_EXTRACT_DETAIL)
            try:
                detail = json.loads(raw) if isinstance(raw, str) else raw
            except (json.JSONDecodeError, TypeError):
                detail = None
            if not isinstance(detail, dict):
                job["detail_error"] = "detail_parse_failed"
                self.stats["detail_failures"] += 1
                return detail_tab
            for key, value in detail.items():
                if isinstance(value, str):
                    decoded = decode_text(value, decoder.mapping)
                    job[key] = decoded
                else:
                    job[key] = value
            salary = str(job.get("salary") or "")
            if has_private_glyph(salary):
                job["salary_unreadable"] = True
                job["salary_raw"] = salary
                job["salary"] = ""
            else:
                job["salary_unreadable"] = False
            job.pop("detail_error", None)
            self.stats["detail_pages"] += 1
            self.state.reset_failures()
            return detail_tab
        except ToolError:
            if opened_new and detail_tab is not None:
                self.browser.close(detail_tab)
                detail_tab = None
            raise
        except CDPError as exc:
            job["detail_error"] = f"page_error: {exc}"
            self.stats["detail_failures"] += 1
            return detail_tab

    def _reserve_page(self, kind: str) -> None:
        limits = self.config["safety"]
        try:
            self.state.reserve_page(
                kind,
                daily_limit=limits[f"daily_{kind}_page_limit"] if kind in {"search", "detail"} else None,
            )
        except SafetyError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc
        if kind == "search":
            self.stats["search_pages"] += 1

    @staticmethod
    def _page_diagnostics(tab: CDPTab) -> dict[str, Any]:
        raw = tab.evaluate(
            """
            (() => {
                const hasList = Boolean(document.querySelector(
                    '.job-card-wrap, .job-list-container, .rec-job-list, .job-list-box'
                ));
                const text = document.body ? document.body.innerText : '';
                return JSON.stringify({
                    url: location.href,
                    title: document.title || '',
                    has_list_container: hasList,
                    snippet: text.slice(0, 600)
                });
            })()
            """
        )
        try:
            value = json.loads(raw) if isinstance(raw, str) else raw
        except (json.JSONDecodeError, TypeError):
            return {}
        return value if isinstance(value, dict) else {}

    def _sleep(self, minimum: float, maximum: float) -> None:
        time.sleep(self._random.uniform(minimum, maximum))


def search_jobs(
    config: dict[str, Any],
    state: SafetyState,
    *,
    keywords: list[str] | None = None,
    cities: list[str] | None = None,
    max_pages: int | None = None,
    filters: dict[str, Any] | None = None,
    sort: str | None = None,
    include_details: bool = False,
    max_details: int = 60,
) -> dict[str, Any]:
    searcher = BossSearcher(config, state)
    return searcher.search(
        keywords=keywords,
        cities=cities,
        max_pages=max_pages,
        filters=filters,
        sort=sort,
        include_details=include_details,
        max_details=max_details,
    )


def get_job_detail(config: dict[str, Any], state: SafetyState, url: str) -> dict[str, Any]:
    searcher = BossSearcher(config, state)
    return searcher.detail(url)


def check_login(config: dict[str, Any], state: SafetyState) -> dict[str, Any]:
    browser = Browser(config)
    tab: CDPTab | None = None
    try:
        try:
            state.reserve_page("chat")
        except SafetyError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc
        tab = browser.open(CHAT_URL, background=True)
        if not tab.wait_for_load(20):
            time.sleep(1.5)
        risk = read_risk(tab)
        if risk and risk["kind"] != "login_required":
            minutes = int(config["safety"]["risk_lock_minutes"])
            state.lock(risk["kind"], minutes)
            raise ToolError(
                "platform_risk",
                f"BOSS 返回了 {risk['kind']} 风险提示",
                risk=risk["kind"],
                evidence=risk.get("evidence", ""),
                lock_minutes=minutes,
            )
        raw = tab.evaluate(JS_VISIBLE_LOGIN)
        try:
            info = json.loads(raw) if isinstance(raw, str) else raw
        except (json.JSONDecodeError, TypeError):
            info = {}
        if not isinstance(info, dict):
            info = {}
        logged_in = bool(info.get("logged_in")) and not (risk and risk["kind"] == "login_required")
        return {
            "logged_in": logged_in,
            "url": tab.current_url(),
            "title": str(info.get("title") or ""),
            "risk": risk,
            "profile_dir": str(config["browser"]["user_data_dir"]),
            "login_url": "https://www.zhipin.com/web/user/",
        }
    finally:
        if tab is not None:
            browser.close(tab)
