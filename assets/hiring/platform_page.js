(() => {
  const platform = __PLATFORM__;
  const visible = e => !!e && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden';
  const text = e => (e?.innerText || e?.textContent || '').trim();
  const first = (root, selectors) => { for (const s of selectors) { const e = root.querySelector(s); if (e && text(e)) return text(e); } return ''; };
  const config = {
    '51job': {
      link: 'a[href*="jobs.51job.com/"]', card: '.joblist-item, .j_joblist > div, .e, .job-item',
      title: ['h1', '.job-title', '.jobname'], company: ['.com_msg a', '.cname a', '.company-name', '.companyName'],
      jd: ['.job_msg', '.job-detail', '.job-description', '.bmsg.job_msg'],
      listTitle: ['.jname', '.joblist-item-job', '.job-name'], listCompany: ['.cname', '.joblist-item-company', '.company-name']
    },
    zhaopin: {
      link: 'a[href*="jobs.zhaopin.com/"]', card: '.joblist-box__item, .joblist-box-item, .job-item',
      title: ['h1', '.summary-plane__title'], company: ['.company__title', '.company-name', '.companyinfo__name'],
      jd: ['.describtion__detail-content', '.job-description', '.job-detail'],
      listTitle: ['.jobinfo__name', '.job-name'], listCompany: ['.companyinfo__name', '.company-name']
    },
    shixiseng: {
      link: 'a[href*="/intern/"], a[href*="/job/"]', card: '.intern-wrap, .intern-item, .job-item',
      title: ['h1', '.new_job_name', '.job-name'], company: ['.com-name', '.company-name', '.company_name'],
      jd: ['.job_detail', '.job-detail', '.job-description'],
      listTitle: ['.intern-name', '.job-name'], listCompany: ['.company-name', '.company_name']
    }
  }[platform];
  const body = document.body?.innerText || '';
  const dialogs = [...document.querySelectorAll('[role="dialog"], .login-modal, .login-dialog, .geetest_panel, .nc-container, .captcha')].filter(visible);
  const captcha = dialogs.some(e => /滑块|安全验证|拖动|验证后继续/.test(text(e))) || /访问过于频繁|操作过于频繁|访问异常|请完成安全验证|请拖动滑块/.test(body);
  const login = dialogs.some(e => /登录|注册/.test(text(e)) && !!e.querySelector('input,iframe')) || /\/(login|passport|signin)(\/|\?|$)/i.test(location.href);
  const loggedIn = [...document.querySelectorAll('a,button,[class*="user-name"],[class*="username"],[class*="avatar"]')].some(e => visible(e) && (/^(退出|退出登录|我的简历|个人中心)$/.test(text(e)) || /avatar/.test(e.className || '')));
  const jobs = [], seen = new Set();
  for (const a of document.querySelectorAll(config.link)) {
    if (!visible(a)) continue;
    const url = a.href.split(/[?#]/)[0];
    if (seen.has(url)) continue;
    const card = a.closest(config.card) || a.parentElement?.parentElement || a;
    const title = first(card, config.listTitle) || text(a);
    if (!title || title.length > 160) continue;
    seen.add(url);
    jobs.push({url, title, company:first(card, config.listCompany), salary:first(card, ['.sal', '.jobinfo__salary', '.daymoney', '.salary', '.job-salary']), location:first(card, ['.d', '.jobinfo__other-info-item', '.city', '.job-area'])});
  }
  let structured = null;
  function findJob(v) {
    if (!v || typeof v !== 'object') return;
    if (Array.isArray(v)) { v.forEach(findJob); return; }
    if (v['@type'] === 'JobPosting') structured = v;
    if (v['@graph']) findJob(v['@graph']);
  }
  for (const e of document.querySelectorAll('script[type="application/ld+json"]')) { try { findJob(JSON.parse(e.textContent)); } catch {} }
  const cleanHtml = s => { const d = document.createElement('div'); d.innerHTML = s || ''; return text(d); };
  const job = {
    url:location.href, title:first(document, config.title) || structured?.title || '',
    company:first(document, config.company) || structured?.hiringOrganization?.name || '',
    jd:first(document, config.jd) || cleanHtml(structured?.description),
    salary:first(document, ['.sal', '.summary-plane__salary', '.job_money', '.salary']),
    location:first(document, ['.summary-plane__info', '.job_position', '.lname', '.job-area']),
    education:first(document, ['.job_academic', '.education']), experience:first(document, ['.experience'])
  };
  const labels = ['申请职位','立即申请','立即投递','投递简历','申请该职位','投个简历','确认投递','提交申请'];
  const buttons = [...document.querySelectorAll('button,a,[role="button"],input[type="submit"], .apply, .deliver, .job-apply, .aox, .summary-plane__apply, .job-apply-button')]
    .filter(e => visible(e) && !e.disabled && e.getAttribute('aria-disabled') !== 'true' && labels.includes(text(e) || e.value));
  // Nested labels inside a button represent one control.
  const controls = buttons.filter(e => !buttons.some(other => other !== e && other.contains(e)));
  controls.forEach((e,i) => e.setAttribute('data-pd-platform-apply', String(i)));
  const alerts = [...document.querySelectorAll('[role="alert"], .el-message, .ant-message, .toast, .apply-success, .deliver-success, [class*="success"]')].filter(visible).map(text);
  const applied = [...document.querySelectorAll('button,a,[role="button"],.apply,.deliver,.summary-plane__apply')].filter(visible).some(e => /^(已申请|已投递|投递成功|申请成功)$/.test(text(e)));
  const success = applied || alerts.some(t => /投递成功|申请成功|简历已成功/.test(t));
  const unavailable = /该职位已下线|职位已停止招聘|该职位已关闭|职位不存在|该岗位已下线/.test(body);
  const truncated = /登录后查看完整|登录查看完整|登录后可查看/.test(body);
  return {url:location.href,job,jobs,captcha,login,logged_in:loggedIn,success,unavailable,truncated,
    empty:/暂无相关职位|没有找到相关职位|未找到相关职位|暂无搜索结果|暂无职位/.test(body),
    buttons:controls.map((e,i)=>({id:i,label:text(e)||e.value})),
    // Used locally for the preflight digest only; never returned to MCP.
    fields:[...document.querySelectorAll('input,select,textarea')].filter(visible).filter(e=>e.type!=='password').map(e=>[e.name,e.type,e.type==='file'?[...e.files].map(f=>[f.name,f.size,f.lastModified]):e.value,e.checked])};
})()
