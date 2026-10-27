
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
    if (/captcha|security-check|\/verify/i.test(url)) return JSON.stringify({risk: 'captcha', evidence: 'captcha_url'});
    if (/\/web\/user\/(?:login)?(?:\?|$)/i.test(url) || /\/login(?:\?|$)/i.test(url)) {
        return JSON.stringify({risk: 'login_required', evidence: 'login_url'});
    }
    const pageUrl = new URL(url);
    const blockedPath = /(?:^|\/)(?:403|forbidden|access-denied)(?:\/|$)/i.test(pageUrl.pathname);
    const blockedStatus = Array.from(pageUrl.searchParams).some(([key, value]) =>
        /^(?:code|status|error|error_code|status_code)$/i.test(key)
        && /^(?:403|forbidden|access-denied)$/i.test(value)
    );
    if (blockedPath || blockedStatus) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_url'});
    }
    if (/^(?:403(?:\s+forbidden)?|forbidden|access denied|访问被拒绝|账号异常|账号受限)/i.test(title.trim())) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_title'});
    }
    if (!hasExpectedContent && /验证码|安全验证|完成验证/.test(text)) {
        return JSON.stringify({risk: 'captcha', evidence: 'captcha_page'});
    }
    if (!hasExpectedContent && /(?:^|\n)\s*403(?:\s+forbidden)?\s*(?:\n|$)|访问被拒绝|账号异常|账号受限/i.test(text)) {
        return JSON.stringify({risk: 'blocked', evidence: 'blocked_page'});
    }
    if (!hasExpectedContent && /操作频繁|访问频繁|请求频繁|稍后再试|频率限制/.test(text)) {
        return JSON.stringify({risk: 'rate_limit', evidence: 'rate_limit_page'});
    }
    return JSON.stringify({risk: null});
})()
