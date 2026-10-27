
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const editor = document.querySelector('.chat-editor, .chat-container, .chat-input');
    const scope = editor || document;
    const nodes = Array.from(scope.querySelectorAll(
        'button, [role="button"], a, [class*="toolbar"] span, [class*="icon"], [class*="upload"]'
    ));
    const describe = (element) => {
        const text = (element.innerText || element.textContent || '').trim();
        const title = element.getAttribute('title') || '';
        const aria = element.getAttribute('aria-label') || '';
        const klass = String(element.className || '');
        const haystack = [text, title, aria, klass].join(' ');
        let score = 0;
        if (/图片|相册|照片/.test(haystack)) score += 200;
        if (/image|photo|picture|album|upload/i.test(haystack)) score += 120;
        if (/toolbar/i.test(klass)) score += 30;
        if (text.length > 8) score -= 40;
        return {element, text, title, aria, klass: klass.slice(0, 160), score};
    };
    const candidates = nodes.filter((element) => {
        if (!visible(element)) return false;
        const text = (element.innerText || element.textContent || '').trim();
        if (text.length > 12) return false;
        return true;
    }).map(describe).filter((item) => item.score > 0).sort((a, b) => b.score - a.score);
    document.querySelectorAll('[data-bosshunter-image-trigger]').forEach((element) => {
        element.removeAttribute('data-bosshunter-image-trigger');
    });
    const best = candidates[0];
    if (best) best.element.setAttribute('data-bosshunter-image-trigger', '1');
    return JSON.stringify({
        found: !!best,
        candidates: candidates.slice(0, 12).map((item) => ({
            text: item.text,
            title: item.title,
            aria: item.aria,
            klass: item.klass,
            score: item.score
        }))
    });
})()
