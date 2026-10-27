
(() => {
    const selectors = [
        'a[redirect-url*="/web/geek/chat"]',
        'a[data-url*="/friend/add"]',
        'a.btn-startchat',
        '[ka="job_detail_chat"]',
        '[ka^="go_chat"]',
        '[ka*="gochat"]',
        '.op-btn-chat',
        '.btn-startchat-wrap'
    ];
    const candidates = selectors.flatMap((selector, priority) =>
        Array.from(document.querySelectorAll(selector)).map((el) => ({el, selector, priority}))
    );
    const elementState = (el) => {
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        const visible = !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
        const inViewport = visible && rect.bottom > 0 && rect.right > 0
            && rect.top < innerHeight && rect.left < innerWidth;
        const x = Math.min(Math.max(rect.x + rect.width / 2, 0), innerWidth - 1);
        const y = Math.min(Math.max(rect.y + rect.height / 2, 0), innerHeight - 1);
        const top = inViewport ? document.elementFromPoint(x, y) : null;
        const topmost = !!(top && (top === el || el.contains(top)));
        return {visible, inViewport, topmost};
    };
    const score = (item) => {
        const el = item.el;
        const text = (el.innerText || el.textContent || '').trim();
        const ka = el.getAttribute('ka') || '';
        const redirectUrl = el.getAttribute('redirect-url') || '';
        const dataUrl = el.getAttribute('data-url') || '';
        const tagName = String(el.tagName || '').toLowerCase();
        let value = 0;
        const state = elementState(el);
        if (state.visible) value += 1000;
        if (state.inViewport) value += 500;
        if (state.topmost) value += 300;
        if (tagName === 'a') value += 200;
        if (redirectUrl.includes('/web/geek/chat')) value += 300;
        if (dataUrl.includes('/friend/add')) value += 250;
        if (el.classList && el.classList.contains('btn-startchat')) value += 120;
        if (text.includes('沟通')) value += 80;
        if (ka === 'job_detail_chat' || ka.includes('go_chat') || ka.includes('gochat')) value += 60;
        if (el.classList && el.classList.contains('btn-startchat-wrap')) value -= 100;
        return value - item.priority;
    };
    const matches = candidates.filter((item) => {
        const el = item.el;
        const text = (el.innerText || el.textContent || '').trim();
        const ka = el.getAttribute('ka') || '';
        return text.includes('沟通')
            || (el.getAttribute('redirect-url') || '').includes('/web/geek/chat')
            || (el.getAttribute('data-url') || '').includes('/friend/add')
            || ka === 'job_detail_chat'
            || ka.includes('go_chat')
            || ka.includes('gochat');
    }).sort((a, b) => score(b) - score(a));
    const btn = matches[0] && matches[0].el;
    if (!btn) {
        return JSON.stringify({
            success: false,
            error: 'no_chat_button',
            candidates: candidates.map((item) => ({
                text: (item.el.innerText || item.el.textContent || '').trim(),
                ka: item.el.getAttribute('ka'),
                className: String(item.el.className || ''),
                tagName: item.el.tagName,
                visible: elementState(item.el).visible
            }))
        });
    }
    btn.scrollIntoView({block: 'center', inline: 'center'});
    const rect = btn.getBoundingClientRect();
    btn.click();
    return JSON.stringify({
        success: true,
        interaction: 'dom_click',
        x: rect.x + rect.width / 2,
        y: rect.y + rect.height / 2,
        button_text: (btn.innerText || btn.textContent || '').trim(),
        klass: String(btn.className || ''),
        redirectUrl: btn.getAttribute('redirect-url'),
        dataUrl: btn.getAttribute('data-url')
    });
})()
